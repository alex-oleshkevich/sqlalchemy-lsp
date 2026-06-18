use std::collections::HashMap;

use tower_lsp_server::ls_types::{
    Position, PrepareRenameResponse, Range, TextEdit, Uri, WorkspaceEdit,
};

use crate::{model::types::Model, state::WorkspaceState};

// ── Helpers ───────────────────────────────────────────────────────────────────

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

/// Detect the quote character used at the given byte offset in source.
fn quote_char_at(source: &str, line: u32, col: u32) -> char {
    source
        .lines()
        .nth(line as usize)
        .and_then(|l| l.as_bytes().get(col as usize))
        .and_then(|&b| {
            if b == b'\'' {
                Some('\'')
            } else if b == b'"' {
                Some('"')
            } else {
                None
            }
        })
        .unwrap_or('"')
}

/// Wrap `name` in the same quotes as the source uses at the given position.
fn quoted(source: &str, line: u32, col: u32, name: &str) -> String {
    let q = quote_char_at(source, line, col);
    format!("{q}{name}{q}")
}

/// Convert `CamelCase` → `snake_case` for deriving the new table name from a class name.
fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

// ── Dispatch helpers ──────────────────────────────────────────────────────────

enum Cursor<'a> {
    Model(&'a Model),
    Column { model: &'a Model, col_name: &'a str },
    Relationship { model: &'a Model, rel_name: &'a str },
}

fn dispatch(file_models: &[Model], pos: Position) -> Option<Cursor<'_>> {
    for model in file_models {
        if pos_in(pos, model.name_range) {
            return Some(Cursor::Model(model));
        }
        for (col_name, col) in &model.columns {
            if pos_in(pos, col.name_range) {
                return Some(Cursor::Column { model, col_name });
            }
        }
        for (rel_name, rel) in &model.relationships {
            if pos_in(pos, rel.name_range) {
                return Some(Cursor::Relationship { model, rel_name });
            }
        }
    }
    None
}

// ── prepareRename ──────────────────────────────────────────────────────────────

pub fn prepare_rename(
    uri: &Uri,
    pos: Position,
    state: &WorkspaceState,
) -> Option<PrepareRenameResponse> {
    let file_models = state.file_models.get(uri)?;
    match dispatch(&file_models, pos)? {
        Cursor::Model(m) => Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: lsp_range(m.name_range),
            placeholder: m.name.clone(),
        }),
        Cursor::Column { col_name, model } => {
            let col = model.columns.get(col_name)?;
            Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: lsp_range(col.name_range),
                placeholder: col.name.clone(),
            })
        }
        Cursor::Relationship { rel_name, model } => {
            let rel = model.relationships.get(rel_name)?;
            Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: lsp_range(rel.name_range),
                placeholder: rel.name.clone(),
            })
        }
    }
}

// ── compute_rename ────────────────────────────────────────────────────────────

pub fn compute_rename(
    uri: &Uri,
    pos: Position,
    new_name: &str,
    state: &WorkspaceState,
) -> Option<WorkspaceEdit> {
    let file_models = state.file_models.get(uri)?.clone();
    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();

    let edit = |changes: &mut HashMap<Uri, Vec<TextEdit>>,
                file_uri: Uri,
                r: crate::model::types::Range,
                new_text: String| {
        changes.entry(file_uri).or_default().push(TextEdit {
            range: lsp_range(r),
            new_text,
        });
    };

    match dispatch(&file_models, pos)? {
        // ── Model rename ──────────────────────────────────────────────────────
        Cursor::Model(model) => {
            let old_name = model.name.clone();
            let old_table = model.table_name.clone();
            let new_table = to_snake_case(new_name);

            // Declaration
            edit(
                &mut changes,
                uri.clone(),
                model.name_range,
                new_name.to_string(),
            );

            // Scan all files for FK and relationship references
            for entry in state.file_models.iter() {
                let ref_uri = entry.key().clone();
                let ref_models = entry.value().clone();
                let source = state
                    .file_sources
                    .get(&ref_uri)
                    .map(|s| s.clone())
                    .unwrap_or_default();

                for m in &ref_models {
                    for col in m.columns.values() {
                        if let Some(ref fk) = col.foreign_key {
                            let table_match = old_table.as_deref().is_some_and(|t| fk.table == t);
                            let class_match = fk.table == old_name;
                            if table_match || class_match {
                                let new_table_part =
                                    if class_match { new_name } else { &new_table };
                                let q =
                                    quote_char_at(&source, fk.range.start_line, fk.range.start_col);
                                let new_text = format!("{q}{}.{}{q}", new_table_part, fk.column);
                                edit(&mut changes, ref_uri.clone(), fk.range, new_text);
                            }
                        }
                    }
                    for rel in m.relationships.values() {
                        if rel.target_model == old_name {
                            if let Some(target_range) = rel.target_range {
                                let new_text = quoted(
                                    &source,
                                    target_range.start_line,
                                    target_range.start_col,
                                    new_name,
                                );
                                edit(&mut changes, ref_uri.clone(), target_range, new_text);
                            }
                        }
                    }
                }
            }
        }

        // ── Column rename ─────────────────────────────────────────────────────
        Cursor::Column { model, col_name } => {
            let col = model.columns.get(col_name)?;
            let old_col_name = col.name.clone();
            let old_table = model.table_name.clone();
            let model_name = model.name.clone();

            // Declaration
            edit(
                &mut changes,
                uri.clone(),
                col.name_range,
                new_name.to_string(),
            );

            // FK references
            for entry in state.file_models.iter() {
                let ref_uri = entry.key().clone();
                let ref_models = entry.value().clone();
                let source = state
                    .file_sources
                    .get(&ref_uri)
                    .map(|s| s.clone())
                    .unwrap_or_default();

                for m in &ref_models {
                    for c in m.columns.values() {
                        if let Some(ref fk) = c.foreign_key {
                            let table_ok = old_table.as_deref().is_some_and(|t| fk.table == t)
                                || fk.table == model_name;
                            if table_ok && fk.column == old_col_name {
                                let q =
                                    quote_char_at(&source, fk.range.start_line, fk.range.start_col);
                                let new_text = format!("{q}{}.{}{q}", fk.table, new_name);
                                edit(&mut changes, ref_uri.clone(), fk.range, new_text);
                            }
                        }
                    }
                }
            }
        }

        // ── Relationship rename ───────────────────────────────────────────────
        Cursor::Relationship { model, rel_name } => {
            let rel = model.relationships.get(rel_name)?;
            let old_rel_name = rel.name.clone();
            let owning_model = model.name.clone();

            // Declaration
            edit(
                &mut changes,
                uri.clone(),
                rel.name_range,
                new_name.to_string(),
            );

            // back_populates counterparts
            for entry in state.file_models.iter() {
                let ref_uri = entry.key().clone();
                let ref_models = entry.value().clone();
                let source = state
                    .file_sources
                    .get(&ref_uri)
                    .map(|s| s.clone())
                    .unwrap_or_default();

                for m in &ref_models {
                    for r in m.relationships.values() {
                        if r.target_model == owning_model
                            && r.back_populates.as_deref() == Some(&old_rel_name)
                        {
                            if let Some(bp_range) = r.back_populates_range {
                                let new_text = quoted(
                                    &source,
                                    bp_range.start_line,
                                    bp_range.start_col,
                                    new_name,
                                );
                                edit(&mut changes, ref_uri.clone(), bp_range, new_text);
                            }
                        }
                    }
                }
            }
        }
    }

    if changes.is_empty() {
        return None;
    }
    Some(WorkspaceEdit::new(changes))
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::{Column, ColumnArgs, ForeignKeyRef, MappedType, Range, Relationship};
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

    fn fk_col(
        name: &str,
        name_rng: Range,
        fk_table: &str,
        fk_col_name: &str,
        fk_rng: Range,
    ) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs::default(),
            foreign_key: Some(ForeignKeyRef {
                table: fk_table.to_string(),
                column: fk_col_name.to_string(),
                raw_text: format!("{fk_table}.{fk_col_name}"),
                range: fk_rng,
            }),
            doc: None,
            name_range: name_rng,
            full_range: name_rng,
        }
    }

    fn simple_model(
        name: &str,
        table: Option<&str>,
        name_rng: Range,
    ) -> crate::model::types::Model {
        crate::model::types::Model {
            name: name.to_string(),
            table_name: table.map(|t| t.to_string()),
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

    fn bp_rel(name: &str, target: &str, name_rng: Range, bp: &str, bp_rng: Range) -> Relationship {
        Relationship {
            name: name.to_string(),
            target_model: target.to_string(),
            explicit_target: None,
            annotation_target: None,
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

    fn target_rel(name: &str, target: &str, name_rng: Range, target_rng: Range) -> Relationship {
        Relationship {
            name: name.to_string(),
            target_model: target.to_string(),
            explicit_target: None,
            annotation_target: None,
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
            target_range: Some(target_rng),
            back_populates_range: None,
            cascade_range: None,
        }
    }

    // ── REQ-RN-01: prepareRename returns range + placeholder ──────────────────

    #[test]
    fn req_rn_01_prepare_rename_on_model() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let user = simple_model("User", Some("users"), rng(0, 6, 0, 10));
        state.update_file(&u, vec![user]);

        let resp = prepare_rename(&u, pos(0, 7), &state).unwrap();
        match resp {
            PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } => {
                assert_eq!(placeholder, "User");
                assert_eq!(range.start.line, 0);
            }
            _ => panic!("expected RangeWithPlaceholder"),
        }
    }

    // ── REQ-RN-02: prepareRename returns None on plain Python ─────────────────

    #[test]
    fn req_rn_02_prepare_rename_no_symbol() {
        let state = WorkspaceState::new();
        let u = uri("file:///plain.py");
        assert!(prepare_rename(&u, pos(0, 5), &state).is_none());
    }

    // ── REQ-RN-03: renaming model rewrites class + FK + relationship target ────

    #[test]
    fn req_rn_03_model_rename_rewrites_class_fk_and_rel() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let user = simple_model("User", Some("users"), rng(0, 6, 0, 10));
        state.update_file(&user_u, vec![user]);
        state
            .file_sources
            .insert(user_u.clone(), "class User:\n".to_string());

        let post_u = uri("file:///post.py");
        let fk_rng = rng(3, 30, 3, 42);
        // source for post: the FK starts with "
        let post_src = "class Post:\n    author_id: int\n    author: rel\n    author_id = mapped_column(ForeignKey(\"users.id\"))\n";
        state
            .file_sources
            .insert(post_u.clone(), post_src.to_string());

        let col = fk_col("author_id", rng(3, 4, 3, 13), "users", "id", fk_rng);
        let target_rng = rng(2, 20, 2, 26);
        let rel = target_rel("author", "User", rng(2, 4, 2, 10), target_rng);
        let mut post = simple_model("Post", Some("posts"), rng(0, 6, 0, 10));
        post.columns.insert("author_id".into(), col);
        post.relationships.insert("author".into(), rel);
        state.update_file(&post_u, vec![post]);

        let edit = compute_rename(&user_u, pos(0, 7), "Account", &state).unwrap();
        let changes = edit.changes.unwrap();

        // Declaration rewritten
        let user_edits = changes.get(&user_u).unwrap();
        assert!(user_edits.iter().any(|e| e.new_text == "Account"));

        // FK rewritten (table half only)
        let post_edits = changes.get(&post_u).unwrap();
        assert!(
            post_edits
                .iter()
                .any(|e| e.new_text.contains("account") && e.new_text.contains(".id"))
        );

        // Relationship target rewritten
        assert!(post_edits.iter().any(|e| e.new_text.contains("Account")));
    }

    // ── REQ-RN-04: FK rewrite preserves column half ───────────────────────────

    #[test]
    fn req_rn_04_fk_rewrite_preserves_column() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let user = simple_model("User", Some("users"), rng(0, 6, 0, 10));
        state.update_file(&user_u, vec![user]);
        state
            .file_sources
            .insert(user_u.clone(), "class User:\n".to_string());

        let post_u = uri("file:///post.py");
        let fk_rng = rng(1, 10, 1, 22);
        state
            .file_sources
            .insert(post_u.clone(), "    col = FK(\"users.id\")\n".to_string());
        let col = fk_col("col", rng(1, 4, 1, 7), "users", "id", fk_rng);
        let mut post = simple_model("Post", Some("posts"), rng(0, 6, 0, 10));
        post.columns.insert("col".into(), col);
        state.update_file(&post_u, vec![post]);

        let edit = compute_rename(&user_u, pos(0, 7), "Account", &state).unwrap();
        let changes = edit.changes.unwrap();
        let post_edits = changes.get(&post_u).unwrap();
        // The column half "id" must be preserved
        let fk_edit = post_edits.iter().find(|e| e.range.start.line == 1).unwrap();
        assert!(
            fk_edit.new_text.ends_with(".id\""),
            "column half must be preserved, got: {}",
            fk_edit.new_text
        );
    }

    // ── REQ-RN-05: column rename updates FK column half only ──────────────────

    #[test]
    fn req_rn_05_column_rename_rewrites_matching_fk() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let id_col = simple_col("id", rng(1, 4, 1, 6));
        let mut user = simple_model("User", Some("users"), rng(0, 6, 0, 10));
        user.columns.insert("id".into(), id_col);
        state.update_file(&user_u, vec![user]);
        state
            .file_sources
            .insert(user_u.clone(), "class User:\n    id: int\n".to_string());

        let post_u = uri("file:///post.py");
        let fk_rng = rng(1, 10, 1, 22);
        state
            .file_sources
            .insert(post_u.clone(), "    col = FK(\"users.id\")\n".to_string());
        let col = fk_col("col", rng(1, 4, 1, 7), "users", "id", fk_rng);
        let mut post = simple_model("Post", Some("posts"), rng(0, 6, 0, 10));
        post.columns.insert("col".into(), col);
        state.update_file(&post_u, vec![post]);

        let edit = compute_rename(&user_u, pos(1, 5), "user_id", &state).unwrap();
        let changes = edit.changes.unwrap();

        // Declaration
        let user_edits = changes.get(&user_u).unwrap();
        assert!(user_edits.iter().any(|e| e.new_text == "user_id"));

        // FK column half updated, table half preserved
        let post_edits = changes.get(&post_u).unwrap();
        let fk_edit = post_edits.iter().find(|e| e.range.start.line == 1).unwrap();
        assert!(
            fk_edit.new_text.contains("users."),
            "table half must be preserved"
        );
        assert!(
            fk_edit.new_text.contains("user_id"),
            "column half must be new name"
        );
    }

    // ── REQ-RN-06: relationship rename rewrites back_populates ────────────────

    #[test]
    fn req_rn_06_relationship_rename_rewrites_back_populates() {
        let state = WorkspaceState::new();

        let user_u = uri("file:///user.py");
        let posts_rel = Relationship {
            name: "posts".to_string(),
            target_model: "Post".to_string(),
            explicit_target: None,
            annotation_target: None,
            back_populates: None,
            lazy: None,
            uselist: None,
            secondary: None,
            cascade: None,
            is_list: true,
            backref: None,
            remote_side: false,
            has_foreign_keys: false,
            viewonly: None,
            name_range: rng(5, 4, 5, 9),
            full_range: rng(5, 0, 5, 50),
            target_range: None,
            back_populates_range: None,
            cascade_range: None,
        };
        let mut user = simple_model("User", Some("users"), rng(0, 6, 0, 10));
        user.relationships.insert("posts".into(), posts_rel);
        state.update_file(&user_u, vec![user]);
        state
            .file_sources
            .insert(user_u.clone(), "class User:\n".to_string());

        let post_u = uri("file:///post.py");
        let bp_rng = rng(6, 30, 6, 38);
        state.file_sources.insert(
            post_u.clone(),
            "    r = relationship(User, back_populates=\"posts\")\n".to_string(),
        );
        let author_rel = bp_rel("author", "User", rng(6, 4, 6, 10), "posts", bp_rng);
        let mut post = simple_model("Post", Some("posts"), rng(0, 6, 0, 10));
        post.relationships.insert("author".into(), author_rel);
        state.update_file(&post_u, vec![post]);

        let edit = compute_rename(&user_u, pos(5, 6), "articles", &state).unwrap();
        let changes = edit.changes.unwrap();

        // Declaration
        let user_edits = changes.get(&user_u).unwrap();
        assert!(user_edits.iter().any(|e| e.new_text == "articles"));

        // back_populates value on Post.author
        let post_edits = changes.get(&post_u).unwrap();
        assert!(post_edits.iter().any(|e| e.new_text.contains("articles")));
    }

    // ── REQ-RN-07: all edits in one WorkspaceEdit ─────────────────────────────

    #[test]
    fn req_rn_07_single_workspace_edit() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let user = simple_model("User", Some("users"), rng(0, 6, 0, 10));
        state.update_file(&u, vec![user]);
        state
            .file_sources
            .insert(u.clone(), "class User:\n".to_string());

        let edit = compute_rename(&u, pos(0, 7), "Account", &state).unwrap();
        // One WorkspaceEdit returned (not multiple separate calls)
        assert!(edit.changes.is_some());
    }
}
