/// F03 — Context-aware completions for SQLAlchemy and Alembic constructs.
///
/// Fires only inside recognized SA/Alembic call sites (the companion gate, REQ-CMP-15).
/// Uses tree-sitter to classify the cursor's context and then returns the matching item set.
use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
    Position, Uri,
};

use crate::{parsing::python::node_text, state::WorkspaceState};

// ── Constants ─────────────────────────────────────────────────────────────────

/// The relationship kwargs we complete (REQ-CMP-06).
const REL_KWARGS: &[(&str, &str, &str)] = &[
    (
        "back_populates",
        "back_populates=\"$1\"",
        "Reverse relationship name",
    ),
    ("lazy", "lazy=\"$1\"", "Loading strategy"),
    ("uselist", "uselist=$1", "Whether to return a list"),
    ("secondary", "secondary=$1", "Association table"),
    ("cascade", "cascade=\"$1\"", "Cascade rules"),
    ("order_by", "order_by=$1", "Default sort"),
    (
        "foreign_keys",
        "foreign_keys=[$1]",
        "Disambiguate FK columns",
    ),
    ("viewonly", "viewonly=$1", "Read-only relationship"),
    ("primaryjoin", "primaryjoin=$1", "Custom join condition"),
    (
        "secondaryjoin",
        "secondaryjoin=$1",
        "Secondary join condition",
    ),
];

const LAZY_VALUES: &[(&str, &str)] = &[
    ("select", "Load on first access (default)"),
    ("joined", "Eager load via JOIN"),
    ("subquery", "Eager load via subquery"),
    ("selectin", "Eager load via SELECT IN"),
    ("raise", "Raise on access"),
    ("raise_on_sql", "Raise only when SQL is emitted"),
    ("write_only", "Write-only collection (SA 2.0)"),
    ("dynamic", "Dynamic query (SA 1.x legacy)"),
    ("noload", "Never load"),
];

const CASCADE_TOKENS: &[(&str, &str)] = &[
    ("save-update", "Cascade save/update"),
    ("merge", "Cascade merge"),
    ("expunge", "Cascade expunge"),
    ("delete", "Cascade delete"),
    ("delete-orphan", "Cascade delete orphans"),
    ("refresh-expire", "Cascade refresh/expire"),
    ("all", "All standard cascades"),
    (
        "all, delete-orphan",
        "All + orphan deletion (common combination)",
    ),
    ("save-update, merge", "Default subset"),
];

/// The `mapped_column()` kwargs (REQ-CMP-11).
const MC_KWARGS: &[(&str, &str, &str)] = &[
    ("primary_key", "primary_key=True", "Mark as primary key"),
    ("nullable", "nullable=$1", "Allow NULL values"),
    ("unique", "unique=True", "Unique constraint"),
    ("index", "index=True", "Create an index"),
    ("default", "default=$1", "Python-side default"),
    (
        "server_default",
        "server_default=$1",
        "Database-side default",
    ),
    ("name", "name=\"$1\"", "Override column name in DB"),
    ("type_", "type_=$1", "Explicit SA type"),
    (
        "ForeignKey",
        "ForeignKey(\"${1:table.col}\")",
        "Add a foreign key",
    ),
];

// ── Snippet catalogue (REQ-CMP-14) ────────────────────────────────────────────

struct Snippet {
    prefix: &'static str,
    label: &'static str,
    body: &'static str,
    root_only: bool,
    class_only: bool,
}

const SNIPPETS: &[Snippet] = &[
    Snippet {
        prefix: "sa",
        label: "saimport",
        body: "import sqlalchemy as sa\nfrom sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship",
        root_only: true,
        class_only: false,
    },
    Snippet {
        prefix: "sabase",
        label: "sabase",
        body: "class Base(DeclarativeBase):\n    pass",
        root_only: true,
        class_only: false,
    },
    Snippet {
        prefix: "samodel",
        label: "samodel",
        body: "class ${1:Model}(Base):\n    __tablename__ = \"${2:table_name}\"\n\n    id: Mapped[int] = mapped_column(primary_key=True)\n    $0",
        root_only: true,
        class_only: false,
    },
    Snippet {
        prefix: "sapk",
        label: "sapk",
        body: "id: Mapped[int] = mapped_column(primary_key=True)",
        root_only: false,
        class_only: true,
    },
    Snippet {
        prefix: "sacol",
        label: "sacol",
        body: "${1:name}: Mapped[${2:str}] = mapped_column()",
        root_only: false,
        class_only: true,
    },
    Snippet {
        prefix: "saopt",
        label: "saopt",
        body: "${1:name}: Mapped[Optional[${2:str}]] = mapped_column(nullable=True)",
        root_only: false,
        class_only: true,
    },
    Snippet {
        prefix: "safk",
        label: "safk",
        body: "${1:target_id}: Mapped[int] = mapped_column(ForeignKey(\"${2:table.id}\"))",
        root_only: false,
        class_only: true,
    },
    Snippet {
        prefix: "sarel",
        label: "sarel",
        body: "${1:target}: Mapped[\"${2:Model}\"] = relationship(back_populates=\"${3:reverse}\")",
        root_only: false,
        class_only: true,
    },
    Snippet {
        prefix: "sarelmany",
        label: "sarelmany",
        body: "${1:targets}: Mapped[list[\"${2:Model}\"]] = relationship(back_populates=\"${3:reverse}\")",
        root_only: false,
        class_only: true,
    },
    Snippet {
        prefix: "sam2m",
        label: "sam2m",
        body: "${1:targets}: Mapped[list[\"${2:Model}\"]] = relationship(secondary=${3:assoc_table}, back_populates=\"${4:reverse}\")",
        root_only: false,
        class_only: true,
    },
    Snippet {
        prefix: "satable",
        label: "satable",
        body: "__tablename__ = \"${1:table_name}\"",
        root_only: false,
        class_only: true,
    },
    Snippet {
        prefix: "saidx",
        label: "saidx",
        body: "__table_args__ = (\n    sa.Index(\"ix_${1:table}_${2:col}\", \"${2:col}\"),\n)",
        root_only: false,
        class_only: true,
    },
];

// ── Alembic op catalogue (REQ-CMP-03) ─────────────────────────────────────────

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
        doc: "Add a unique constraint.",
        snippet: "create_unique_constraint(\"${1:name}\", \"${2:table}\", [\"${3:column}\"])",
    },
    OpInfo {
        name: "drop_constraint",
        doc: "Drop a named constraint.",
        snippet: "drop_constraint(\"${1:name}\", \"${2:table}\", type_=\"${3:unique}\")",
    },
    OpInfo {
        name: "create_foreign_key",
        doc: "Add a foreign-key constraint.",
        snippet: "create_foreign_key(\"${1:name}\", \"${2:src}\", \"${3:ref}\", [\"${4:local}\"], [\"${5:remote}\"])",
    },
    OpInfo {
        name: "create_check_constraint",
        doc: "Add a CHECK constraint.",
        snippet: "create_check_constraint(\"${1:name}\", \"${2:table}\", ${3:condition})",
    },
    OpInfo {
        name: "rename_table",
        doc: "Rename a table.",
        snippet: "rename_table(\"${1:old}\", \"${2:new}\")",
    },
    OpInfo {
        name: "execute",
        doc: "Execute arbitrary SQL.",
        snippet: "execute(\"${1:sql}\")",
    },
    OpInfo {
        name: "bulk_insert",
        doc: "Bulk-insert rows.",
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
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        })
        .collect()
}

// ── Item constructors ─────────────────────────────────────────────────────────

fn kwarg_item(label: &str, snippet: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some(detail.to_string()),
        insert_text: Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

fn value_item(label: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some(detail.to_string()),
        ..Default::default()
    }
}

fn field_item(label: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::FIELD),
        detail: Some(detail.to_string()),
        ..Default::default()
    }
}

fn reference_item(label: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::REFERENCE),
        detail: Some(detail.to_string()),
        ..Default::default()
    }
}

fn snippet_item(label: &str, body: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        insert_text: Some(body.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

// ── Text helpers ──────────────────────────────────────────────────────────────

fn line_prefix(source: &str, line: u32, col: u32) -> String {
    source
        .lines()
        .nth(line as usize)
        .map(|l| l[..col.min(l.len() as u32) as usize].to_string())
        .unwrap_or_default()
}

/// Return the word currently being typed (identifier chars before cursor).
fn current_word(source: &str, line: u32, col: u32) -> String {
    let prefix = line_prefix(source, line, col);
    prefix
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

/// Detect the target model from a `Mapped["Target"]` or `Mapped[list["Target"]]`
/// annotation on the same source line.
fn extract_mapped_target(source: &str, line: u32) -> Option<String> {
    let l = source.lines().nth(line as usize)?;
    // Patterns: Mapped["Target"] or Mapped[Optional["Target"]] or Mapped[list["Target"]]
    let mapped_start = l.find("Mapped[")?;
    let after = &l[mapped_start + 7..]; // skip "Mapped["
    // Remove Optional[ or list[ wrappers
    let inner = if let Some(s) = after.strip_prefix("list[") {
        s
    } else if let Some(s) = after.strip_prefix("Optional[") {
        s
    } else {
        after
    };
    // Extract string: "Target" or 'Target'
    let (quote, rest) = if let Some(s) = inner.strip_prefix('"') {
        ('"', s)
    } else if let Some(s) = inner.strip_prefix('\'') {
        ('\'', s)
    } else if inner
        .chars()
        .next()
        .map(|c| c.is_alphabetic())
        .unwrap_or(false)
    {
        // Bare name: Mapped[User]
        let name: String = inner
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        return if name.is_empty() { None } else { Some(name) };
    } else {
        return None;
    };
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

// ── Context classification via tree-sitter ────────────────────────────────────

/// Whether the cursor is on a kwarg key or inside a kwarg value.
#[derive(Debug)]
enum KwargPos {
    Key,
    Value,
}

/// Find the enclosing `call` node walking up from `leaf`.
/// Walk up the AST from `node` to find the enclosing `keyword_argument` and return its key.
fn kwarg_key_from_ast(mut node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    loop {
        if node.kind() == "keyword_argument" {
            return node
                .child_by_field_name("name")
                .map(|n| node_text(n, source).to_string());
        }
        node = node.parent()?;
    }
}

fn find_enclosing_call(leaf: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut node = leaf;
    loop {
        let kind = node.kind();
        if kind == "call" {
            return Some(node);
        }
        if matches!(kind, "module" | "class_definition" | "decorated_definition") {
            return None;
        }
        node = node.parent()?;
    }
}

/// Whether we are currently inside a class body.
fn cursor_in_class(leaf: tree_sitter::Node) -> bool {
    let mut node = leaf;
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "class_definition" => return true,
            "module" => return false,
            _ => node = parent,
        }
    }
    false
}

/// Determine the kwarg position relative to the cursor inside a call's argument list.
fn kwarg_position(args_node: tree_sitter::Node, pos: Position, _source: &[u8]) -> Option<KwargPos> {
    let cursor_row = pos.line as usize;
    let cursor_col = pos.character as usize;
    let mut c = args_node.walk();
    for child in args_node.named_children(&mut c) {
        if child.kind() != "keyword_argument" {
            continue;
        }
        // Check if cursor is within this keyword_argument
        let start = child.start_position();
        let end = child.end_position();
        let in_range = (cursor_row > start.row
            || (cursor_row == start.row && cursor_col >= start.column))
            && (cursor_row < end.row || (cursor_row == end.row && cursor_col <= end.column));
        if !in_range {
            continue;
        }
        // Find the `=` separator — anything to the right of it is the value
        let name_node = child.child_by_field_name("name")?;
        let eq_end = name_node.end_position();
        // If cursor is after the name node's end we're in the value
        if cursor_row > eq_end.row || (cursor_row == eq_end.row && cursor_col > eq_end.column) {
            return Some(KwargPos::Value);
        }
        return Some(KwargPos::Key);
    }
    None
}

/// Count the positional argument index (0-based) at the cursor.
/// Commas inside strings and nested parens are not counted.
fn positional_arg_index(args_node: tree_sitter::Node, pos: Position, source: &[u8]) -> usize {
    let end_byte = args_node.end_byte();
    let start_byte = args_node.start_byte();
    let cursor_row = pos.line as usize;
    let cursor_col = pos.character as usize;

    // Determine cursor byte offset within source.
    let source_str = std::str::from_utf8(source).unwrap_or("");
    let cursor_byte = source_str
        .lines()
        .take(cursor_row)
        .map(|l| l.len() + 1)
        .sum::<usize>()
        + cursor_col;

    if cursor_byte <= start_byte {
        return 0;
    }
    let rel_end = cursor_byte.min(end_byte).saturating_sub(start_byte);
    let slice = &source[start_byte..start_byte + rel_end];
    let text = std::str::from_utf8(slice).unwrap_or("");

    let mut depth = 0i32;
    let mut in_str = false;
    let mut quote_ch = ' ';
    let mut commas = 0usize;
    let mut past_open = false;
    for ch in text.chars() {
        if !in_str {
            match ch {
                '(' | '[' | '{' => {
                    depth += 1;
                    past_open = true;
                }
                ')' | ']' | '}' => depth -= 1,
                '"' | '\'' if depth >= 1 => {
                    in_str = true;
                    quote_ch = ch;
                }
                ',' if depth == 1 && past_open => commas += 1,
                _ => {}
            }
        } else if ch == quote_ch {
            in_str = false;
        }
    }
    commas
}

// ── Completion sets ───────────────────────────────────────────────────────────

fn complete_fk(prefix: &str, state: &WorkspaceState) -> Option<Vec<CompletionItem>> {
    let items: Vec<CompletionItem> = state
        .file_models
        .iter()
        .flat_map(|entry| {
            let models = entry.value().clone();
            models.into_iter().filter_map(|m| {
                let table = m.table_name.clone()?;
                Some(m.columns.into_iter().map(move |(col_name, col)| {
                    let label = format!("{}.{}", table, col_name);
                    let detail = format!("{}.{} ({})", m.name, col_name, col.mapped_type);
                    reference_item(&label, &detail)
                }))
            })
        })
        .flatten()
        .filter(|item| item.label.starts_with(prefix))
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

fn complete_relationship_kwargs(prefix: &str) -> Option<Vec<CompletionItem>> {
    let items: Vec<CompletionItem> = REL_KWARGS
        .iter()
        .filter(|(k, _, _)| k.starts_with(prefix))
        .map(|(k, snip, doc)| kwarg_item(k, snip, doc))
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

fn complete_lazy_values(prefix: &str) -> Option<Vec<CompletionItem>> {
    let items: Vec<CompletionItem> = LAZY_VALUES
        .iter()
        .filter(|(v, _)| v.starts_with(prefix))
        .map(|(v, d)| value_item(v, d))
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

fn complete_cascade_values(text_in_string: &str) -> Option<Vec<CompletionItem>> {
    // Filter by the token after the last comma.
    let prefix = text_in_string.rsplit(',').next().unwrap_or("").trim();
    let items: Vec<CompletionItem> = CASCADE_TOKENS
        .iter()
        .filter(|(t, _)| t.starts_with(prefix))
        .map(|(t, d)| value_item(t, d))
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

fn complete_back_populates(
    target_model: &str,
    prefix: &str,
    state: &WorkspaceState,
) -> Option<Vec<CompletionItem>> {
    let loc = state.model_index.get(target_model)?;
    let uri = loc.uri.clone();
    drop(loc);
    let file_models = state.file_models.get(&uri)?;
    let model = file_models.iter().find(|m| m.name == target_model)?;
    let items: Vec<CompletionItem> = model
        .relationships
        .keys()
        .filter(|k| k.starts_with(prefix))
        .map(|k| field_item(k, &format!("{}.{}", target_model, k)))
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

fn complete_rel_target_models(prefix: &str, state: &WorkspaceState) -> Option<Vec<CompletionItem>> {
    let items: Vec<CompletionItem> = state
        .model_index
        .iter()
        .filter(|e| e.key().starts_with(prefix))
        .map(|e| CompletionItem {
            label: e.key().clone(),
            kind: Some(CompletionItemKind::CLASS),
            ..Default::default()
        })
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

fn complete_mapped_column_kwargs(prefix: &str) -> Option<Vec<CompletionItem>> {
    let items: Vec<CompletionItem> = MC_KWARGS
        .iter()
        .filter(|(k, _, _)| k.starts_with(prefix))
        .map(|(k, snip, doc)| kwarg_item(k, snip, doc))
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

fn complete_table_arg_columns(
    _source_line: u32,
    prefix: &str,
    leaf: tree_sitter::Node,
    source: &[u8],
    state: &WorkspaceState,
) -> Option<Vec<CompletionItem>> {
    // Walk up to find the enclosing class to get the model's columns.
    let mut node = leaf;
    let class_name = loop {
        match node.kind() {
            "class_definition" => {
                let name_node = node.child_by_field_name("name")?;
                break node_text(name_node, source).to_string();
            }
            "module" => return None,
            _ => node = node.parent()?,
        }
    };
    let loc = state.model_index.get(&class_name)?;
    let uri = loc.uri.clone();
    drop(loc);
    let file_models = state.file_models.get(&uri)?;
    let model = file_models.iter().find(|m| m.name == class_name)?;
    let items: Vec<CompletionItem> = model
        .columns
        .iter()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(k, col)| field_item(k, &format!("{}", col.mapped_type)))
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

fn complete_model_constructor(
    model_name: &str,
    prefix: &str,
    state: &WorkspaceState,
) -> Option<Vec<CompletionItem>> {
    let loc = state.model_index.get(model_name)?;
    let uri = loc.uri.clone();
    drop(loc);
    let file_models = state.file_models.get(&uri)?;
    let model = file_models.iter().find(|m| m.name == model_name)?;
    let mut items: Vec<CompletionItem> = model
        .columns
        .iter()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(k, col)| {
            let mut item = kwarg_item(k, &format!("{k}=$1"), &col.mapped_type.to_string());
            // sort_text: "0" prefix so columns appear before relationships
            item.sort_text = Some(format!("0{k}"));
            item
        })
        .collect();
    items.extend(
        model
            .relationships
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, rel)| {
                let detail = if rel.is_list {
                    format!("list[{}]", rel.target_model)
                } else {
                    rel.target_model.clone()
                };
                let mut item = kwarg_item(k, &format!("{k}=$1"), &detail);
                item.sort_text = Some(format!("1{k}"));
                item
            }),
    );
    if items.is_empty() { None } else { Some(items) }
}

fn complete_op_table_or_column(
    op_name: &str,
    arg_idx: usize,
    partial: &str,
    state: &WorkspaceState,
    call_node: tree_sitter::Node,
    source: &[u8],
) -> Option<Vec<CompletionItem>> {
    // Determine which arg position carries a table name for this op.
    let table_arg_idx = match op_name {
        "create_index" | "create_unique_constraint" => 1,
        "create_foreign_key" => 1,
        _ => 0,
    };
    let col_arg_idx = match op_name {
        "drop_column" | "alter_column" => table_arg_idx + 1,
        _ => usize::MAX,
    };

    if arg_idx == table_arg_idx {
        // Offer table names.
        let items: Vec<CompletionItem> = state
            .table_index
            .iter()
            .filter(|e| e.key().starts_with(partial))
            .map(|e| CompletionItem {
                label: e.key().clone(),
                kind: Some(CompletionItemKind::VALUE),
                ..Default::default()
            })
            .collect();
        return if items.is_empty() { None } else { Some(items) };
    }

    if arg_idx == col_arg_idx {
        // Find the table name from the table_arg_idx positional argument.
        let args = call_node.child_by_field_name("arguments")?;
        let mut c = args.walk();
        let positional: Vec<_> = args
            .named_children(&mut c)
            .filter(|n| n.kind() != "keyword_argument")
            .collect();
        let table_node = positional.get(table_arg_idx)?;
        if table_node.kind() != "string" {
            return None;
        }
        let table_text = node_text(*table_node, source);
        let table = table_text.trim_matches('"').trim_matches('\'').to_string();
        let model_name = state.table_index.get(&table)?;
        let loc = state.model_index.get(&*model_name)?;
        let uri = loc.uri.clone();
        drop(loc);
        let file_models = state.file_models.get(&uri)?;
        let model = file_models
            .iter()
            .find(|m| m.table_name.as_deref() == Some(&table))?;
        let items: Vec<CompletionItem> = model
            .columns
            .keys()
            .filter(|k| k.starts_with(partial))
            .map(|k| CompletionItem {
                label: k.clone(),
                kind: Some(CompletionItemKind::FIELD),
                ..Default::default()
            })
            .collect();
        return if items.is_empty() { None } else { Some(items) };
    }
    None
}

fn complete_snippets(prefix: &str, in_class: bool) -> Option<Vec<CompletionItem>> {
    if prefix.is_empty() {
        return None;
    }
    let items: Vec<CompletionItem> = SNIPPETS
        .iter()
        .filter(|s| {
            if s.root_only && in_class {
                return false;
            }
            if s.class_only && !in_class {
                return false;
            }
            s.prefix.starts_with(prefix) || s.label.starts_with(prefix)
        })
        .map(|s| snippet_item(s.label, s.body))
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

// ── String-content extraction ─────────────────────────────────────────────────

/// Extract the text currently typed inside the string at the cursor.
fn string_prefix_at_cursor(source: &str, pos: Position) -> String {
    let prefix = line_prefix(source, pos.line, pos.character);
    // Find the last unmatched opening quote.
    let mut in_string = false;
    let mut quote_ch = ' ';
    let mut content = String::new();
    for ch in prefix.chars() {
        if !in_string {
            if ch == '"' || ch == '\'' {
                in_string = true;
                quote_ch = ch;
                content.clear();
            }
        } else if ch == quote_ch {
            // String ended — start fresh if there's another open quote later
            in_string = false;
            content.clear();
        } else {
            content.push(ch);
        }
    }
    if in_string { content } else { String::new() }
}

// ── Text-based call-context detection ─────────────────────────────────────────
// These helpers work on incomplete source (the typical editing scenario) by
// scanning the line prefix with a simple state machine.

/// True when `prefix` ends with an unclosed string literal.
fn is_in_open_string(prefix: &str) -> bool {
    let mut in_string = false;
    let mut quote_ch = ' ';
    for ch in prefix.chars() {
        if in_string {
            if ch == quote_ch {
                in_string = false;
            }
        } else if ch == '"' || ch == '\'' {
            in_string = true;
            quote_ch = ch;
        }
    }
    in_string
}

/// Return the innermost unclosed function-call's base name from `prefix`.
/// E.g. `relationship("Author", lazy="` → `"relationship"`.
fn innermost_call_name(prefix: &str) -> Option<String> {
    let chars: Vec<char> = prefix.chars().collect();
    let n = chars.len();
    let mut call_stack: Vec<String> = Vec::new();
    let mut in_string = false;
    let mut quote_ch = ' ';
    let mut i = 0;

    while i < n {
        let ch = chars[i];
        if in_string {
            if ch == quote_ch {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_string = true;
                quote_ch = ch;
            }
            '(' => {
                // Extract identifier (possibly dotted) immediately before this '('
                let func_end = i;
                let func_start = chars[..func_end]
                    .iter()
                    .rposition(|c| !c.is_alphanumeric() && *c != '_' && *c != '.')
                    .map(|p| p + 1)
                    .unwrap_or(0);
                let base: String = chars[func_start..func_end]
                    .iter()
                    .collect::<String>()
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                call_stack.push(base);
            }
            ')' => {
                call_stack.pop();
            }
            _ => {}
        }
        i += 1;
    }
    call_stack.last().filter(|s| !s.is_empty()).cloned()
}

/// If the cursor is inside a string that is the value of `key="…`, return the key name.
fn kwarg_key_for_string_value(prefix: &str) -> Option<String> {
    if !is_in_open_string(prefix) {
        return None;
    }
    // Find position of the last unclosed opening quote.
    let mut in_string = false;
    let mut quote_ch = ' ';
    let mut open_pos: Option<usize> = None;
    for (i, ch) in prefix.char_indices() {
        if in_string {
            if ch == quote_ch {
                in_string = false;
                open_pos = None;
            }
        } else if ch == '"' || ch == '\'' {
            in_string = true;
            quote_ch = ch;
            open_pos = Some(i);
        }
    }
    let open_pos = open_pos?;
    // Text before the opening quote must end with `key=`
    let before = prefix[..open_pos].trim_end();
    let before_eq = before.strip_suffix('=')?.trim_end();
    let key: String = before_eq
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if key.is_empty() { None } else { Some(key) }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Return completions for `pos` in `source`, or `None` outside recognized contexts (REQ-CMP-15).
pub fn provide_completions(
    uri: &Uri,
    source: &str,
    pos: Position,
    state: &WorkspaceState,
) -> Option<Vec<CompletionItem>> {
    let prefix = line_prefix(source, pos.line, pos.character);
    let str_pfx = string_prefix_at_cursor(source, pos);
    let word_pfx = current_word(source, pos.line, pos.character);
    let in_open_str = is_in_open_string(&prefix);

    // REQ-CMP-03: `op.` → Alembic operation names.
    if prefix.trim_end().ends_with("op.") && state.migration_files.contains_key(uri) {
        return Some(op_items());
    }

    // Text-based context detection — robust against incomplete source.
    match innermost_call_name(&prefix).as_deref() {
        // REQ-CMP-05: ForeignKey("…") → table.column targets
        Some("ForeignKey") if in_open_str => return complete_fk(&str_pfx, state),

        // REQ-CMP-06..10: relationship(…)
        Some("relationship") => {
            if in_open_str {
                match kwarg_key_for_string_value(&prefix).as_deref() {
                    Some("lazy") => return complete_lazy_values(&str_pfx), // REQ-CMP-07
                    Some("cascade") => return complete_cascade_values(&str_pfx), // REQ-CMP-08
                    Some("back_populates") => {
                        // REQ-CMP-09
                        let target = extract_mapped_target(source, pos.line)?;
                        return complete_back_populates(&target, &str_pfx, state);
                    }
                    None => return complete_rel_target_models(&str_pfx, state), // REQ-CMP-10
                    _ => return None,
                }
            } else {
                return complete_relationship_kwargs(&word_pfx); // REQ-CMP-06
            }
        }

        // REQ-CMP-11: mapped_column(…) kwargs (not inside a string)
        Some("mapped_column") if !in_open_str => return complete_mapped_column_kwargs(&word_pfx),

        _ => {}
    }

    // Tree-sitter for structural queries that need the full parse tree.
    let source_bytes = source.as_bytes();
    let tree_ref = state.parse_trees.get(uri)?;
    let root = tree_ref.root_node();
    let cursor_pt = tree_sitter::Point {
        row: pos.line as usize,
        column: pos.character as usize,
    };
    let leaf = root
        .descendant_for_point_range(cursor_pt, cursor_pt)
        .unwrap_or(root);

    let Some(call_node) = find_enclosing_call(leaf) else {
        // REQ-CMP-14: snippets at a non-call position
        let in_class = cursor_in_class(leaf);
        return complete_snippets(&word_pfx, in_class);
    };

    let func_node = call_node.child_by_field_name("function")?;
    let base_name = {
        let ft = node_text(func_node, source_bytes);
        ft.rsplit('.').next().unwrap_or(ft).to_string()
    };
    let args_node = call_node.child_by_field_name("arguments");
    let arg_idx = args_node
        .map(|a| positional_arg_index(a, pos, source_bytes))
        .unwrap_or(0);
    let in_string_ts = matches!(
        leaf.kind(),
        "string" | "string_content" | "string_end" | "string_start"
    );

    match base_name.as_str() {
        // REQ-CMP-12: __table_args__ constraint column strings
        "Index" | "UniqueConstraint" | "PrimaryKeyConstraint" if in_string_ts => {
            let name_arg_count = if base_name != "PrimaryKeyConstraint" {
                1
            } else {
                0
            };
            if arg_idx > name_arg_count {
                return complete_table_arg_columns(pos.line, &str_pfx, leaf, source_bytes, state);
            }
        }

        // REQ-CMP-04: op.xxx args → table/column
        op_name if state.migration_files.contains_key(uri) && in_string_ts => {
            return complete_op_table_or_column(
                op_name,
                arg_idx,
                &str_pfx,
                state,
                call_node,
                source_bytes,
            );
        }

        // REQ-CMP-05 (multi-line): ForeignKey("…") → table.column targets
        "ForeignKey" if in_string_ts => return complete_fk(&str_pfx, state),

        // REQ-CMP-06..10 (multi-line): relationship(…) kwarg completions via AST
        "relationship" => {
            if in_string_ts {
                let kwarg_key = kwarg_key_from_ast(leaf, source_bytes);
                match kwarg_key.as_deref() {
                    Some("lazy") => return complete_lazy_values(&str_pfx),
                    Some("cascade") => return complete_cascade_values(&str_pfx),
                    Some("back_populates") => {
                        let target = extract_mapped_target(source, pos.line)?;
                        return complete_back_populates(&target, &str_pfx, state);
                    }
                    None => return complete_rel_target_models(&str_pfx, state),
                    _ => return None,
                }
            } else {
                return complete_relationship_kwargs(&word_pfx);
            }
        }

        // REQ-CMP-11 (multi-line): mapped_column(…) kwargs
        "mapped_column" if !in_string_ts => return complete_mapped_column_kwargs(&word_pfx),

        // REQ-CMP-13: Model constructor → column/relationship keywords
        model_name if state.model_index.contains_key(model_name) => {
            let kwarg_pos_ts = args_node.and_then(|a| kwarg_position(a, pos, source_bytes));
            if !matches!(kwarg_pos_ts, Some(KwargPos::Value)) {
                return complete_model_constructor(model_name, &word_pfx, state);
            }
        }

        _ => {}
    }

    None
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::{ColumnArgs, MappedType, Range};
    use crate::state::WorkspaceState;
    use std::collections::HashMap;
    use tower_lsp_server::ls_types::Uri;

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }
    fn def_range() -> Range {
        Range::default()
    }

    fn parse_and_store(src: &str, u: &Uri, state: &WorkspaceState) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        state.parse_trees.insert(u.clone(), tree);
        state.file_sources.insert(u.clone(), src.to_string());
    }

    fn make_model_with_cols(
        state: &WorkspaceState,
        u: &Uri,
        name: &str,
        table: &str,
        cols: &[&str],
    ) {
        use crate::model::types::{Column, Model};
        let columns: HashMap<String, Column> = cols
            .iter()
            .map(|c| {
                (
                    c.to_string(),
                    Column {
                        name: c.to_string(),
                        key: None,
                        mapped_type: MappedType::Str,
                        args: ColumnArgs::default(),
                        foreign_key: None,
                        doc: None,
                        name_range: def_range(),
                        full_range: def_range(),
                    },
                )
            })
            .collect();
        let model = Model {
            name: name.to_string(),
            table_name: Some(table.to_string()),
            bases: vec![],
            columns,
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: def_range(),
            full_range: def_range(),
        };
        state.update_file(u, vec![model]);
    }

    // ── REQ-CMP-03: op. completions ───────────────────────────────────────────

    #[test]
    fn req_cmp_03_op_dot_in_migration_returns_ops() {
        let state = WorkspaceState::new();
        let u = uri("file:///v1.py");
        let src = "from alembic import op\nrevision = 'a1'\ndef upgrade():\n    op.";
        parse_and_store(src, &u, &state);
        // Mark as migration file
        use crate::alembic::{DownRevision, MigrationFile};
        state.migration_files.insert(
            u.clone(),
            MigrationFile {
                revision: Some("a1".to_string()),
                down_revision: DownRevision::None,
                message: None,
                revision_range: None,
                down_revision_range: None,
                op_calls: vec![],
            },
        );
        let pos = Position {
            line: 3,
            character: 7,
        }; // after "op."
        let items = provide_completions(&u, src, pos, &state).unwrap();
        assert!(items.iter().any(|i| i.label == "add_column"), "{items:?}");
        assert!(items.iter().any(|i| i.label == "create_table"), "{items:?}");
    }

    #[test]
    fn req_cmp_03_op_dot_outside_migration_returns_none() {
        let state = WorkspaceState::new();
        let u = uri("file:///models.py");
        let src = "op.";
        parse_and_store(src, &u, &state);
        // No migration file registered
        let pos = Position {
            line: 0,
            character: 3,
        };
        assert!(provide_completions(&u, src, pos, &state).is_none());
    }

    // ── REQ-CMP-05: FK completions ────────────────────────────────────────────

    #[test]
    fn req_cmp_05_fk_string_returns_table_dot_col() {
        let state = WorkspaceState::new();
        let model_u = uri("file:///user.py");
        make_model_with_cols(&state, &model_u, "User", "users", &["id", "email"]);

        let u = uri("file:///post.py");
        let src =
            "from sqlalchemy import ForeignKey\nauthor_id = mapped_column(ForeignKey(\"us\"))";
        parse_and_store(src, &u, &state);
        let pos = Position {
            line: 1,
            character: 39,
        }; // inside "us" (on the 's')
        let items = provide_completions(&u, src, pos, &state);
        let items = items.unwrap_or_default();
        assert!(
            items.iter().any(|i| i.label.starts_with("users.")),
            "expected users.* items, got {items:?}"
        );
    }

    // ── REQ-CMP-06: relationship kwargs ──────────────────────────────────────

    #[test]
    fn req_cmp_06_relationship_kwarg_at_empty() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let src = "from sqlalchemy.orm import relationship\nauthor = relationship(";
        parse_and_store(src, &u, &state);
        let pos = Position {
            line: 1,
            character: src.lines().nth(1).unwrap().len() as u32,
        };
        let items = provide_completions(&u, src, pos, &state).unwrap_or_default();
        assert!(
            items.iter().any(|i| i.label == "back_populates"),
            "{items:?}"
        );
        assert!(items.iter().any(|i| i.label == "lazy"), "{items:?}");
    }

    // ── REQ-CMP-07: lazy= values ──────────────────────────────────────────────

    #[test]
    fn req_cmp_07_lazy_value_completions() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let src = "from sqlalchemy.orm import relationship\nauthor = relationship(lazy=\"";
        parse_and_store(src, &u, &state);
        let pos = Position {
            line: 1,
            character: src.lines().nth(1).unwrap().len() as u32,
        };
        let items = provide_completions(&u, src, pos, &state).unwrap_or_default();
        assert!(items.iter().any(|i| i.label == "joined"), "{items:?}");
        assert!(items.iter().any(|i| i.label == "selectin"), "{items:?}");
    }

    // ── REQ-CMP-08: cascade= values ───────────────────────────────────────────

    #[test]
    fn req_cmp_08_cascade_value_completions() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let src = "from sqlalchemy.orm import relationship\nauthor = relationship(cascade=\"";
        parse_and_store(src, &u, &state);
        let pos = Position {
            line: 1,
            character: src.lines().nth(1).unwrap().len() as u32,
        };
        let items = provide_completions(&u, src, pos, &state).unwrap_or_default();
        assert!(
            items.iter().any(|i| i.label == "all, delete-orphan"),
            "{items:?}"
        );
        assert!(
            items.iter().any(|i| i.label == "delete-orphan"),
            "{items:?}"
        );
    }

    #[test]
    fn req_cmp_08_cascade_after_existing_token() {
        // Regression: cascade="all, $CURSOR" on a multi-line relationship call.
        // innermost_call_name returns None (no `(` on the continuation line),
        // so the tree-sitter fallback must handle it.
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let src = "from sqlalchemy.orm import relationship\nauthor = relationship(\n    back_populates=\"bp\",\n    cascade=\"all, \"\n)";
        parse_and_store(src, &u, &state);
        // Line 3 is `    cascade="all, "` — cursor before the closing quote
        let line3 = src.lines().nth(3).unwrap();
        let char_pos = line3.find("all, ").unwrap() as u32 + "all, ".len() as u32;
        let pos = Position { line: 3, character: char_pos };
        let items = provide_completions(&u, src, pos, &state).unwrap_or_default();
        assert!(
            items.iter().any(|i| i.label == "delete-orphan"),
            "multi-line cascade after existing token should offer delete-orphan; got {items:?}"
        );
    }

    // ── REQ-CMP-11: mapped_column kwargs ──────────────────────────────────────

    #[test]
    fn req_cmp_11_mapped_column_kwargs() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let src = "from sqlalchemy.orm import mapped_column\nid = mapped_column(";
        parse_and_store(src, &u, &state);
        let pos = Position {
            line: 1,
            character: src.lines().nth(1).unwrap().len() as u32,
        };
        let items = provide_completions(&u, src, pos, &state).unwrap_or_default();
        assert!(items.iter().any(|i| i.label == "primary_key"), "{items:?}");
        assert!(items.iter().any(|i| i.label == "nullable"), "{items:?}");
    }

    // ── REQ-CMP-14: snippets ──────────────────────────────────────────────────

    #[test]
    fn req_cmp_14_samodel_at_file_root() {
        let state = WorkspaceState::new();
        let u = uri("file:///new_model.py");
        let src = "samodel";
        parse_and_store(src, &u, &state);
        let pos = Position {
            line: 0,
            character: 7,
        };
        let items = provide_completions(&u, src, pos, &state).unwrap_or_default();
        assert!(items.iter().any(|i| i.label == "samodel"), "{items:?}");
    }

    // ── Multi-line call regressions ───────────────────────────────────────────

    #[test]
    fn multiline_fk_string_completions() {
        // ForeignKey(\n    "us|"\n) — ForeignKey( is on a different line
        let state = WorkspaceState::new();
        let model_u = uri("file:///user.py");
        make_model_with_cols(&state, &model_u, "User", "users", &["id"]);
        let u = uri("file:///post.py");
        let src = "from sqlalchemy import ForeignKey\nauthor_id = mapped_column(\n    ForeignKey(\n        \"us\"\n    )\n)";
        parse_and_store(src, &u, &state);
        // cursor inside "us" on line 3
        let pos = Position { line: 3, character: 10 };
        let items = provide_completions(&u, src, pos, &state).unwrap_or_default();
        assert!(
            items.iter().any(|i| i.label.starts_with("users.")),
            "multi-line ForeignKey should offer FK targets; got {items:?}"
        );
    }

    #[test]
    fn multiline_mapped_column_kwargs() {
        // mapped_column( on line 1, cursor before existing kwarg on line 2.
        // Needs complete source so tree-sitter can identify the enclosing call.
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let src = "from sqlalchemy.orm import mapped_column\nid = mapped_column(\n    nullable=False\n)";
        parse_and_store(src, &u, &state);
        // Cursor at (2, 4) — start of `nullable` but word_pfx="" since only spaces to the left
        let pos = Position { line: 2, character: 4 };
        let items = provide_completions(&u, src, pos, &state).unwrap_or_default();
        assert!(
            items.iter().any(|i| i.label == "primary_key"),
            "multi-line mapped_column should offer kwargs; got {items:?}"
        );
    }

    #[test]
    fn multiline_relationship_positional_string() {
        // relationship(\n    "Us|"\n) — positional model-name string on continuation line
        let state = WorkspaceState::new();
        let model_u = uri("file:///user.py");
        make_model_with_cols(&state, &model_u, "User", "users", &["id"]);
        let u = uri("file:///post.py");
        let src = "from sqlalchemy.orm import relationship\nauthor = relationship(\n    \"Us\"\n)";
        parse_and_store(src, &u, &state);
        // cursor inside "Us" on line 2 (after "Us")
        let pos = Position { line: 2, character: 7 };
        let items = provide_completions(&u, src, pos, &state).unwrap_or_default();
        assert!(
            items.iter().any(|i| i.label == "User"),
            "multi-line relationship positional string should offer model names; got {items:?}"
        );
    }

    // ── REQ-CMP-15: negative — plain Python → None ────────────────────────────

    #[test]
    fn req_cmp_15_plain_python_returns_none() {
        let state = WorkspaceState::new();
        let u = uri("file:///plain.py");
        let src = "def slugify(title):\n    return title.lower()";
        parse_and_store(src, &u, &state);
        // Cursor after "title." — no SA context
        let pos = Position {
            line: 1,
            character: 18,
        }; // after "title."
        // This line ends with "title.lower()" and cursor is inside lower() call
        // but "lower" is not an SA call, so should return None
        assert!(provide_completions(&u, src, pos, &state).is_none());
    }
}
