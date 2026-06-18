/// F04 — Context-aware hover cards for SQLAlchemy and Alembic constructs.
///
/// Returns `None` (null response) for non-SQLAlchemy symbols so the companion
/// Python LSP can answer generic Python hover (REQ-HOV-09, constitution P5).
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range, Uri};

use crate::{model::types::Model, state::WorkspaceState};

// ── Range helpers ─────────────────────────────────────────────────────────────

fn model_range(r: crate::model::types::Range) -> Range {
    Range {
        start: Position { line: r.start_line, character: r.start_col },
        end: Position { line: r.end_line, character: r.end_col },
    }
}

fn pos_in(pos: Position, r: crate::model::types::Range) -> bool {
    let after_start = pos.line > r.start_line
        || (pos.line == r.start_line && pos.character >= r.start_col);
    let before_end = pos.line < r.end_line
        || (pos.line == r.end_line && pos.character < r.end_col);
    after_start && before_end
}

// ── Cascade token glossary ────────────────────────────────────────────────────

fn cascade_doc(token: &str) -> Option<&'static str> {
    match token.trim() {
        "save-update" => Some("Cascade session `add()` to related objects when the parent is added."),
        "merge"       => Some("Cascade `session.merge()` to related objects."),
        "expunge"     => Some("Cascade `session.expunge()` to related objects."),
        "delete"      => Some("Delete related objects when the parent is deleted."),
        "delete-orphan" => Some("Delete related objects when they are de-associated from the parent."),
        "refresh-expire" => Some("Cascade `session.refresh()` and `session.expire()` to related objects."),
        "all" => Some("`save-update` + `merge` + `refresh-expire` + `expunge` + `delete`."),
        _ => None,
    }
}

// ── Card renderers ─────────────────────────────────────────────────────────────

fn column_card(model: &Model, col_name: &str, state: &WorkspaceState) -> String {
    let col = match model.columns.get(col_name) {
        Some(c) => c,
        None => return String::new(),
    };
    let table = model.table_name.as_deref().unwrap_or("—");

    let mut md = format!("**{}.{}**", model.name, col_name);
    let pk_flag = if col.args.primary_key { " *(column, pk)*" } else if col.foreign_key.is_some() { " *(column, FK)*" } else { " *(column)*" };
    md.push_str(pk_flag);
    md.push_str("\n\n---\n");
    md.push_str("| | |\n|---|---|\n");
    md.push_str(&format!("| table | `{}` |\n", table));

    // DB-column alias
    if let Some(alias) = &col.key {
        if alias != col_name {
            md.push_str(&format!("| column | `{}` ← aliased (attr `{}`) |\n", alias, col_name));
        }
    }

    // Type
    let mapped = format!("Mapped[{}]", col.mapped_type);
    md.push_str(&format!("| type | `{}` |\n", mapped));

    // Flags
    md.push_str(&format!(
        "| nullable | {} &nbsp;&nbsp; unique &nbsp; {} &nbsp;&nbsp; primary key &nbsp; {} |\n",
        col.args.nullable,
        col.args.unique,
        col.args.primary_key,
    ));

    // Default
    if let Some(ref def) = col.args.default {
        md.push_str(&format!("| default | `{}` |\n", def));
    } else {
        md.push_str("| default | — |\n");
    }

    // Index/constraint membership from __table_args__
    let in_index: Vec<String> = model.table_args.iter()
        .filter(|ta| ta.columns.iter().any(|c| c == col_name))
        .filter_map(|ta| ta.name.clone())
        .collect();
    if !in_index.is_empty() {
        md.push_str(&format!("| index | `{}` |\n", in_index.join(", ")));
    }

    // doc
    if let Some(ref doc) = col.doc {
        md.push_str(&format!("| doc | {} |\n", doc));
    }

    // Cross-file FK target
    if let Some(ref fk) = col.foreign_key {
        let fk_target_model = state.table_index.get(&fk.table).map(|r| r.value().clone());
        let target_line = if let Some(ref mn) = fk_target_model {
            format!("→ `{}.{}` ({}.{})", fk.table, fk.column, mn, fk.column)
        } else {
            format!("→ `{}.{}` *(target not in workspace)*", fk.table, fk.column)
        };
        md.push_str(&format!("| foreign | {} |\n", target_line));

        // Relationships that ride on this FK
        let backing: Vec<String> = model.relationships.values()
            .filter(|rel| fk_target_model.as_deref() == Some(rel.target_model.as_str()))
            .map(|rel| {
                let counterpart = rel.back_populates.as_deref().unwrap_or("?");
                format!("`{}.{}` ↔ `{}.{}`", model.name, rel.name, rel.target_model, counterpart)
            })
            .collect();
        if !backing.is_empty() {
            md.push_str(&format!("| backs | {} |\n", backing.join(", ")));
        }
    }

    // Used-by: FKs from other models pointing at this column + relationships they back
    let col_is_pk = col.args.primary_key;
    if col_is_pk || col.args.unique {
        let mut used_by: Vec<String> = Vec::new();
        for entry in state.file_models.iter() {
            for other_model in entry.value().iter() {
                if other_model.name == model.name { continue; }
                for (other_col_name, other_col) in &other_model.columns {
                    if let Some(ref fk) = other_col.foreign_key {
                        if fk.table == table && fk.column == col_name {
                            used_by.push(format!(
                                "FK  `{}.{}` → `{}.{}`",
                                other_model.table_name.as_deref().unwrap_or(&other_model.name),
                                other_col_name, table, col_name
                            ));
                            // Find relationship that rides on this FK
                            for rel in other_model.relationships.values() {
                                if rel.target_model == model.name {
                                    let bp = rel.back_populates.as_deref().unwrap_or("?");
                                    used_by.push(format!(
                                        "rel `{}.{}` ↔ `{}.{}`",
                                        other_model.name, rel.name, model.name, bp
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        if used_by.is_empty() {
            md.push_str("| used by | — *(no relationship references this column)* |\n");
        } else {
            let cap = 6;
            let total = used_by.len();
            let shown: Vec<&str> = used_by.iter().take(cap).map(|s| s.as_str()).collect();
            let used_cell = if total > cap {
                format!("{} … +{} more", shown.join("<br>"), total - cap)
            } else {
                shown.join("<br>")
            };
            md.push_str(&format!("| used by | {} |\n", used_cell));
        }
    }

    md
}

fn fk_string_card(table: &str, column: &str, state: &WorkspaceState) -> String {
    let mut md = format!("**ForeignKey(`\"{}.{}\"`)**\n\n---\n", table, column);
    if let Some(model_name) = state.table_index.get(table) {
        let model_name = model_name.clone();
        md.push_str(&format!("→ `{}.{}` ({}. {})\n", table, column, model_name, column));
        if let Some(loc) = state.model_index.get(&model_name) {
            let uri_str = loc.uri.to_string();
            md.push_str(&format!("\nDefined in `{}`", uri_str));
        }
    } else {
        md.push_str(&format!("→ `{}.{}` *(target table not in workspace)*\n", table, column));
    }
    md
}

fn relationship_card(model: &Model, rel_name: &str, state: &WorkspaceState) -> String {
    let rel = match model.relationships.get(rel_name) {
        Some(r) => r,
        None => return String::new(),
    };

    let mut md = format!("**{}.{}** *(relationship)*\n\n---\n", model.name, rel_name);
    md.push_str("| | |\n|---|---|\n");

    // Target + cardinality
    let cardinality = if let Some(ref sec) = rel.secondary {
        format!("list[{}] (m2m via `{}`)", rel.target_model, sec)
    } else if rel.is_list {
        format!("list[{}]", rel.target_model)
    } else {
        format!("→ {}", rel.target_model)
    };
    // Verify target exists in index
    if state.model_index.contains_key(&rel.target_model) {
        md.push_str(&format!("| target | {} |\n", cardinality));
    } else {
        md.push_str(&format!("| target | {} *(unresolved)* |\n", rel.target_model));
    }

    if let Some(ref bp) = rel.back_populates {
        md.push_str(&format!("| back_populates | `{}` |\n", bp));
    }
    if let Some(ref lazy) = rel.lazy {
        md.push_str(&format!("| lazy | `{}` |\n", lazy));
    }
    if let Some(ref cascade) = rel.cascade {
        md.push_str(&format!("| cascade | `{}` |\n", cascade));
    }
    if rel.viewonly == Some(true) {
        md.push_str("| viewonly | true |\n");
    }

    md
}

fn back_populates_card(
    target_model: &str,
    counterpart: &str,
    state: &WorkspaceState,
) -> String {
    let loc = match state.model_index.get(target_model) {
        Some(l) => l.clone(),
        None => {
            return format!(
                "**back_populates** `\"{}\"`\n\n*Target model `{}` not found in workspace.*",
                counterpart, target_model
            );
        }
    };
    let file_models = match state.file_models.get(&loc.uri) {
        Some(m) => m.clone(),
        None => {
            return format!(
                "**back_populates** `\"{}\"`\n\n*Target model `{}` not found in workspace.*",
                counterpart, target_model
            );
        }
    };
    let target = match file_models.iter().find(|m| m.name == target_model) {
        Some(m) => m.clone(),
        None => {
            return format!(
                "**back_populates** `\"{}\"`\n\n*Target model `{}` not found in workspace.*",
                counterpart, target_model
            );
        }
    };
    if let Some(rel) = target.relationships.get(counterpart) {
        // Render the counterpart relationship card
        let mut md = format!("**back_populates** `\"{}\"` → **{}.{}**\n\n---\n", counterpart, target_model, counterpart);
        md.push_str("| | |\n|---|---|\n");
        let cardinality = if rel.is_list { format!("list[{}]", rel.target_model) } else { format!("→ {}", rel.target_model) };
        md.push_str(&format!("| target | {} |\n", cardinality));
        if let Some(ref lazy) = rel.lazy {
            md.push_str(&format!("| lazy | `{}` |\n", lazy));
        }
        md
    } else {
        format!(
            "**back_populates** `\"{}\"`\n\n*Counterpart `{}.{}` not found on target model.*",
            counterpart, target_model, counterpart
        )
    }
}

fn cascade_card(cascade_str: &str) -> String {
    let tokens: Vec<&str> = cascade_str.split(',').collect();
    let mut md = format!("**cascade** `\"{}\"`\n\n---\n", cascade_str);
    md.push_str("| Token | Meaning |\n|---|---|\n");
    for tok in &tokens {
        let tok = tok.trim();
        if let Some(doc) = cascade_doc(tok) {
            md.push_str(&format!("| `{}` | {} |\n", tok, doc));
        } else {
            md.push_str(&format!("| `{}` | ⚠ unknown cascade token |\n", tok));
        }
    }
    md
}

fn model_card(model: &Model, state: &WorkspaceState) -> String {
    let table = model.table_name.as_deref().unwrap_or("—");

    let mut md = format!("**class {}(Base)** *(model)*\n\n---\n", model.name);
    md.push_str("| | |\n|---|---|\n");

    if model.table_name.is_some() {
        md.push_str(&format!("| table | `{}` |\n", table));
    } else {
        md.push_str("| table | — *(no __tablename__)* |\n");
    }

    // Column summary: count + highlights + capped preview
    let n_cols = model.columns.len();
    let pk_names: Vec<&str> = model.columns.iter()
        .filter(|(_, c)| c.args.primary_key)
        .map(|(n, _)| n.as_str())
        .collect();
    let unique_names: Vec<&str> = model.columns.iter()
        .filter(|(_, c)| c.args.unique && !c.args.primary_key)
        .map(|(n, _)| n.as_str())
        .collect();
    let fk_count = model.columns.values().filter(|c| c.foreign_key.is_some()).count();

    let mut highlights = Vec::new();
    for pk in &pk_names { highlights.push(format!("pk {}", pk)); }
    for uq in &unique_names { highlights.push(format!("unique {}", uq)); }
    if fk_count > 0 { highlights.push(format!("{} fk", fk_count)); }

    let col_names: Vec<&str> = model.columns.keys().map(|s| s.as_str()).collect();
    let preview_cap = 6usize;
    let preview = if col_names.len() <= preview_cap {
        col_names.join(", ")
    } else {
        let shown = col_names[..preview_cap].join(", ");
        format!("{} … +{} more", shown, col_names.len() - preview_cap)
    };

    let highlights_str = if highlights.is_empty() { String::new() } else { format!("  ·  {}", highlights.join("  ·  ")) };
    md.push_str(&format!("| columns | {}{}  |\n", n_cols, highlights_str));
    md.push_str(&format!("| | {} |\n", preview));

    // Relationship summary
    let n_rels = model.relationships.len();
    if n_rels > 0 {
        let rel_cap = 3usize;
        let rel_previews: Vec<String> = model.relationships.iter().take(rel_cap)
            .map(|(name, rel)| {
                let resolved = if state.model_index.contains_key(&rel.target_model) {
                    rel.target_model.clone()
                } else {
                    format!("{} *(unresolved)*", rel.target_model)
                };
                let kind = if rel.is_list { format!("list[{}]", resolved) } else { format!("→ {}", resolved) };
                format!("`{}` {}", name, kind)
            })
            .collect();
        let rel_summary = if n_rels > rel_cap {
            format!("{}  … +{} more", rel_previews.join("  ·  "), n_rels - rel_cap)
        } else {
            rel_previews.join("  ·  ")
        };
        md.push_str(&format!("| relations | {}  ·  {} |\n", n_rels, rel_summary));
    }

    // Docstring
    if let Some(ref doc) = model.docstring {
        md.push_str(&format!("| doc | {} |\n", doc));
    }

    md
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Return a hover card for the construct at `pos`, or `None` (REQ-HOV-09).
pub fn provide_hover(
    uri: &Uri,
    pos: Position,
    state: &WorkspaceState,
) -> Option<Hover> {
    let file_models = state.file_models.get(uri)?;
    let models = file_models.clone();

    for model in &models {
        // ── REQ-HOV-01 specificity: test innermost ranges first ───────────────

        // 1. Test relationship sub-ranges (back_populates, cascade) — most specific
        for rel in model.relationships.values() {
            // back_populates value range
            if let (Some(bp), Some(bp_range)) = (&rel.back_populates, &rel.back_populates_range) {
                if pos_in(pos, *bp_range) {
                    let md = back_populates_card(&rel.target_model, bp, state);
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: md,
                        }),
                        range: Some(model_range(*bp_range)),
                    });
                }
            }
            // cascade value range
            if let (Some(cas), Some(cas_range)) = (&rel.cascade, &rel.cascade_range) {
                if pos_in(pos, *cas_range) {
                    let md = cascade_card(cas);
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: md,
                        }),
                        range: Some(model_range(*cas_range)),
                    });
                }
            }
        }

        // 2. Test FK-string ranges inside columns (more specific than column name)
        for col in model.columns.values() {
            if let Some(ref fk) = col.foreign_key {
                if pos_in(pos, fk.range) {
                    let md = fk_string_card(&fk.table, &fk.column, state);
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: md,
                        }),
                        range: Some(model_range(fk.range)),
                    });
                }
            }
        }

        // 3. Test column name ranges
        for (col_name, col) in &model.columns {
            if pos_in(pos, col.name_range) {
                let md = column_card(model, col_name, state);
                if md.is_empty() { continue; }
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: md,
                    }),
                    range: Some(model_range(col.name_range)),
                });
            }
        }

        // 4. Test relationship name ranges
        for (rel_name, rel) in &model.relationships {
            if pos_in(pos, rel.name_range) {
                let md = relationship_card(model, rel_name, state);
                if md.is_empty() { continue; }
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: md,
                    }),
                    range: Some(model_range(rel.name_range)),
                });
            }
        }

        // 5. Test model name range (least specific)
        if pos_in(pos, model.name_range) {
            let md = model_card(model, state);
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }),
                range: Some(model_range(model.name_range)),
            });
        }
    }

    // REQ-HOV-09: nothing SQLAlchemy-specific found → return null
    None
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::{
        Column, ColumnArgs, ForeignKeyRef, MappedType, Range, Relationship,
    };
    use crate::state::WorkspaceState;
    use std::collections::HashMap;
    use tower_lsp_server::ls_types::Uri;

    fn uri(s: &str) -> Uri { s.parse().unwrap() }

    fn rng(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range { start_line: sl, start_col: sc, end_line: el, end_col: ec }
    }
    fn pos(line: u32, ch: u32) -> Position { Position { line, character: ch } }

    fn simple_model(name: &str, table: &str, col_name: &str) -> Model {
        let col = Column {
            name: col_name.to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs::default(),
            foreign_key: None,
            doc: None,
            name_range: rng(5, 4, 5, 6),
            full_range: rng(5, 4, 5, 40),
        };
        Model {
            name: name.to_string(),
            table_name: Some(table.to_string()),
            bases: vec![],
            columns: HashMap::from([(col_name.to_string(), col)]),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(0, 6, 0, 10),
            full_range: rng(0, 0, 20, 0),
        }
    }

    // ── REQ-HOV-01: null outside SA constructs ────────────────────────────────

    #[test]
    fn req_hov_01_no_model_returns_none() {
        let state = WorkspaceState::new();
        let u = uri("file:///plain.py");
        assert!(provide_hover(&u, pos(5, 4), &state).is_none());
    }

    #[test]
    fn req_hov_09_non_sa_position_returns_none() {
        let state = WorkspaceState::new();
        let u = uri("file:///model.py");
        let model = simple_model("User", "users", "id");
        state.update_file(&u, vec![model]);
        // Position not matching any column/rel/model range
        assert!(provide_hover(&u, pos(99, 0), &state).is_none());
    }

    // ── REQ-HOV-02: column card renders facts ─────────────────────────────────

    #[test]
    fn req_hov_02_column_card_basic_facts() {
        let state = WorkspaceState::new();
        let u = uri("file:///model.py");
        let mut model = simple_model("User", "users", "email");
        // Make the column unique
        model.columns.get_mut("email").unwrap().args.unique = true;
        model.columns.get_mut("email").unwrap().mapped_type = MappedType::Str;
        state.update_file(&u, vec![model]);

        let hover = provide_hover(&u, pos(5, 4), &state).unwrap();
        let md = match hover.contents {
            HoverContents::Markup(mc) => mc.value,
            _ => panic!("expected markup"),
        };
        assert!(md.contains("User.email"), "{md}");
        assert!(md.contains("users"), "{md}");
        assert!(md.contains("unique"), "{md}");
    }

    // ── REQ-HOV-02: aliased column shows DB name ──────────────────────────────

    #[test]
    fn req_hov_02_aliased_column_shows_db_name() {
        let state = WorkspaceState::new();
        let u = uri("file:///model.py");
        let mut model = simple_model("User", "users", "name");
        let col = model.columns.get_mut("name").unwrap();
        col.key = Some("full_name".to_string());
        state.update_file(&u, vec![model]);

        let hover = provide_hover(&u, pos(5, 4), &state).unwrap();
        let md = match hover.contents { HoverContents::Markup(mc) => mc.value, _ => panic!() };
        assert!(md.contains("full_name"), "{md}");
        assert!(md.contains("aliased"), "{md}");
    }

    // ── REQ-HOV-04: FK string card ────────────────────────────────────────────

    #[test]
    fn req_hov_04_fk_string_card_resolved() {
        let state = WorkspaceState::new();
        let user_u = uri("file:///user.py");
        let user_model = simple_model("User", "users", "id");
        state.update_file(&user_u, vec![user_model]);

        let u = uri("file:///post.py");
        let fk_range = rng(3, 30, 3, 42); // range of the "users.id" string
        let fk = ForeignKeyRef {
            table: "users".to_string(),
            column: "id".to_string(),
            raw_text: "users.id".to_string(),
            range: fk_range,
        };
        let col = Column {
            name: "author_id".to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs::default(),
            foreign_key: Some(fk),
            doc: None,
            name_range: rng(3, 4, 3, 13),
            full_range: rng(3, 0, 3, 50),
        };
        let model = Model {
            name: "Post".to_string(),
            table_name: Some("posts".to_string()),
            bases: vec![],
            columns: HashMap::from([("author_id".to_string(), col)]),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(0, 6, 0, 10),
            full_range: rng(0, 0, 20, 0),
        };
        state.update_file(&u, vec![model]);

        // Position inside the FK string range
        let hover = provide_hover(&u, pos(3, 35), &state).unwrap();
        let md = match hover.contents { HoverContents::Markup(mc) => mc.value, _ => panic!() };
        assert!(md.contains("users"), "{md}");
        assert!(md.contains("id"), "{md}");
        assert!(md.contains("User"), "{md}");
    }

    #[test]
    fn req_hov_04_fk_string_unresolved() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let fk_range = rng(3, 30, 3, 45);
        let fk = ForeignKeyRef {
            table: "missing_table".to_string(),
            column: "id".to_string(),
            raw_text: "missing_table.id".to_string(),
            range: fk_range,
        };
        let col = Column {
            name: "ref_id".to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs::default(),
            foreign_key: Some(fk),
            doc: None,
            name_range: rng(3, 4, 3, 10),
            full_range: rng(3, 0, 3, 50),
        };
        let model = Model {
            name: "Post".to_string(),
            table_name: Some("posts".to_string()),
            bases: vec![],
            columns: HashMap::from([("ref_id".to_string(), col)]),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(0, 6, 0, 10),
            full_range: rng(0, 0, 20, 0),
        };
        state.update_file(&u, vec![model]);

        let hover = provide_hover(&u, pos(3, 35), &state).unwrap();
        let md = match hover.contents { HoverContents::Markup(mc) => mc.value, _ => panic!() };
        assert!(md.contains("not in workspace"), "{md}");
    }

    // ── REQ-HOV-05: relationship card ─────────────────────────────────────────

    #[test]
    fn req_hov_05_relationship_card() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let mut model = simple_model("Post", "posts", "id");
        let rel = Relationship {
            name: "author".to_string(),
            target_model: "User".to_string(),
            explicit_target: None,
            back_populates: Some("posts".to_string()),
            lazy: Some("select".to_string()),
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
            back_populates_range: None,
            cascade_range: None,
        };
        model.relationships.insert("author".to_string(), rel);
        state.update_file(&u, vec![model]);

        // Register User in the index
        let user_u = uri("file:///user.py");
        let user_model = simple_model("User", "users", "id");
        state.update_file(&user_u, vec![user_model]);

        let hover = provide_hover(&u, pos(6, 5), &state).unwrap();
        let md = match hover.contents { HoverContents::Markup(mc) => mc.value, _ => panic!() };
        assert!(md.contains("relationship"), "{md}");
        assert!(md.contains("User"), "{md}");
        assert!(md.contains("select"), "{md}");
        assert!(md.contains("back_populates"), "{md}");
    }

    // ── REQ-HOV-06: cascade card ──────────────────────────────────────────────

    #[test]
    fn req_hov_06_cascade_card_known_tokens() {
        let md = cascade_card("all, delete-orphan");
        assert!(md.contains("all"), "{md}");
        assert!(md.contains("delete-orphan"), "{md}");
        assert!(!md.contains("unknown"), "{md}");
    }

    #[test]
    fn req_hov_06_cascade_card_unknown_token() {
        let md = cascade_card("delete-orphen");
        assert!(md.contains("unknown cascade token"), "{md}");
    }

    // ── REQ-HOV-07: back_populates card ──────────────────────────────────────

    #[test]
    fn req_hov_07_back_populates_card() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let user_u = uri("file:///user.py");
        let mut user_model = simple_model("User", "users", "id");
        let rel = Relationship {
            name: "posts".to_string(),
            target_model: "Post".to_string(),
            explicit_target: None,
            back_populates: Some("author".to_string()),
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
        user_model.relationships.insert("posts".to_string(), rel);
        state.update_file(&user_u, vec![user_model]);

        // back_populates_range on Post.author
        let bp_range = rng(6, 25, 6, 30);
        let mut post_model = simple_model("Post", "posts", "id");
        let rel2 = Relationship {
            name: "author".to_string(),
            target_model: "User".to_string(),
            explicit_target: None,
            back_populates: Some("posts".to_string()),
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
        post_model.relationships.insert("author".to_string(), rel2);
        state.update_file(&u, vec![post_model]);

        let hover = provide_hover(&u, pos(6, 27), &state).unwrap();
        let md = match hover.contents { HoverContents::Markup(mc) => mc.value, _ => panic!() };
        assert!(md.contains("back_populates"), "{md}");
        assert!(md.contains("User"), "{md}");
        assert!(md.contains("posts"), "{md}");
    }

    #[test]
    fn req_hov_07_back_populates_missing_counterpart() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        // User exists but has no "posts" rel
        let user_u = uri("file:///user.py");
        let user_model = simple_model("User", "users", "id");
        state.update_file(&user_u, vec![user_model]);

        let bp_range = rng(6, 25, 6, 35);
        let mut post_model = simple_model("Post", "posts", "id");
        let rel = Relationship {
            name: "author".to_string(),
            target_model: "User".to_string(),
            explicit_target: None,
            back_populates: Some("posts".to_string()),
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
        post_model.relationships.insert("author".to_string(), rel);
        state.update_file(&u, vec![post_model]);

        let hover = provide_hover(&u, pos(6, 27), &state).unwrap();
        let md = match hover.contents { HoverContents::Markup(mc) => mc.value, _ => panic!() };
        assert!(md.contains("not found"), "{md}");
    }

    // ── REQ-HOV-10: model card ────────────────────────────────────────────────

    #[test]
    fn req_hov_10_model_card_shows_table() {
        let state = WorkspaceState::new();
        let u = uri("file:///model.py");
        let model = simple_model("User", "users", "id");
        state.update_file(&u, vec![model]);

        let hover = provide_hover(&u, pos(0, 7), &state).unwrap();
        let md = match hover.contents { HoverContents::Markup(mc) => mc.value, _ => panic!() };
        assert!(md.contains("class User"), "{md}");
        assert!(md.contains("users"), "{md}");
        assert!(md.contains("model"), "{md}");
    }

    #[test]
    fn req_hov_10_model_card_no_tablename() {
        let state = WorkspaceState::new();
        let u = uri("file:///model.py");
        let mut model = simple_model("UserBase", "users", "id");
        model.table_name = None;
        state.update_file(&u, vec![model]);

        let hover = provide_hover(&u, pos(0, 7), &state).unwrap();
        let md = match hover.contents { HoverContents::Markup(mc) => mc.value, _ => panic!() };
        assert!(md.contains("no __tablename__"), "{md}");
    }

    // ── REQ-HOV-01 specificity: FK string range takes precedence over column name ─

    #[test]
    fn req_hov_01_fk_string_more_specific_than_col_name() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        // FK range is nested inside column full_range, but FK range != name_range
        let fk_range = rng(3, 30, 3, 45);
        let fk = ForeignKeyRef {
            table: "users".to_string(),
            column: "id".to_string(),
            raw_text: "users.id".to_string(),
            range: fk_range,
        };
        let col = Column {
            name: "author_id".to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs::default(),
            foreign_key: Some(fk),
            doc: None,
            name_range: rng(3, 4, 3, 13),
            full_range: rng(3, 0, 3, 60),
        };
        let model = Model {
            name: "Post".to_string(),
            table_name: Some("posts".to_string()),
            bases: vec![],
            columns: HashMap::from([("author_id".to_string(), col)]),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(0, 6, 0, 10),
            full_range: rng(0, 0, 20, 0),
        };
        state.update_file(&u, vec![model]);

        // Hover on FK string range → FK string card (not column card)
        let hover = provide_hover(&u, pos(3, 35), &state).unwrap();
        let md = match hover.contents { HoverContents::Markup(mc) => mc.value, _ => panic!() };
        assert!(md.contains("ForeignKey"), "{md}");
        assert!(!md.contains("(column"), "{md}");
    }
}
