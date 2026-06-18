/// Completions for SQLAlchemy model files and Alembic migration files.
///
/// Two trigger contexts:
///   1. `op.` in a migration file  → the Alembic operations list (REQ-ALM-06)
///   2. First string arg of `op.*` → table names from the index (REQ-ALM-07)
///   3. Later string arg of `op.*` → column names for the resolved table (REQ-ALM-07)
use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind, Position,
};

use crate::state::WorkspaceState;

// ── Static op.* catalogue ─────────────────────────────────────────────────────

struct OpInfo {
    name: &'static str,
    doc: &'static str,
    snippet: &'static str,
}

const OP_CATALOGUE: &[OpInfo] = &[
    OpInfo {
        name: "add_column",
        doc: "Add a column to an existing table.",
        snippet: "add_column(\"${1:table}\", sa.Column(\"${2:name}\", sa.${3:String}))",
    },
    OpInfo {
        name: "drop_column",
        doc: "Drop a column from a table.",
        snippet: "drop_column(\"${1:table}\", \"${2:column}\")",
    },
    OpInfo {
        name: "alter_column",
        doc: "Alter a column's type or constraints.",
        snippet: "alter_column(\"${1:table}\", \"${2:column}\", nullable=${3:True})",
    },
    OpInfo {
        name: "create_table",
        doc: "Create a new table.",
        snippet: "create_table(\n    \"${1:table_name}\",\n    sa.Column(\"id\", sa.Integer, primary_key=True),\n)",
    },
    OpInfo {
        name: "drop_table",
        doc: "Drop a table.",
        snippet: "drop_table(\"${1:table}\")",
    },
    OpInfo {
        name: "create_index",
        doc: "Create a new index.",
        snippet: "create_index(\"${1:name}\", \"${2:table}\", [\"${3:column}\"])",
    },
    OpInfo {
        name: "drop_index",
        doc: "Drop an index.",
        snippet: "drop_index(\"${1:name}\", table_name=\"${2:table}\")",
    },
    OpInfo {
        name: "create_unique_constraint",
        doc: "Add a unique constraint to a table.",
        snippet: "create_unique_constraint(\"${1:name}\", \"${2:table}\", [\"${3:column}\"])",
    },
    OpInfo {
        name: "drop_constraint",
        doc: "Drop a named constraint from a table.",
        snippet: "drop_constraint(\"${1:name}\", \"${2:table}\", type_=\"${3:unique}\")",
    },
    OpInfo {
        name: "create_foreign_key",
        doc: "Add a foreign-key constraint.",
        snippet: "create_foreign_key(\"${1:name}\", \"${2:src_table}\", \"${3:ref_table}\", [\"${4:local_col}\"], [\"${5:remote_col}\"])",
    },
    OpInfo {
        name: "create_check_constraint",
        doc: "Add a CHECK constraint.",
        snippet: "create_check_constraint(\"${1:name}\", \"${2:table}\", ${3:condition})",
    },
    OpInfo {
        name: "rename_table",
        doc: "Rename a table.",
        snippet: "rename_table(\"${1:old_name}\", \"${2:new_name}\")",
    },
    OpInfo {
        name: "execute",
        doc: "Execute arbitrary SQL.",
        snippet: "execute(\"${1:sql}\")",
    },
    OpInfo {
        name: "bulk_insert",
        doc: "Bulk-insert rows into a table.",
        snippet: "bulk_insert(${1:table}, [${2:rows}])",
    },
];

fn op_items() -> Vec<CompletionItem> {
    OP_CATALOGUE
        .iter()
        .map(|op| CompletionItem {
            label: op.name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: op.doc.to_string(),
            })),
            insert_text: Some(op.snippet.to_string()),
            insert_text_format: Some(
                tower_lsp_server::ls_types::InsertTextFormat::SNIPPET,
            ),
            ..Default::default()
        })
        .collect()
}

// ── Context detection ─────────────────────────────────────────────────────────

/// Returns the text of the line up to (not including) `col`.
fn line_prefix(source: &str, line: u32, col: u32) -> &str {
    let target = line as usize;
    let col = col as usize;
    for (i, l) in source.lines().enumerate() {
        if i == target {
            return &l[..col.min(l.len())];
        }
    }
    ""
}

/// Detect the trigger context at `pos` in `source`.
enum Context {
    /// Cursor follows `op.` — offer the operations list.
    OpDot,
    /// Cursor is inside the first string arg of a known `op.*` call — offer table names.
    OpTableArg,
    /// Cursor is inside a column arg — offer columns of `table`.
    OpColumnArg { table: String },
    /// No Alembic completion context.
    None,
}

fn detect_context(source: &str, pos: Position) -> Context {
    let prefix = line_prefix(source, pos.line, pos.character);
    let trimmed = prefix.trim_end();

    // `op.` trigger
    if trimmed.ends_with("op.") {
        return Context::OpDot;
    }

    // Inside an `op.*("..."` call — look for  op.xxx(" pattern on the same line
    // up to the cursor.
    //
    // Simple heuristic: scan for `op.<name>` then count the string arguments
    // that have been started (opened with `"` or `'`).
    if let Some(op_start) = prefix.find("op.") {
        let after_op = &prefix[op_start + 3..];
        // Extract the operation name
        let op_name: String = after_op
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if op_name.is_empty() {
            return Context::None;
        }
        let after_name = &after_op[op_name.len()..];
        // Count positional string arguments opened so far (simplified: count opening quotes)
        // This is a heuristic — real arg counting needs an AST, but for completion this
        // is sufficient for the two-argument case.
        let open_parens = after_name.chars().filter(|c| *c == '(').count();
        if open_parens == 0 {
            return Context::None;
        }
        // Count completed string args by counting pairs of quotes
        let arg_idx = count_completed_string_args(after_name);
        match arg_idx {
            0 => return Context::OpTableArg,
            1 => {
                // Try to extract the table from the first arg
                if let Some(table) = extract_first_string_arg(after_name) {
                    return Context::OpColumnArg { table };
                }
            }
            _ => {}
        }
    }

    Context::None
}

fn count_completed_string_args(text: &str) -> usize {
    let mut count = 0;
    let mut in_string = false;
    let mut quote_char = ' ';
    for ch in text.chars() {
        if !in_string {
            if ch == '"' || ch == '\'' {
                in_string = true;
                quote_char = ch;
            }
        } else if ch == quote_char {
            in_string = false;
            count += 1;
        }
    }
    count / 2 // each complete arg has one open + one close quote
}

fn extract_first_string_arg(text: &str) -> Option<String> {
    let start = text.find('"').or_else(|| text.find('\''))?;
    let quote = text.chars().nth(start)?;
    let rest = &text[start + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Produce completion items for the given position in `source`.
///
/// Returns `None` when this position has no SA/Alembic completion context.
pub fn provide_completions(
    uri: &tower_lsp_server::ls_types::Uri,
    source: &str,
    pos: Position,
    state: &WorkspaceState,
) -> Option<Vec<CompletionItem>> {
    // Only fire in Alembic context.
    if !state.migration_files.contains_key(uri) {
        return None;
    }

    match detect_context(source, pos) {
        Context::OpDot => Some(op_items()),
        Context::OpTableArg => {
            let items: Vec<CompletionItem> = state
                .table_index
                .iter()
                .map(|e| CompletionItem {
                    label: e.key().clone(),
                    kind: Some(CompletionItemKind::VALUE),
                    ..Default::default()
                })
                .collect();
            if items.is_empty() { None } else { Some(items) }
        }
        Context::OpColumnArg { table } => {
            let model_name = state.table_index.get(&table)?;
            let loc = state.model_index.get(&*model_name)?;
            let uri = loc.uri.clone();
            drop(loc);
            let file_models = state.file_models.get(&uri)?;
            let model = file_models.iter().find(|m| m.table_name.as_deref() == Some(&table))?;
            let items: Vec<CompletionItem> = model
                .columns
                .keys()
                .map(|col| CompletionItem {
                    label: col.clone(),
                    kind: Some(CompletionItemKind::FIELD),
                    ..Default::default()
                })
                .collect();
            if items.is_empty() { None } else { Some(items) }
        }
        Context::None => None,
    }
}
