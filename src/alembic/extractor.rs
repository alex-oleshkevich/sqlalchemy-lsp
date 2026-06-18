#![allow(dead_code)]

use tree_sitter::{Node, Tree};

use crate::parsing::python::{node_text, ts_range};

use super::{ColumnRef, DownRevision, MigrationFile, OpCall, TableRef};

// ── Public entry point ────────────────────────────────────────────────────────

/// Extract migration metadata from a parsed Alembic migration file.
/// Returns `None` only if the file has no recognisable Alembic content.
pub fn extract_migration(source: &str, tree: &Tree) -> Option<MigrationFile> {
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let mut revision: Option<String> = None;
    let mut revision_range = None;
    let mut down_revision = DownRevision::None;
    let mut down_revision_range = None;
    let mut message: Option<String> = None;
    let mut op_calls: Vec<OpCall> = Vec::new();

    // Walk top-level statements.
    // In tree-sitter-python 0.25, assignments and expressions are wrapped in
    // `expression_statement` at every level — unwrap to get the real node.
    let mut c = root.walk();
    for outer in root.named_children(&mut c) {
        let node = unwrap_expr_stmt(outer);
        match node.kind() {
            "string" if message.is_none() => {
                let s = strip_string_quotes(node_text(node, bytes));
                let first_line = s.lines().next().unwrap_or("").trim().to_string();
                if !first_line.is_empty() {
                    message = Some(first_line);
                }
            }
            "assignment" => {
                let lhs = node.child_by_field_name("left");
                let rhs = node.child_by_field_name("right");
                let name = lhs.map(|n| node_text(n, bytes)).unwrap_or("");
                match name {
                    "revision" => {
                        if let Some(rhs_node) = rhs {
                            revision = extract_string_value(&rhs_node, bytes);
                            revision_range = Some(ts_range(rhs_node));
                        }
                    }
                    "down_revision" => {
                        if let Some(rhs_node) = rhs {
                            down_revision_range = Some(ts_range(rhs_node));
                            down_revision = extract_down_revision(&rhs_node, bytes);
                        }
                    }
                    _ => {}
                }
            }
            // `def upgrade(): ...` or `def downgrade(): ...`
            // function_definition is a direct child (not wrapped in expression_statement)
            "function_definition" => {
                let fn_name = node
                    .child_by_field_name("name")
                    .map(|n| node_text(n, bytes))
                    .unwrap_or("");
                if matches!(fn_name, "upgrade" | "downgrade") {
                    if let Some(body) = node.child_by_field_name("body") {
                        collect_op_calls(&body, bytes, &mut op_calls);
                    }
                }
            }
            _ => {}
        }
    }

    // Only return Some if we found at least a revision ID or op calls
    if revision.is_none() && op_calls.is_empty() {
        return None;
    }

    Some(MigrationFile {
        revision,
        down_revision,
        message,
        revision_range,
        down_revision_range,
        op_calls,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Unwrap an `expression_statement` to its inner node.
fn unwrap_expr_stmt(node: Node) -> Node {
    if node.kind() == "expression_statement" {
        node.named_child(0).unwrap_or(node)
    } else {
        node
    }
}

fn extract_string_value(node: &Node, source: &[u8]) -> Option<String> {
    if node.kind() == "string" {
        let text = node_text(*node, source);
        return Some(strip_string_quotes(text).to_string());
    }
    None
}

fn strip_string_quotes(s: &str) -> &str {
    for delim in &["\"\"\"", "'''"] {
        if s.starts_with(delim) && s.ends_with(delim) {
            return s[delim.len()..s.len() - delim.len()].trim();
        }
    }
    s.trim_matches('"').trim_matches('\'')
}

/// Parse `down_revision = None | "id" | ("id1", "id2")`.
fn extract_down_revision(rhs: &Node, source: &[u8]) -> DownRevision {
    match rhs.kind() {
        "none" => DownRevision::None,
        "string" => {
            let text = strip_string_quotes(node_text(*rhs, source)).to_string();
            DownRevision::Single(text)
        }
        "tuple" => {
            let mut ids = Vec::new();
            let mut c = rhs.walk();
            for child in rhs.named_children(&mut c) {
                if child.kind() == "string" {
                    ids.push(strip_string_quotes(node_text(child, source)).to_string());
                }
            }
            if ids.is_empty() {
                DownRevision::None
            } else {
                DownRevision::Multiple(ids)
            }
        }
        _ => DownRevision::None,
    }
}

/// Collect all `op.*` calls recursively within a function body.
fn collect_op_calls(body: &Node, source: &[u8], out: &mut Vec<OpCall>) {
    let mut c = body.walk();
    for stmt in body.named_children(&mut c) {
        match stmt.kind() {
            "expression_statement" => {
                if let Some(expr) = stmt.named_child(0) {
                    if let Some(op) = try_parse_op_call(&expr, source) {
                        out.push(op);
                    }
                }
            }
            "with_statement" | "if_statement" | "block" | "try_statement" => {
                collect_op_calls(&stmt, source, out);
            }
            _ => {}
        }
    }
}

/// Try to parse a `op.create_table(...)`, `op.add_column(...)`, etc. call.
fn try_parse_op_call(node: &Node, source: &[u8]) -> Option<OpCall> {
    if node.kind() != "call" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    let func_text = node_text(func, source);

    // Must be `op.<something>`
    if !func_text.starts_with("op.") {
        return None;
    }
    let operation = func_text[3..].to_string();
    let full_range = ts_range(*node);

    let args_node = node.child_by_field_name("arguments")?;
    let (table_name, column_name, null_constraint_name_range) =
        extract_op_args(&operation, &args_node, source, full_range);

    Some(OpCall { operation, full_range, table_name, column_name, null_constraint_name_range })
}

/// Ops whose first positional arg is a constraint name (not a table name).
const CONSTRAINT_OPS: &[&str] =
    &["drop_constraint", "create_foreign_key", "create_unique_constraint", "create_check_constraint"];

/// Extract table, column, and null-constraint-name info from an op call's argument list.
fn extract_op_args(
    operation: &str,
    args: &Node,
    source: &[u8],
    call_range: crate::model::types::Range,
) -> (Option<TableRef>, Option<ColumnRef>, Option<crate::model::types::Range>) {
    let mut c = args.walk();
    let positional: Vec<Node> = args
        .named_children(&mut c)
        .filter(|n| n.kind() != "keyword_argument")
        .collect();

    let is_constraint_op = CONSTRAINT_OPS.contains(&operation);

    // For constraint ops the first positional is the constraint name, second is the table.
    // For all other ops the first positional is the table.
    let (table_idx, constraint_name_idx) =
        if is_constraint_op { (1, Some(0)) } else { (0, None) };

    let table = positional.get(table_idx).and_then(|n| {
        if n.kind() == "string" {
            let name = strip_string_quotes(node_text(*n, source)).to_string();
            Some(TableRef { name, range: ts_range(*n) })
        } else {
            None
        }
    });

    // For add_column / drop_column / alter_column: second arg (after table) is column name
    let col_idx = table_idx + 1;
    let column = if matches!(operation, "add_column" | "drop_column" | "alter_column") {
        positional.get(col_idx).and_then(|n| {
            if n.kind() == "string" {
                let name = strip_string_quotes(node_text(*n, source)).to_string();
                Some(ColumnRef { name, range: ts_range(*n) })
            } else {
                None
            }
        })
    } else {
        None
    };

    // Detect a null or absent constraint name.
    let null_constraint_name_range = if let Some(ci) = constraint_name_idx {
        match positional.get(ci) {
            Some(n) if n.kind() == "none" => Some(ts_range(*n)),
            Some(_) => None, // non-None literal — name is present, skip
            None => Some(call_range), // constraint name absent — fire on the call
        }
    } else {
        None
    };

    (table, column, null_constraint_name_range)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("load python grammar");
        parser.parse(source, None).expect("parse")
    }

    const MIGRATION_A: &str = r#"
"""initial schema"""
from alembic import op
import sqlalchemy as sa

revision = "a1b2c3d4"
down_revision = None
branch_labels = None
depends_on = None

def upgrade() -> None:
    op.create_table(
        "users",
        sa.Column("id", sa.Integer, primary_key=True),
        sa.Column("email", sa.String(200), nullable=False),
    )

def downgrade() -> None:
    op.drop_table("users")
"#;

    const MIGRATION_B: &str = r#"
"""add posts

Revision ID: b2c3d4e5
"""
from alembic import op
import sqlalchemy as sa

revision = "b2c3d4e5"
down_revision = "a1b2c3d4"

def upgrade() -> None:
    op.create_table("posts",
        sa.Column("id", sa.Integer, primary_key=True),
    )
    op.add_column("posts", sa.Column("title", sa.String))

def downgrade() -> None:
    op.drop_table("posts")
"#;

    const MERGE_MIGRATION: &str = r#"
"""merge heads"""
from alembic import op

revision = "c1c1c1c1"
down_revision = ("a1b2c3d4", "b2c3d4e5")

def upgrade() -> None:
    pass

def downgrade() -> None:
    pass
"#;

    #[test]
    fn extract_revision_and_message() {
        let tree = parse(MIGRATION_A);
        let mf = extract_migration(MIGRATION_A, &tree).expect("migration extracted");
        assert_eq!(mf.revision.as_deref(), Some("a1b2c3d4"));
        assert_eq!(mf.message.as_deref(), Some("initial schema"));
    }

    #[test]
    fn extract_down_revision_none() {
        let tree = parse(MIGRATION_A);
        let mf = extract_migration(MIGRATION_A, &tree).unwrap();
        assert!(matches!(mf.down_revision, DownRevision::None));
    }

    #[test]
    fn extract_down_revision_single() {
        let tree = parse(MIGRATION_B);
        let mf = extract_migration(MIGRATION_B, &tree).unwrap();
        assert!(matches!(
            mf.down_revision,
            DownRevision::Single(ref s) if s == "a1b2c3d4"
        ));
    }

    #[test]
    fn extract_down_revision_multiple() {
        let tree = parse(MERGE_MIGRATION);
        let mf = extract_migration(MERGE_MIGRATION, &tree).unwrap();
        assert!(matches!(mf.down_revision, DownRevision::Multiple(ref v) if v.len() == 2));
    }

    #[test]
    fn extract_op_calls() {
        let tree = parse(MIGRATION_A);
        let mf = extract_migration(MIGRATION_A, &tree).unwrap();
        assert!(!mf.op_calls.is_empty());
        let create = mf.op_calls.iter().find(|o| o.operation == "create_table");
        assert!(create.is_some());
        assert_eq!(create.unwrap().table_name.as_ref().map(|t| t.name.as_str()), Some("users"));
    }

    #[test]
    fn extract_drop_table() {
        let tree = parse(MIGRATION_A);
        let mf = extract_migration(MIGRATION_A, &tree).unwrap();
        let drop = mf.op_calls.iter().find(|o| o.operation == "drop_table");
        assert!(drop.is_some());
        assert_eq!(drop.unwrap().table_name.as_ref().map(|t| t.name.as_str()), Some("users"));
    }

    #[test]
    fn message_first_line_only() {
        let tree = parse(MIGRATION_B);
        let mf = extract_migration(MIGRATION_B, &tree).unwrap();
        assert_eq!(mf.message.as_deref(), Some("add posts"));
    }

    #[test]
    fn none_returns_for_plain_python() {
        let src = "x = 1\ndef foo(): pass\n";
        let tree = parse(src);
        assert!(extract_migration(src, &tree).is_none());
    }
}
