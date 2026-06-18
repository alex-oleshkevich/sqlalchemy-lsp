use tower_lsp_server::ls_types::{Location, Position, Range, Uri};

use crate::{model::types::Model, state::WorkspaceState};

// ── Range helpers ─────────────────────────────────────────────────────────────

fn lsp_range(r: crate::model::types::Range) -> Range {
    Range {
        start: Position { line: r.start_line, character: r.start_col },
        end: Position { line: r.end_line, character: r.end_col },
    }
}

fn pos_in(pos: Position, r: crate::model::types::Range) -> bool {
    let after = pos.line > r.start_line || (pos.line == r.start_line && pos.character >= r.start_col);
    let before = pos.line < r.end_line || (pos.line == r.end_line && pos.character < r.end_col);
    after && before
}

// ── SA model resolution helpers ───────────────────────────────────────────────

fn model_location(model_name: &str, state: &WorkspaceState) -> Option<Location> {
    let loc = state.model_index.get(model_name)?;
    Some(Location { uri: loc.uri.clone(), range: lsp_range(loc.range) })
}

fn column_location(model_name: &str, col_name: &str, state: &WorkspaceState) -> Option<Location> {
    let loc = state.model_index.get(model_name)?;
    let uri = loc.uri.clone();
    drop(loc);
    let file_models = state.file_models.get(&uri)?;
    let model = file_models.iter().find(|m| m.name == model_name)?;
    let col = model.columns.get(col_name)?;
    Some(Location { uri, range: lsp_range(col.name_range) })
}

fn resolve_fk(table: &str, column: &str, state: &WorkspaceState) -> Option<Location> {
    let model_name = state.table_index.get(table)?.value().clone();
    column_location(&model_name, column, state).or_else(|| model_location(&model_name, state))
}

fn resolve_back_populates(target_model: &str, bp_name: &str, state: &WorkspaceState) -> Option<Location> {
    let loc = state.model_index.get(target_model)?;
    let uri = loc.uri.clone();
    drop(loc);
    let file_models = state.file_models.get(&uri)?;
    let model = file_models.iter().find(|m| m.name == target_model)?;
    let rel = model.relationships.get(bp_name)?;
    Some(Location { uri, range: lsp_range(rel.name_range) })
}

fn check_model(uri: &Uri, model: &Model, pos: Position, state: &WorkspaceState) -> Option<Location> {
    for rel in model.relationships.values() {
        if let (Some(bp), Some(bp_range)) = (&rel.back_populates, rel.back_populates_range) {
            if pos_in(pos, bp_range) {
                return resolve_back_populates(&rel.target_model, bp, state);
            }
        }
        if let Some(target_range) = rel.target_range {
            if pos_in(pos, target_range) {
                return model_location(&rel.target_model, state);
            }
        }
    }

    for col in model.columns.values() {
        if let Some(ref fk) = col.foreign_key {
            if pos_in(pos, fk.range) {
                return resolve_fk(&fk.table, &fk.column, state);
            }
        }
    }

    // __table_args__ column string → column in same file
    for ta in &model.table_args {
        for (col_name, &col_range) in ta.columns.iter().zip(ta.column_ranges.iter()) {
            if pos_in(pos, col_range) {
                if let Some(col) = model.columns.get(col_name) {
                    return Some(Location { uri: uri.clone(), range: lsp_range(col.name_range) });
                }
            }
        }
    }

    None
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn resolve_definition(
    uri: &Uri,
    _source: &str,
    pos: Position,
    state: &WorkspaceState,
) -> Option<Location> {
    // ── Alembic path (REQ-DEF-06/07) ─────────────────────────────────────────
    if state.migration_files.contains_key(uri) {
        let mf = state.migration_files.get(uri)?;
        for op in &mf.op_calls {
            if let Some(ref tref) = op.table_name {
                if position_in_range(pos, tref.range) {
                    return resolve_table(tref.name.as_str(), state);
                }
            }
            if let Some(ref cref) = op.column_name {
                if position_in_range(pos, cref.range) {
                    let table_name = op.table_name.as_ref().map(|t| t.name.as_str())?;
                    return resolve_column(table_name, cref.name.as_str(), state);
                }
            }
        }
        return None;
    }

    let file_models = state.file_models.get(uri)?;
    let models = file_models.clone();
    drop(file_models);

    for model in &models {
        if let Some(loc) = check_model(uri, model, pos, state) {
            return Some(loc);
        }
    }

    let tree_ref = state.parse_trees.get(uri)?;
    let source = state.file_sources.get(uri)?;
    let source_bytes = source.as_bytes();
    let root = tree_ref.root_node();
    let cursor_pt = tree_sitter::Point { row: pos.line as usize, column: pos.character as usize };
    if let Some(leaf) = root.descendant_for_point_range(cursor_pt, cursor_pt) {
        let text = crate::parsing::python::node_text(leaf, source_bytes);
        let bare = text.trim_matches('"').trim_matches('\'');
        if state.model_index.contains_key(bare) {
            return model_location(bare, state);
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

fn resolve_table(table: &str, state: &WorkspaceState) -> Option<Location> {
    let model_name = state.table_index.get(table)?;
    let loc = state.model_index.get(&*model_name)?;
    Some(Location {
        uri: loc.uri.clone(),
        range: lsp_range(loc.range),
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
        range: lsp_range(col.name_range),
    })
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::{
        Column, ColumnArgs, ForeignKeyRef, MappedType, Range, Relationship, TableArg,
    };
    use crate::state::WorkspaceState;
    use std::collections::HashMap;
    use tower_lsp_server::ls_types::Uri;

    fn uri(s: &str) -> Uri { s.parse().unwrap() }
    fn rng(sl: u32, sc: u32, el: u32, ec: u32) -> Range { Range { start_line: sl, start_col: sc, end_line: el, end_col: ec } }
    fn pos(line: u32, ch: u32) -> Position { Position { line, character: ch } }

    fn simple_col(name: &str, name_rng: Range) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs::default(),
            foreign_key: None,
            doc: None,
            name_range: name_rng,
            full_range: name_rng,
        }
    }

    fn simple_model(name: &str, table: &str, cols: &[(&str, Range)]) -> Model {
        let columns: HashMap<String, Column> = cols.iter()
            .map(|(n, r)| (n.to_string(), simple_col(n, *r)))
            .collect();
        Model {
            name: name.to_string(),
            table_name: Some(table.to_string()),
            bases: vec![],
            columns,
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(0, 6, 0, 10),
            full_range: rng(0, 0, 30, 0),
        }
    }

    // ── REQ-DEF-01: FK → target column ───────────────────────────────────────

    #[test]
    fn req_def_01_fk_resolves_to_column() {
        let state = WorkspaceState::new();
        let user_u = uri("file:///user.py");
        let user = simple_model("User", "users", &[("id", rng(5, 4, 5, 6))]);
        state.update_file(&user_u, vec![user]);

        let post_u = uri("file:///post.py");
        let fk_range = rng(3, 30, 3, 45);
        let fk = ForeignKeyRef { table: "users".into(), column: "id".into(), raw_text: "users.id".into(), range: fk_range };
        let mut col = simple_col("author_id", rng(3, 4, 3, 13));
        col.foreign_key = Some(fk);
        let mut post = simple_model("Post", "posts", &[]);
        post.columns.insert("author_id".into(), col);
        state.update_file(&post_u, vec![post]);

        let loc = resolve_definition(&post_u, "", pos(3, 35), &state).unwrap();
        assert_eq!(loc.uri, user_u);
        assert_eq!(loc.range.start.line, 5);
    }

    // ── REQ-DEF-02: FK column missing → fall back to model class ─────────────

    #[test]
    fn req_def_02_fk_column_missing_falls_back_to_model() {
        let state = WorkspaceState::new();
        let user_u = uri("file:///user.py");
        let user = simple_model("User", "users", &[]); // no `missing_col`
        state.update_file(&user_u, vec![user]);

        let post_u = uri("file:///post.py");
        let fk_range = rng(3, 30, 3, 50);
        let fk = ForeignKeyRef { table: "users".into(), column: "missing_col".into(), raw_text: "users.missing_col".into(), range: fk_range };
        let mut col = simple_col("x_id", rng(3, 4, 3, 8));
        col.foreign_key = Some(fk);
        let mut post = simple_model("Post", "posts", &[]);
        post.columns.insert("x_id".into(), col);
        state.update_file(&post_u, vec![post]);

        let loc = resolve_definition(&post_u, "", pos(3, 35), &state).unwrap();
        assert_eq!(loc.uri, user_u);
        assert_eq!(loc.range.start.line, 0); // model class range
    }

    // ── REQ-DEF-03: relationship target → model class ─────────────────────────

    #[test]
    fn req_def_03_relationship_target_resolves_to_model() {
        let state = WorkspaceState::new();
        let user_u = uri("file:///user.py");
        let user = simple_model("User", "users", &[]);
        state.update_file(&user_u, vec![user]);

        let post_u = uri("file:///post.py");
        let target_range = rng(6, 22, 6, 28);
        let rel = Relationship {
            name: "author".into(),
            target_model: "User".into(),
            explicit_target: None,
            back_populates: None,
            lazy: None,
            uselist: None,
            secondary: None,
            cascade: None,
            is_list: false,
            backref: None,
            remote_side: false,
            has_foreign_keys: false,
            viewonly: None,
            name_range: rng(6, 4, 6, 10),
            full_range: rng(6, 0, 6, 50),
            target_range: Some(target_range),
            back_populates_range: None,
            cascade_range: None,
        };
        let mut post = simple_model("Post", "posts", &[]);
        post.relationships.insert("author".into(), rel);
        state.update_file(&post_u, vec![post]);

        let loc = resolve_definition(&post_u, "", pos(6, 24), &state).unwrap();
        assert_eq!(loc.uri, user_u);
    }

    // ── REQ-DEF-04: back_populates → counterpart relationship ─────────────────

    #[test]
    fn req_def_04_back_populates_resolves_to_counterpart() {
        let state = WorkspaceState::new();
        let user_u = uri("file:///user.py");
        let mut user = simple_model("User", "users", &[]);
        let posts_rel = Relationship {
            name: "posts".into(),
            target_model: "Post".into(),
            explicit_target: None,
            back_populates: Some("author".into()),
            lazy: None,
            uselist: None,
            secondary: None,
            cascade: None,
            is_list: true,
            backref: None,
            remote_side: false,
            has_foreign_keys: false,
            viewonly: None,
            name_range: rng(8, 4, 8, 9),
            full_range: rng(8, 0, 8, 50),
            target_range: None,
            back_populates_range: None,
            cascade_range: None,
        };
        user.relationships.insert("posts".into(), posts_rel);
        state.update_file(&user_u, vec![user]);

        let post_u = uri("file:///post.py");
        let bp_range = rng(6, 30, 6, 38);
        let rel = Relationship {
            name: "author".into(),
            target_model: "User".into(),
            explicit_target: None,
            back_populates: Some("posts".into()),
            lazy: None,
            uselist: None,
            secondary: None,
            cascade: None,
            is_list: false,
            backref: None,
            remote_side: false,
            has_foreign_keys: false,
            viewonly: None,
            name_range: rng(6, 4, 6, 10),
            full_range: rng(6, 0, 6, 60),
            target_range: None,
            back_populates_range: Some(bp_range),
            cascade_range: None,
        };
        let mut post = simple_model("Post", "posts", &[]);
        post.relationships.insert("author".into(), rel);
        state.update_file(&post_u, vec![post]);

        let loc = resolve_definition(&post_u, "", pos(6, 34), &state).unwrap();
        assert_eq!(loc.uri, user_u);
        assert_eq!(loc.range.start.line, 8); // User.posts name_range
    }

    // ── REQ-DEF-05: __table_args__ column string → column def ─────────────────

    #[test]
    fn req_def_05_table_arg_col_string_resolves_to_column() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let col_range = rng(3, 30, 3, 36);
        let ta = TableArg {
            kind: "Index".into(),
            columns: vec!["email".into()],
            column_ranges: vec![col_range],
            full_range: rng(10, 0, 10, 50),
            name: Some("ix_users_email".into()),
        };
        let mut model = simple_model("User", "users", &[("email", rng(4, 4, 4, 9))]);
        model.table_args = vec![ta];
        state.update_file(&u, vec![model]);

        let loc = resolve_definition(&u, "", pos(3, 32), &state).unwrap();
        assert_eq!(loc.uri, u);
        assert_eq!(loc.range.start.line, 4);
    }

    // ── REQ-DEF-09: unresolved → null ─────────────────────────────────────────

    #[test]
    fn req_def_09_unresolved_fk_returns_none() {
        let state = WorkspaceState::new();
        let post_u = uri("file:///post.py");
        let fk_range = rng(3, 30, 3, 50);
        let fk = ForeignKeyRef { table: "ghost_table".into(), column: "id".into(), raw_text: "ghost_table.id".into(), range: fk_range };
        let mut col = simple_col("ref_id", rng(3, 4, 3, 10));
        col.foreign_key = Some(fk);
        let mut post = simple_model("Post", "posts", &[]);
        post.columns.insert("ref_id".into(), col);
        state.update_file(&post_u, vec![post]);

        assert!(resolve_definition(&post_u, "", pos(3, 35), &state).is_none());
    }

    // ── REQ-DEF-10: plain Python → null ──────────────────────────────────────

    #[test]
    fn req_def_10_no_sa_construct_returns_none() {
        let state = WorkspaceState::new();
        let u = uri("file:///plain.py");
        // No models registered → nothing to resolve
        assert!(resolve_definition(&u, "", pos(0, 5), &state).is_none());
    }
}
