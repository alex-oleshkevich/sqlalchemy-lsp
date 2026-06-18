/// Signature help for `op.*` calls in Alembic migration files (REQ-ALM-08).
///
/// Returns a `SignatureHelp` with the active parameter highlighted when the
/// cursor sits inside a recognised `op.*` call's argument list.  Outside
/// Alembic context this always returns `None` (P5).
use tower_lsp_server::ls_types::{
    ParameterInformation, ParameterLabel, Position, SignatureHelp, SignatureInformation,
};

use crate::state::WorkspaceState;

// ── Static signature table ─────────────────────────────────────────────────────

struct SigInfo {
    label: &'static str,
    params: &'static [&'static str],
}

const SIGNATURES: &[SigInfo] = &[
    SigInfo {
        label: "op.add_column(table_name, column, *, schema=None)",
        params: &["table_name", "column", "schema=None"],
    },
    SigInfo {
        label: "op.drop_column(table_name, column_name, *, schema=None)",
        params: &["table_name", "column_name", "schema=None"],
    },
    SigInfo {
        label: "op.alter_column(table_name, column_name, *, nullable=None, new_column_name=None, type_=None, existing_type=None)",
        params: &["table_name", "column_name", "nullable=None", "new_column_name=None", "type_=None", "existing_type=None"],
    },
    SigInfo {
        label: "op.create_table(table_name, *columns, **kw)",
        params: &["table_name", "*columns", "**kw"],
    },
    SigInfo {
        label: "op.drop_table(table_name, *, schema=None)",
        params: &["table_name", "schema=None"],
    },
    SigInfo {
        label: "op.create_index(index_name, table_name, columns, *, unique=False, schema=None)",
        params: &["index_name", "table_name", "columns", "unique=False", "schema=None"],
    },
    SigInfo {
        label: "op.drop_index(index_name, table_name=None, *, schema=None)",
        params: &["index_name", "table_name=None", "schema=None"],
    },
    SigInfo {
        label: "op.create_unique_constraint(constraint_name, table_name, columns, *, schema=None)",
        params: &["constraint_name", "table_name", "columns", "schema=None"],
    },
    SigInfo {
        label: "op.drop_constraint(constraint_name, table_name, *, type_=None, schema=None)",
        params: &["constraint_name", "table_name", "type_=None", "schema=None"],
    },
    SigInfo {
        label: "op.create_foreign_key(constraint_name, source_table, referent_table, local_cols, remote_cols, *, onupdate=None, ondelete=None, deferrable=None, initially=None, use_alter=False, match=None, source_schema=None, referent_schema=None)",
        params: &["constraint_name", "source_table", "referent_table", "local_cols", "remote_cols", "onupdate=None", "ondelete=None"],
    },
    SigInfo {
        label: "op.create_check_constraint(constraint_name, table_name, condition, *, schema=None)",
        params: &["constraint_name", "table_name", "condition", "schema=None"],
    },
    SigInfo {
        label: "op.rename_table(old_table_name, new_table_name, *, schema=None)",
        params: &["old_table_name", "new_table_name", "schema=None"],
    },
    SigInfo {
        label: "op.execute(sqltext, *, execution_options=immutabledict({}))",
        params: &["sqltext", "execution_options=immutabledict({})"],
    },
    SigInfo {
        label: "op.bulk_insert(table, rows, *, multiinsert=True)",
        params: &["table", "rows", "multiinsert=True"],
    },
];

fn find_sig(op_name: &str) -> Option<&'static SigInfo> {
    SIGNATURES.iter().find(|s| {
        // label starts with `op.<name>(`
        s.label.starts_with(&format!("op.{op_name}("))
    })
}

// ── Active-parameter detection ────────────────────────────────────────────────

/// Returns (op_name, active_param_index) when the cursor is inside an `op.*` call.
fn detect_active_param(source: &str, pos: Position) -> Option<(String, u32)> {
    let line_idx = pos.line as usize;
    let col = pos.character as usize;
    let line = source.lines().nth(line_idx)?;
    let prefix = &line[..col.min(line.len())];

    // Find the innermost `op.<name>(` on this line to the left of the cursor.
    let op_start = prefix.rfind("op.")?;
    let after_op = &prefix[op_start + 3..];
    let op_name: String = after_op
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if op_name.is_empty() {
        return None;
    }
    let after_name = &after_op[op_name.len()..];
    if !after_name.contains('(') {
        return None;
    }
    // Count commas outside strings to determine the active parameter index.
    let active = count_active_param(after_name);
    Some((op_name, active))
}

fn count_active_param(text: &str) -> u32 {
    let mut depth = 0i32;
    let mut commas = 0u32;
    let mut in_string = false;
    let mut quote_char = ' ';
    let mut past_open = false;
    for ch in text.chars() {
        if !in_string {
            match ch {
                '(' => {
                    depth += 1;
                    past_open = true;
                }
                ')' => depth -= 1,
                '"' | '\'' if depth > 0 => {
                    in_string = true;
                    quote_char = ch;
                }
                ',' if depth == 1 && past_open => commas += 1,
                _ => {}
            }
        } else if ch == quote_char {
            in_string = false;
        }
    }
    commas
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Return signature help for `pos` in `source`, or `None` outside Alembic context.
pub fn provide_signature_help(
    uri: &tower_lsp_server::ls_types::Uri,
    source: &str,
    pos: Position,
    state: &WorkspaceState,
) -> Option<SignatureHelp> {
    if !state.migration_files.contains_key(uri) {
        return None;
    }
    let (op_name, active_param) = detect_active_param(source, pos)?;
    let sig_info = find_sig(&op_name)?;

    let params: Vec<ParameterInformation> = sig_info
        .params
        .iter()
        .map(|p| ParameterInformation {
            label: ParameterLabel::Simple(p.to_string()),
            documentation: None,
        })
        .collect();

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label: sig_info.label.to_string(),
            documentation: None,
            parameters: Some(params),
            active_parameter: Some(active_param),
        }],
        active_signature: Some(0),
        active_parameter: Some(active_param),
    })
}
