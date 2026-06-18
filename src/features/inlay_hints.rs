use tower_lsp_server::ls_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintTooltip, Position, Range, Uri,
};

use crate::state::WorkspaceState;

fn lsp_pos(line: u32, col: u32) -> Position {
    Position {
        line,
        character: col,
    }
}

fn in_range(range: &Range, line: u32) -> bool {
    line >= range.start.line && line <= range.end.line
}

pub fn provide_inlay_hints(uri: &Uri, range: &Range, state: &WorkspaceState) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    let file_models = match state.file_models.get(uri) {
        Some(m) => m,
        None => return hints,
    };

    for model in file_models.iter() {
        // FK column hints
        for col in model.columns.values() {
            let fk = match &col.foreign_key {
                Some(fk) => fk.clone(),
                None => continue,
            };
            if !in_range(range, col.full_range.start_line) {
                continue;
            }
            // Resolve table → model name via table_index
            let model_name = match state.table_index.get(&fk.table) {
                Some(m) => m.clone(),
                None => continue,
            };
            let label = format!("→ {}.{}", model_name, fk.column);
            let tooltip = format!("Foreign key to {}.{}", model_name, fk.column);
            hints.push(InlayHint {
                position: lsp_pos(col.full_range.end_line, col.full_range.end_col),
                label: InlayHintLabel::String(label),
                kind: Some(InlayHintKind::TYPE),
                tooltip: Some(InlayHintTooltip::String(tooltip)),
                padding_left: Some(true),
                padding_right: None,
                text_edits: None,
                data: None,
            });
        }

        // Relationship hints
        for rel in model.relationships.values() {
            if !in_range(range, rel.full_range.start_line) {
                continue;
            }
            // Resolve target model
            if !state.model_index.contains_key(&rel.target_model) {
                continue;
            }
            let label = if rel.is_list {
                if rel.secondary.is_some() {
                    format!("list[{}] (m2m)", rel.target_model)
                } else {
                    format!("list[{}]", rel.target_model)
                }
            } else {
                format!("→ {}", rel.target_model)
            };
            let cardinality = if rel.is_list { "collection" } else { "scalar" };
            let tooltip = format!("Relationship to {} ({cardinality})", rel.target_model);
            hints.push(InlayHint {
                position: lsp_pos(rel.full_range.end_line, rel.full_range.end_col),
                label: InlayHintLabel::String(label),
                kind: Some(InlayHintKind::TYPE),
                tooltip: Some(InlayHintTooltip::String(tooltip)),
                padding_left: Some(true),
                padding_right: None,
                text_edits: None,
                data: None,
            });
        }
    }

    hints
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::{
        Column, ColumnArgs, ForeignKeyRef, MappedType, Model, Range as MRange, Relationship,
    };
    use crate::state::WorkspaceState;
    use std::collections::HashMap;

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }
    fn rng(sl: u32, sc: u32, el: u32, ec: u32) -> MRange {
        MRange {
            start_line: sl,
            start_col: sc,
            end_line: el,
            end_col: ec,
        }
    }
    fn lsp_range(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range {
            start: Position {
                line: sl,
                character: sc,
            },
            end: Position {
                line: el,
                character: ec,
            },
        }
    }

    fn plain_col(name: &str, r: MRange) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs::default(),
            foreign_key: None,
            doc: None,
            name_range: r,
            full_range: r,
        }
    }

    fn fk_col(name: &str, table: &str, column: &str, r: MRange) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs::default(),
            foreign_key: Some(ForeignKeyRef {
                table: table.to_string(),
                column: column.to_string(),
                raw_text: format!("{table}.{column}"),
                range: r,
            }),
            doc: None,
            name_range: r,
            full_range: r,
        }
    }

    fn rel(
        name: &str,
        target: &str,
        is_list: bool,
        secondary: Option<&str>,
        r: MRange,
    ) -> Relationship {
        Relationship {
            name: name.to_string(),
            target_model: target.to_string(),
            explicit_target: None,
            annotation_target: None,
            back_populates: None,
            lazy: None,
            uselist: None,
            secondary: secondary.map(|s| s.to_string()),
            cascade: None,
            is_list,
            backref: None,
            remote_side: false,
            has_foreign_keys: false,
            viewonly: None,
            name_range: r,
            full_range: r,
            target_range: None,
            back_populates_range: None,
            cascade_range: None,
        }
    }

    fn seed_model(state: &WorkspaceState, uri: &Uri, model: Model) {
        state.update_file(uri, vec![model]);
    }

    fn base_model(name: &str, table: &str, line: u32) -> Model {
        Model {
            name: name.to_string(),
            table_name: Some(table.to_string()),
            bases: vec![],
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(line, 6, line, 6 + name.len() as u32),
            full_range: rng(line, 0, line + 10, 0),
        }
    }

    // ── REQ-HINT-01: only in range, only resolved ────────────────────────────

    #[test]
    fn req_hint_01_only_in_range() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");

        // Seed User model so FK resolves
        let user_uri = uri("file:///user.py");
        let user = base_model("User", "users", 0);
        seed_model(&state, &user_uri, user);

        let mut post = base_model("Post", "posts", 0);
        post.columns.insert(
            "author_id".into(),
            fk_col("author_id", "users", "id", rng(3, 0, 3, 60)),
        );
        post.columns
            .insert("title".into(), plain_col("title", rng(10, 0, 10, 40)));
        seed_model(&state, &u, post);

        // Range covers only line 3
        let range = lsp_range(3, 0, 3, 200);
        let hints = provide_inlay_hints(&u, &range, &state);
        assert_eq!(hints.len(), 1);
        // title (line 10) is out of range
    }

    #[test]
    fn req_hint_01_unresolved_fk_no_hint() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");

        let mut post = base_model("Post", "posts", 0);
        // FK to unknown table
        post.columns.insert(
            "x_id".into(),
            fk_col("x_id", "unknown_table", "id", rng(3, 0, 3, 60)),
        );
        seed_model(&state, &u, post);

        let range = lsp_range(0, 0, 100, 0);
        let hints = provide_inlay_hints(&u, &range, &state);
        assert!(hints.is_empty(), "should produce no hint for unresolved FK");
    }

    // ── REQ-HINT-02: FK column renders `→ Model.column` ─────────────────────

    #[test]
    fn req_hint_02_fk_label() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");

        let user_uri = uri("file:///user.py");
        let user = base_model("User", "users", 0);
        seed_model(&state, &user_uri, user);

        let mut post = base_model("Post", "posts", 0);
        post.columns.insert(
            "author_id".into(),
            fk_col("author_id", "users", "id", rng(3, 0, 3, 60)),
        );
        seed_model(&state, &u, post);

        let hints = provide_inlay_hints(&u, &lsp_range(0, 0, 100, 0), &state);
        assert_eq!(hints.len(), 1);
        let label = match &hints[0].label {
            InlayHintLabel::String(s) => s.clone(),
            _ => panic!(),
        };
        assert_eq!(label, "→ User.id");
        if let Some(InlayHintTooltip::String(tip)) = &hints[0].tooltip {
            assert_eq!(tip, "Foreign key to User.id");
        } else {
            panic!("expected tooltip");
        }
    }

    // ── REQ-HINT-03: relationship cardinality labels ─────────────────────────

    #[test]
    fn req_hint_03_cardinality_labels() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");

        // Seed target models
        let user_uri = uri("file:///user.py");
        seed_model(&state, &user_uri, base_model("User", "users", 0));
        let comment_uri = uri("file:///comment.py");
        seed_model(&state, &comment_uri, base_model("Comment", "comments", 0));
        let tag_uri = uri("file:///tag.py");
        seed_model(&state, &tag_uri, base_model("Tag", "tags", 0));

        let mut post = base_model("Post", "posts", 0);
        post.relationships.insert(
            "author".into(),
            rel("author", "User", false, None, rng(5, 0, 5, 60)),
        );
        post.relationships.insert(
            "comments".into(),
            rel("comments", "Comment", true, None, rng(6, 0, 6, 60)),
        );
        post.relationships.insert(
            "tags".into(),
            rel("tags", "Tag", true, Some("post_tags"), rng(7, 0, 7, 60)),
        );
        seed_model(&state, &u, post);

        let hints = provide_inlay_hints(&u, &lsp_range(0, 0, 100, 0), &state);
        assert_eq!(hints.len(), 3);

        let labels: Vec<String> = hints
            .iter()
            .map(|h| match &h.label {
                InlayHintLabel::String(s) => s.clone(),
                _ => panic!(),
            })
            .collect();
        assert!(labels.iter().any(|l| l == "→ User"), "scalar: {labels:?}");
        assert!(
            labels.iter().any(|l| l == "list[Comment]"),
            "collection: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l == "list[Tag] (m2m)"),
            "m2m: {labels:?}"
        );
    }

    // ── REQ-HINT-04: position at end-of-line, kind Type, tooltip ────────────

    #[test]
    fn req_hint_04_position_kind_tooltip() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");

        let user_uri = uri("file:///user.py");
        seed_model(&state, &user_uri, base_model("User", "users", 0));

        let mut post = base_model("Post", "posts", 0);
        post.columns.insert(
            "author_id".into(),
            fk_col("author_id", "users", "id", rng(3, 0, 3, 64)),
        );
        seed_model(&state, &u, post);

        let hints = provide_inlay_hints(&u, &lsp_range(0, 0, 100, 0), &state);
        assert_eq!(hints.len(), 1);
        let h = &hints[0];
        assert_eq!(
            h.position,
            Position {
                line: 3,
                character: 64
            }
        );
        assert_eq!(h.kind, Some(InlayHintKind::TYPE));
        assert_eq!(h.padding_left, Some(true));
        assert!(h.tooltip.is_some());
    }

    // ── non-FK column → no hint ──────────────────────────────────────────────

    #[test]
    fn plain_column_no_hint() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let mut post = base_model("Post", "posts", 0);
        post.columns
            .insert("title".into(), plain_col("title", rng(2, 0, 2, 40)));
        seed_model(&state, &u, post);

        let hints = provide_inlay_hints(&u, &lsp_range(0, 0, 100, 0), &state);
        assert!(hints.is_empty());
    }

    // ── unresolvable relationship target → no hint ──────────────────────────

    #[test]
    fn unresolved_rel_target_no_hint() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let mut post = base_model("Post", "posts", 0);
        post.relationships.insert(
            "ghost".into(),
            rel("ghost", "GhostModel", false, None, rng(4, 0, 4, 60)),
        );
        seed_model(&state, &u, post);

        let hints = provide_inlay_hints(&u, &lsp_range(0, 0, 100, 0), &state);
        assert!(hints.is_empty());
    }
}
