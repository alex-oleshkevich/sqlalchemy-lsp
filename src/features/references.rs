use tower_lsp_server::ls_types::{Location, Position, Range, Uri};

use crate::state::WorkspaceState;

// ── Range helpers ─────────────────────────────────────────────────────────────

fn lsp_range(r: crate::model::types::Range) -> Range {
    Range {
        start: Position {
            line: r.start_line,
            character: r.start_col,
        },
        end: Position {
            line: r.end_line,
            character: r.end_col,
        },
    }
}

fn pos_in(pos: Position, r: crate::model::types::Range) -> bool {
    let after =
        pos.line > r.start_line || (pos.line == r.start_line && pos.character >= r.start_col);
    let before = pos.line < r.end_line || (pos.line == r.end_line && pos.character < r.end_col);
    after && before
}

// ── Reference collectors ──────────────────────────────────────────────────────

fn collect_model_refs(
    model_name: &str,
    table_name: Option<&str>,
    state: &WorkspaceState,
) -> Vec<Location> {
    let mut out = Vec::new();
    for entry in state.file_models.iter() {
        let uri = entry.key().clone();
        let file_models = entry.value().clone();
        for m in &file_models {
            for col in m.columns.values() {
                if let Some(ref fk) = col.foreign_key {
                    let matches_table = table_name.is_some_and(|t| fk.table == t);
                    let matches_class = fk.table == model_name;
                    if matches_table || matches_class {
                        out.push(Location {
                            uri: uri.clone(),
                            range: lsp_range(fk.range),
                        });
                    }
                }
            }
            for rel in m.relationships.values() {
                if rel.target_model == model_name {
                    if let Some(target_range) = rel.target_range {
                        out.push(Location {
                            uri: uri.clone(),
                            range: lsp_range(target_range),
                        });
                    }
                }
            }
            if m.bases.iter().any(|b| b == model_name) {
                out.push(Location {
                    uri: uri.clone(),
                    range: lsp_range(m.name_range),
                });
            }
        }
    }
    out
}

fn collect_column_refs(
    model_name: &str,
    table_name: Option<&str>,
    col_name: &str,
    state: &WorkspaceState,
) -> Vec<Location> {
    let mut out = Vec::new();
    for entry in state.file_models.iter() {
        let uri = entry.key().clone();
        let file_models = entry.value().clone();
        for m in &file_models {
            for col in m.columns.values() {
                if let Some(ref fk) = col.foreign_key {
                    let table_ok =
                        table_name.is_some_and(|t| fk.table == t) || fk.table == model_name;
                    if table_ok && fk.column == col_name {
                        out.push(Location {
                            uri: uri.clone(),
                            range: lsp_range(fk.range),
                        });
                    }
                }
            }
        }
    }
    out
}

fn collect_relationship_refs(
    owning_model: &str,
    rel_name: &str,
    state: &WorkspaceState,
) -> Vec<Location> {
    let mut out = Vec::new();
    for entry in state.file_models.iter() {
        let uri = entry.key().clone();
        let file_models = entry.value().clone();
        for m in &file_models {
            for rel in m.relationships.values() {
                if rel.target_model == owning_model
                    && rel.back_populates.as_deref() == Some(rel_name)
                {
                    if let Some(bp_range) = rel.back_populates_range {
                        out.push(Location {
                            uri: uri.clone(),
                            range: lsp_range(bp_range),
                        });
                    }
                }
            }
        }
    }
    out
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide_references(
    uri: &Uri,
    pos: Position,
    include_declaration: bool,
    state: &WorkspaceState,
) -> Vec<Location> {
    let file_models = match state.file_models.get(uri) {
        Some(m) => m.clone(),
        None => return vec![],
    };

    for model in &file_models {
        // REQ-REF-01: model name range
        if pos_in(pos, model.name_range) {
            let table = model.table_name.as_deref();
            let mut refs = collect_model_refs(&model.name, table, state);
            if include_declaration {
                let def_loc = Location {
                    uri: uri.clone(),
                    range: lsp_range(model.name_range),
                };
                refs.insert(0, def_loc);
            }
            return refs;
        }

        // REQ-REF-01: column name range
        for col in model.columns.values() {
            if pos_in(pos, col.name_range) {
                let table = model.table_name.as_deref();
                let mut refs = collect_column_refs(&model.name, table, &col.name, state);
                if include_declaration {
                    let def_loc = Location {
                        uri: uri.clone(),
                        range: lsp_range(col.name_range),
                    };
                    refs.insert(0, def_loc);
                }
                return refs;
            }
        }

        // REQ-REF-01: relationship name range
        for rel in model.relationships.values() {
            if pos_in(pos, rel.name_range) {
                let mut refs = collect_relationship_refs(&model.name, &rel.name, state);
                if include_declaration {
                    let def_loc = Location {
                        uri: uri.clone(),
                        range: lsp_range(rel.name_range),
                    };
                    refs.insert(0, def_loc);
                }
                return refs;
            }
        }
    }

    vec![]
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::{
        Column, ColumnArgs, ForeignKeyRef, MappedType, Model, Range, Relationship,
    };
    use crate::state::WorkspaceState;
    use std::collections::HashMap;

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }
    fn rng(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range {
            start_line: sl,
            start_col: sc,
            end_line: el,
            end_col: ec,
        }
    }
    fn pos(line: u32, ch: u32) -> Position {
        Position {
            line,
            character: ch,
        }
    }

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

    fn fk_col(name: &str, name_rng: Range, fk_table: &str, fk_col: &str, fk_rng: Range) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs::default(),
            foreign_key: Some(ForeignKeyRef {
                table: fk_table.to_string(),
                column: fk_col.to_string(),
                raw_text: format!("{fk_table}.{fk_col}"),
                range: fk_rng,
            }),
            doc: None,
            name_range: name_rng,
            full_range: name_rng,
        }
    }

    fn simple_model(name: &str, table: &str, name_rng: Range) -> Model {
        Model {
            name: name.to_string(),
            table_name: Some(table.to_string()),
            bases: vec![],
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: name_rng,
            full_range: rng(0, 0, 30, 0),
        }
    }

    fn simple_rel(
        name: &str,
        target: &str,
        name_rng: Range,
        target_rng: Option<Range>,
    ) -> Relationship {
        Relationship {
            name: name.to_string(),
            target_model: target.to_string(),
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
            name_range: name_rng,
            full_range: name_rng,
            target_range: target_rng,
            back_populates_range: None,
            cascade_range: None,
        }
    }

    fn bp_rel(name: &str, target: &str, name_rng: Range, bp: &str, bp_rng: Range) -> Relationship {
        Relationship {
            name: name.to_string(),
            target_model: target.to_string(),
            explicit_target: None,
            back_populates: Some(bp.to_string()),
            lazy: None,
            uselist: None,
            secondary: None,
            cascade: None,
            is_list: false,
            backref: None,
            remote_side: false,
            has_foreign_keys: false,
            viewonly: None,
            name_range: name_rng,
            full_range: name_rng,
            target_range: None,
            back_populates_range: Some(bp_rng),
            cascade_range: None,
        }
    }

    // ── REQ-REF-02: FK targeting this model's table ───────────────────────────

    #[test]
    fn req_ref_02_fk_on_model_is_reference() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let user = simple_model("User", "users", rng(0, 6, 0, 10));
        state.update_file(&user_u, vec![user]);

        let post_u = uri("file:///post.py");
        let fk_rng = rng(3, 30, 3, 45);
        let col = fk_col("author_id", rng(3, 4, 3, 13), "users", "id", fk_rng);
        let mut post = simple_model("Post", "posts", rng(0, 6, 0, 10));
        post.columns.insert("author_id".into(), col);
        state.update_file(&post_u, vec![post]);

        let refs = provide_references(&user_u, pos(0, 7), false, &state);
        assert!(
            refs.iter().any(|l| l.uri == post_u
                && l.range.start.line == 3
                && l.range.start.character == 30)
        );
    }

    // ── REQ-REF-02: FK by model class name ───────────────────────────────────

    #[test]
    fn req_ref_02_fk_by_class_name_is_reference() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let user = simple_model("User", "users", rng(0, 6, 0, 10));
        state.update_file(&user_u, vec![user]);

        let post_u = uri("file:///post.py");
        let fk_rng = rng(3, 30, 3, 43);
        let col = fk_col("author_id", rng(3, 4, 3, 13), "User", "id", fk_rng);
        let mut post = simple_model("Post", "posts", rng(0, 6, 0, 10));
        post.columns.insert("author_id".into(), col);
        state.update_file(&post_u, vec![post]);

        let refs = provide_references(&user_u, pos(0, 7), false, &state);
        assert!(refs.iter().any(|l| l.uri == post_u));
    }

    // ── REQ-REF-03: relationship targeting this model ─────────────────────────

    #[test]
    fn req_ref_03_relationship_target_is_reference() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let user = simple_model("User", "users", rng(0, 6, 0, 10));
        state.update_file(&user_u, vec![user]);

        let post_u = uri("file:///post.py");
        let target_rng = rng(6, 22, 6, 28);
        let rel = simple_rel("author", "User", rng(6, 4, 6, 10), Some(target_rng));
        let mut post = simple_model("Post", "posts", rng(0, 6, 0, 10));
        post.relationships.insert("author".into(), rel);
        state.update_file(&post_u, vec![post]);

        let refs = provide_references(&user_u, pos(0, 7), false, &state);
        assert!(
            refs.iter().any(|l| l.uri == post_u
                && l.range.start.line == 6
                && l.range.start.character == 22)
        );
    }

    // ── REQ-REF-04: subclasses inheriting this model ──────────────────────────

    #[test]
    fn req_ref_04_subclass_base_is_reference() {
        let state = WorkspaceState::new();

        let base_u = uri("file:///base.py");
        let base = simple_model("Base", "base_table", rng(0, 6, 0, 10));
        state.update_file(&base_u, vec![base]);

        let child_u = uri("file:///child.py");
        let mut child = simple_model("Child", "children", rng(2, 6, 2, 11));
        child.bases = vec!["Base".into()];
        state.update_file(&child_u, vec![child]);

        let refs = provide_references(&base_u, pos(0, 7), false, &state);
        assert!(
            refs.iter()
                .any(|l| l.uri == child_u && l.range.start.line == 2)
        );
    }

    // ── REQ-REF-05: FK must match both table and column ───────────────────────

    #[test]
    fn req_ref_05_column_refs_match_both_halves() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let id_col = simple_col("id", rng(1, 4, 1, 6));
        let mut user = simple_model("User", "users", rng(0, 6, 0, 10));
        user.columns.insert("id".into(), id_col);
        state.update_file(&user_u, vec![user]);

        let post_u = uri("file:///post.py");
        let author_fk_rng = rng(3, 30, 3, 45);
        let author_col = fk_col("author_id", rng(3, 4, 3, 13), "users", "id", author_fk_rng);
        // A FK to posts.id — should NOT match User.id
        let other_fk_rng = rng(4, 30, 4, 42);
        let other_col = fk_col("post_id", rng(4, 4, 4, 11), "posts", "id", other_fk_rng);
        let mut post = simple_model("Post", "posts", rng(0, 6, 0, 10));
        post.columns.insert("author_id".into(), author_col);
        post.columns.insert("post_id".into(), other_col);
        state.update_file(&post_u, vec![post]);

        let refs = provide_references(&user_u, pos(1, 5), false, &state);
        assert!(
            refs.iter()
                .any(|l| l.range.start.line == 3 && l.range.start.character == 30),
            "should include users.id FK"
        );
        assert!(
            !refs.iter().any(|l| l.range.start.line == 4),
            "should NOT include posts.id FK"
        );
    }

    // ── REQ-REF-06: back_populates → relationship reference ───────────────────

    #[test]
    fn req_ref_06_back_populates_is_reference() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let posts_rel = simple_rel("posts", "Post", rng(5, 4, 5, 9), None);
        let mut user = simple_model("User", "users", rng(0, 6, 0, 10));
        user.relationships.insert("posts".into(), posts_rel);
        state.update_file(&user_u, vec![user]);

        let post_u = uri("file:///post.py");
        let bp_rng = rng(6, 30, 6, 38);
        let author_rel = bp_rel("author", "User", rng(6, 4, 6, 10), "posts", bp_rng);
        let mut post = simple_model("Post", "posts", rng(0, 6, 0, 10));
        post.relationships.insert("author".into(), author_rel);
        state.update_file(&post_u, vec![post]);

        let refs = provide_references(&user_u, pos(5, 6), false, &state);
        assert!(
            refs.iter().any(|l| l.uri == post_u
                && l.range.start.line == 6
                && l.range.start.character == 30)
        );
    }

    // ── REQ-REF-07: no references → empty list ────────────────────────────────

    #[test]
    fn req_ref_07_no_references_returns_empty() {
        let state = WorkspaceState::new();

        let u = uri("file:///user.py");
        let id_col = simple_col("id", rng(1, 4, 1, 6));
        let mut user = simple_model("User", "users", rng(0, 6, 0, 10));
        user.columns.insert("id".into(), id_col);
        state.update_file(&u, vec![user]);

        let refs = provide_references(&u, pos(1, 5), false, &state);
        assert!(refs.is_empty());
    }

    // ── REQ-REF-08: includeDeclaration ───────────────────────────────────────

    #[test]
    fn req_ref_08_include_declaration_adds_def_location() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let user = simple_model("User", "users", rng(0, 6, 0, 10));
        state.update_file(&user_u, vec![user]);

        // No other references — only the declaration
        let refs_with = provide_references(&user_u, pos(0, 7), true, &state);
        assert_eq!(refs_with.len(), 1);
        assert_eq!(refs_with[0].uri, user_u);
        assert_eq!(refs_with[0].range.start.line, 0);

        let refs_without = provide_references(&user_u, pos(0, 7), false, &state);
        assert!(refs_without.is_empty());
    }

    // ── REQ-REF-01: cursor on plain Python → empty list ───────────────────────

    #[test]
    fn req_ref_01_plain_python_returns_empty() {
        let state = WorkspaceState::new();
        // No models registered at all
        let u = uri("file:///plain.py");
        let refs = provide_references(&u, pos(0, 0), false, &state);
        assert!(refs.is_empty());
    }

    // ── REQ-REF-07: column with no table name doesn't over-match ─────────────

    #[test]
    fn req_ref_05_no_table_name_returns_empty() {
        let state = WorkspaceState::new();

        let u = uri("file:///abstract.py");
        let id_col = simple_col("id", rng(1, 4, 1, 6));
        // Model with no table name
        let mut model = Model {
            name: "Abstract".to_string(),
            table_name: None, // no table
            bases: vec![],
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(0, 6, 0, 14),
            full_range: rng(0, 0, 10, 0),
        };
        model.columns.insert("id".into(), id_col);
        state.update_file(&u, vec![model]);

        // Register a FK that matches by column name only (table won't match)
        let post_u = uri("file:///post.py");
        let fk_rng = rng(3, 30, 3, 45);
        let col = fk_col("ref_id", rng(3, 4, 3, 10), "abstract", "id", fk_rng);
        let mut post = simple_model("Post", "posts", rng(0, 6, 0, 10));
        post.columns.insert("ref_id".into(), col);
        state.update_file(&post_u, vec![post]);

        let refs = provide_references(&u, pos(1, 5), false, &state);
        // "abstract" doesn't match "Abstract" (class name) or None (table name) so no match
        assert!(refs.is_empty());
    }
}
