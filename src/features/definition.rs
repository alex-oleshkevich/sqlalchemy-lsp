/// Go-to-definition handler.
///
/// Alembic slice (F13, REQ-ALM-09/10):
///   - A table string in an `op.*` call → the model's class-name range.
///   - A column string in an `op.*` call → the column's attribute range.
///
/// Returns `None` (not an error) when the target can't be resolved (P4).
use tower_lsp_server::ls_types::{Location, Position, Range, Uri};

use crate::state::WorkspaceState;

/// Resolve a position in a migration file to a definition location.
///
/// Only fires in Alembic context; for all other files returns `None`.
pub fn resolve_definition(
    uri: &Uri,
    _source: &str,
    pos: Position,
    state: &WorkspaceState,
) -> Option<Location> {
    if !state.migration_files.contains_key(uri) {
        return None;
    }
    let mf = state.migration_files.get(uri)?;

    for op in &mf.op_calls {
        // Check if the cursor is on the table reference.
        if let Some(ref tref) = op.table_name {
            if position_in_range(pos, tref.range) {
                return resolve_table(tref.name.as_str(), state);
            }
        }
        // Check if the cursor is on the column reference.
        if let Some(ref cref) = op.column_name {
            if position_in_range(pos, cref.range) {
                let table_name = op.table_name.as_ref().map(|t| t.name.as_str())?;
                return resolve_column(table_name, cref.name.as_str(), state);
            }
        }
    }
    None
}

fn position_in_range(pos: Position, r: crate::model::types::Range) -> bool {
    let on_start_line = pos.line == r.start_line && pos.character >= r.start_col;
    let on_end_line = pos.line == r.end_line && pos.character < r.end_col;
    let between = pos.line > r.start_line && pos.line < r.end_line;
    on_start_line || on_end_line || between
}

fn model_range_to_lsp(r: crate::model::types::Range) -> Range {
    Range {
        start: Position { line: r.start_line, character: r.start_col },
        end: Position { line: r.end_line, character: r.end_col },
    }
}

fn resolve_table(table: &str, state: &WorkspaceState) -> Option<Location> {
    let model_name = state.table_index.get(table)?;
    let loc = state.model_index.get(&*model_name)?;
    Some(Location {
        uri: loc.uri.clone(),
        range: model_range_to_lsp(loc.range),
    })
}

fn resolve_column(table: &str, column: &str, state: &WorkspaceState) -> Option<Location> {
    let model_name = state.table_index.get(table)?;
    let loc = state.model_index.get(&*model_name)?;
    let file_uri = loc.uri.clone();
    drop(loc);
    let file_models = state.file_models.get(&file_uri)?;
    let model = file_models.iter().find(|m| m.table_name.as_deref() == Some(table))?;
    let col = model.columns.get(column)?;
    Some(Location {
        uri: file_uri,
        range: model_range_to_lsp(col.name_range),
    })
}
