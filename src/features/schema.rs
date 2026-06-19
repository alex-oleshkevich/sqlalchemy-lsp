use crate::{
    model::types::{Column, MappedType, Model, Relationship},
    state::WorkspaceState,
};

pub fn render_schema(state: &WorkspaceState, format: &str) -> String {
    let models = collect_sorted_models(state);
    match format {
        "graphviz" => render_graphviz(&models),
        "ascii" => render_ascii(&models),
        _ => render_mermaid(&models),
    }
}

fn collect_sorted_models(state: &WorkspaceState) -> Vec<Model> {
    let mut models: Vec<Model> = state
        .file_models
        .iter()
        .flat_map(|entry| {
            entry
                .value()
                .iter()
                .filter(|m| m.table_name.is_some())
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    models.sort_by(|a, b| a.name.cmp(&b.name));
    models
}

fn sorted_columns(model: &Model) -> Vec<&Column> {
    let mut cols: Vec<&Column> = model.columns.values().collect();
    cols.sort_by_key(|c| (c.name_range.start_line, c.name_range.start_col));
    cols
}

fn sorted_rels<'a>(model: &'a Model, known: &[Model]) -> Vec<&'a Relationship> {
    let mut rels: Vec<&Relationship> = model
        .relationships
        .values()
        .filter(|r| known.iter().any(|m| m.name == r.target_model))
        .collect();
    rels.sort_by(|a, b| a.name.cmp(&b.name));
    rels
}

fn schema_type(mt: &MappedType) -> String {
    match mt {
        MappedType::Optional(inner) => schema_type(inner),
        other => other.to_string(),
    }
}

fn mermaid_type(mt: &MappedType) -> String {
    match mt {
        MappedType::Int => "int".to_string(),
        MappedType::Str => "string".to_string(),
        MappedType::Float => "float".to_string(),
        MappedType::Bool => "boolean".to_string(),
        MappedType::DateTime => "datetime".to_string(),
        MappedType::Optional(inner) => mermaid_type(inner),
        MappedType::List(_) => "array".to_string(),
        MappedType::SqlType { name, .. } => name.to_lowercase(),
        MappedType::ForwardRef(s) | MappedType::Unknown(s) => s.to_lowercase(),
    }
}

// ── Mermaid ───────────────────────────────────────────────────────────────────

fn render_mermaid(models: &[Model]) -> String {
    let mut out = String::from(
        "%%{init: {'theme': 'base', 'themeVariables': {'fontSize': '14px'}}}%%\nerDiagram\n",
    );

    if models.is_empty() {
        out.push_str("    %% No SQLAlchemy models found in workspace.\n");
        return out;
    }

    for model in models {
        for rel in sorted_rels(model, models) {
            let card = if rel.is_list && rel.secondary.is_some() {
                "}o--o{"
            } else if rel.is_list {
                "||--o{"
            } else {
                "||--||"
            };
            let from = model.name.to_uppercase();
            let to = rel.target_model.to_uppercase();
            out.push_str(&format!("    {from} {card} {to} : \"{}\"\n", rel.name));
        }
    }

    out.push('\n');

    for model in models {
        let ent = model.name.to_uppercase();
        out.push_str(&format!("    {ent} {{\n"));
        for col in sorted_columns(model) {
            let typ = mermaid_type(&col.mapped_type);
            let mut keys: Vec<&str> = Vec::new();
            if col.args.primary_key {
                keys.push("PK");
            }
            if col.foreign_key.is_some() {
                keys.push("FK");
            }
            if col.args.unique {
                keys.push("UK");
            }
            let key_str = if keys.is_empty() {
                String::new()
            } else {
                format!(" {}", keys.join(" "))
            };
            out.push_str(&format!("        {typ} {}{key_str}\n", col.name));
        }
        out.push_str("    }\n");
    }

    out
}

// ── Graphviz DOT ──────────────────────────────────────────────────────────────

fn render_graphviz(models: &[Model]) -> String {
    let mut out = String::from(
        "digraph schema {\n  rankdir=LR;\n  node [shape=record, fontname=\"monospace\"];\n\n",
    );

    if models.is_empty() {
        out.push_str("  // No SQLAlchemy models found in workspace.\n}\n");
        return out;
    }

    for model in models {
        let table = model.table_name.as_deref().unwrap_or("");
        let mut label = format!("{{{} ({})|", model.name, table);
        for col in sorted_columns(model) {
            let typ = schema_type(&col.mapped_type);
            let mut markers: Vec<&str> = Vec::new();
            if col.args.primary_key {
                markers.push("PK");
            }
            if col.foreign_key.is_some() {
                markers.push("FK");
            }
            if col.args.unique {
                markers.push("UQ");
            }
            let marker_str = if markers.is_empty() {
                String::new()
            } else {
                format!(" ({})", markers.join(", "))
            };
            label.push_str(&format!("{} : {}{marker_str}\\l", col.name, typ));
        }
        label.push('}');
        out.push_str(&format!("  {} [label=\"{label}\"];\n", model.name));
    }

    out.push('\n');

    // Build table→model name map for FK edge resolution
    let model_by_table: std::collections::HashMap<&str, &str> = models
        .iter()
        .filter_map(|m| m.table_name.as_deref().map(|t| (t, m.name.as_str())))
        .collect();

    for model in models {
        for col in sorted_columns(model) {
            if let Some(fk) = &col.foreign_key {
                if let Some(&target) = model_by_table.get(fk.table.as_str()) {
                    let label = format!("{} \u{2192} {}.{}", col.name, fk.table, fk.column);
                    out.push_str(&format!(
                        "  {} -> {} [label=\"{label}\"];\n",
                        model.name, target
                    ));
                }
            }
        }
        for rel in sorted_rels(model, models) {
            if rel.is_list && rel.secondary.is_some() {
                out.push_str(&format!(
                    "  {} -> {} [label=\"{} (m2m)\", dir=both];\n",
                    model.name, rel.target_model, rel.name
                ));
            }
        }
    }

    out.push_str("}\n");
    out
}

// ── ASCII ─────────────────────────────────────────────────────────────────────

fn render_ascii(models: &[Model]) -> String {
    if models.is_empty() {
        return "No SQLAlchemy models found in workspace.\n".to_string();
    }

    let mut out = String::new();
    let mut all_fk_lines: Vec<String> = Vec::new();

    for model in models {
        let table = model.table_name.as_deref().unwrap_or("");
        let header = format!("{} ({})", model.name, table);

        let col_lines: Vec<String> = sorted_columns(model)
            .iter()
            .map(|c| {
                let typ = schema_type(&c.mapped_type);
                let mut flags: Vec<&str> = Vec::new();
                if c.args.primary_key {
                    flags.push("PK");
                }
                if c.foreign_key.is_some() {
                    flags.push("FK");
                }
                if c.args.unique {
                    flags.push("UQ");
                }
                if !c.args.nullable {
                    flags.push("NN");
                }
                if flags.is_empty() {
                    format!("{}: {}", c.name, typ)
                } else {
                    format!("{}: {} [{}]", c.name, typ, flags.join(","))
                }
            })
            .collect();

        let rel_lines: Vec<String> = sorted_rels(model, models)
            .iter()
            .map(|r| {
                if r.is_list && r.secondary.is_some() {
                    format!("{} \u{2192} list[{}] (m2m)", r.name, r.target_model)
                } else if r.is_list {
                    format!("{} \u{2192} list[{}]", r.name, r.target_model)
                } else {
                    format!("{} \u{2192} {}", r.name, r.target_model)
                }
            })
            .collect();

        let content_width = header
            .chars()
            .count()
            .max(
                col_lines
                    .iter()
                    .map(|l| l.chars().count())
                    .max()
                    .unwrap_or(0),
            )
            .max(
                rel_lines
                    .iter()
                    .map(|l| l.chars().count())
                    .max()
                    .unwrap_or(0),
            );
        let inner = content_width + 2;

        let bar: String = "\u{2500}".repeat(inner);
        let top = format!("\u{250C}{bar}\u{2510}");
        let mid = format!("\u{251C}{bar}\u{2524}");
        let bot = format!("\u{2514}{bar}\u{2518}");

        let pad_center = |s: &str| -> String {
            let n = s.chars().count();
            let left = (inner - n) / 2;
            let right = inner - n - left;
            format!(
                "\u{2502}{}{s}{}\u{2502}",
                " ".repeat(left),
                " ".repeat(right)
            )
        };
        let pad_left = |s: &str| -> String {
            let n = s.chars().count();
            let right = inner - 1 - n;
            format!("\u{2502} {s}{}\u{2502}", " ".repeat(right))
        };

        out.push_str(&top);
        out.push('\n');
        out.push_str(&pad_center(&header));
        out.push('\n');
        out.push_str(&mid);
        out.push('\n');
        for line in &col_lines {
            out.push_str(&pad_left(line));
            out.push('\n');
        }
        if !rel_lines.is_empty() {
            out.push_str(&mid);
            out.push('\n');
            for line in &rel_lines {
                out.push_str(&pad_left(line));
                out.push('\n');
            }
        }
        out.push_str(&bot);
        out.push_str("\n\n");

        for col in sorted_columns(model) {
            if let Some(fk) = &col.foreign_key {
                all_fk_lines.push(format!(
                    "  {}.{} \u{2192} {}.{}",
                    model.name, col.name, fk.table, fk.column
                ));
            }
        }
    }

    if !all_fk_lines.is_empty() {
        out.push_str("Foreign Keys:\n");
        for line in &all_fk_lines {
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::{
        Column, ColumnArgs, ForeignKeyRef, MappedType, Model, Range as MRange, Relationship,
    };
    use crate::state::WorkspaceState;
    use std::collections::HashMap;

    fn uri(s: &str) -> tower_lsp_server::ls_types::Uri {
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

    fn mk_col(
        name: &str,
        mt: MappedType,
        pk: bool,
        fk: Option<(&str, &str)>,
        uq: bool,
        nullable: bool,
        line: u32,
    ) -> Column {
        let r = rng(line, 4, line, 40);
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: mt,
            args: ColumnArgs {
                primary_key: pk,
                nullable,
                explicit_nullable_false: false,
                explicit_nullable_true: false,
                unique: uq,
                index: false,
                default: None,
                server_default: None,
            },
            foreign_key: fk.map(|(t, c)| ForeignKeyRef {
                table: t.to_string(),
                column: c.to_string(),
                raw_text: format!("{t}.{c}"),
                range: r,
            }),
            doc: None,
            name_range: r,
            full_range: r,
        }
    }

    fn mk_rel(
        name: &str,
        target: &str,
        is_list: bool,
        secondary: Option<&str>,
        line: u32,
    ) -> Relationship {
        let r = rng(line, 4, line, 60);
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
            string_fk_refs: vec![],
            viewonly: None,
            name_range: r,
            full_range: r,
            target_range: None,
            back_populates_range: None,
            cascade_range: None,
        }
    }

    fn mk_model(name: &str, table: &str, cols: Vec<Column>, rels: Vec<Relationship>) -> Model {
        let mut columns = HashMap::new();
        for c in cols {
            columns.insert(c.name.clone(), c);
        }
        let mut relationships = HashMap::new();
        for r in rels {
            relationships.insert(r.name.clone(), r);
        }
        Model {
            name: name.to_string(),
            table_name: Some(table.to_string()),
            bases: vec![],
            columns,
            relationships,
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(0, 6, 0, 6 + name.len() as u32),
            full_range: rng(0, 0, 20, 0),
        }
    }

    fn build_state(models: Vec<(Model, &str)>) -> WorkspaceState {
        let state = WorkspaceState::new();
        for (model, uri_str) in models {
            state.update_file(&uri(uri_str), vec![model]);
        }
        state
    }

    // ── REQ-SCHEMA-01: all resolved models rendered ──────────────────────────

    #[test]
    fn req_schema_01_includes_all_models() {
        let user = mk_model(
            "User",
            "users",
            vec![mk_col("id", MappedType::Int, true, None, false, false, 1)],
            vec![],
        );
        let post = mk_model(
            "Post",
            "posts",
            vec![mk_col("id", MappedType::Int, true, None, false, false, 1)],
            vec![],
        );
        let state = build_state(vec![(user, "file:///user.py"), (post, "file:///post.py")]);

        let mmd = render_schema(&state, "mermaid");
        assert!(mmd.contains("USER"), "mermaid: USER");
        assert!(mmd.contains("POST"), "mermaid: POST");

        let gv = render_schema(&state, "graphviz");
        assert!(gv.contains("User ["), "graphviz: User node");
        assert!(gv.contains("Post ["), "graphviz: Post node");

        let ascii = render_schema(&state, "ascii");
        assert!(ascii.contains("User (users)"), "ascii: User");
        assert!(ascii.contains("Post (posts)"), "ascii: Post");
    }

    // ── REQ-SCHEMA-02: deterministic/sorted output ───────────────────────────

    #[test]
    fn req_schema_02_deterministic_sorted() {
        let alpha = mk_model("Alpha", "alphas", vec![], vec![]);
        let zeta = mk_model("Zeta", "zetas", vec![], vec![]);
        let state = build_state(vec![(zeta, "file:///z.py"), (alpha, "file:///a.py")]);

        let ascii = render_schema(&state, "ascii");
        let pos_alpha = ascii.find("Alpha").unwrap();
        let pos_zeta = ascii.find("Zeta").unwrap();
        assert!(pos_alpha < pos_zeta, "Alpha must appear before Zeta");

        let mmd = render_schema(&state, "mermaid");
        let pos_a = mmd.find("ALPHA").unwrap();
        let pos_z = mmd.find("ZETA").unwrap();
        assert!(pos_a < pos_z, "ALPHA must appear before ZETA in mermaid");
    }

    // ── REQ-SCHEMA-07: PK/FK/UQ/NN markers ──────────────────────────────────

    #[test]
    fn req_schema_07_ascii_markers() {
        let post = mk_model(
            "Post",
            "posts",
            vec![
                mk_col("id", MappedType::Int, true, None, false, false, 1),
                mk_col(
                    "author_id",
                    MappedType::Int,
                    false,
                    Some(("users", "id")),
                    false,
                    false,
                    2,
                ),
                mk_col("slug", MappedType::Str, false, None, true, false, 3),
                mk_col("body", MappedType::Str, false, None, false, true, 4),
            ],
            vec![],
        );
        let state = build_state(vec![(post, "file:///post.py")]);
        let ascii = render_schema(&state, "ascii");
        assert!(ascii.contains("[PK,NN]"), "id should have PK,NN");
        assert!(ascii.contains("[FK,NN]"), "author_id should have FK,NN");
        assert!(ascii.contains("[UQ,NN]"), "slug should have UQ,NN");
        assert!(
            ascii.contains("body: str\n") || ascii.contains("body: str "),
            "body has no flags"
        );
    }

    #[test]
    fn req_schema_07_mermaid_markers() {
        let model = mk_model(
            "User",
            "users",
            vec![
                mk_col("id", MappedType::Int, true, None, false, false, 1),
                mk_col("email", MappedType::Str, false, None, true, false, 2),
            ],
            vec![],
        );
        let state = build_state(vec![(model, "file:///user.py")]);
        let mmd = render_schema(&state, "mermaid");
        assert!(mmd.contains("int id PK"), "id should have PK");
        assert!(mmd.contains("string email UK"), "email should have UK");
    }

    #[test]
    fn req_schema_07_mermaid_m2m_cardinality() {
        let post = mk_model(
            "Post",
            "posts",
            vec![mk_col("id", MappedType::Int, true, None, false, false, 1)],
            vec![mk_rel("tags", "Tag", true, Some("post_tags"), 5)],
        );
        let tag = mk_model(
            "Tag",
            "tags",
            vec![mk_col("id", MappedType::Int, true, None, false, false, 1)],
            vec![],
        );
        let state = build_state(vec![(post, "file:///post.py"), (tag, "file:///tag.py")]);
        let mmd = render_schema(&state, "mermaid");
        assert!(mmd.contains("}o--o{"), "m2m cardinality glyph");
    }

    #[test]
    fn req_schema_07_ascii_m2m_label() {
        let post = mk_model(
            "Post",
            "posts",
            vec![mk_col("id", MappedType::Int, true, None, false, false, 1)],
            vec![mk_rel("tags", "Tag", true, Some("post_tags"), 5)],
        );
        let tag = mk_model(
            "Tag",
            "tags",
            vec![mk_col("id", MappedType::Int, true, None, false, false, 1)],
            vec![],
        );
        let state = build_state(vec![(post, "file:///post.py"), (tag, "file:///tag.py")]);
        let ascii = render_schema(&state, "ascii");
        assert!(ascii.contains("list[Tag] (m2m)"), "m2m label in ascii");
    }

    // ── REQ-SCHEMA-08: empty/partial workspace ───────────────────────────────

    #[test]
    fn req_schema_08_empty_workspace() {
        let state = WorkspaceState::new();
        assert_eq!(
            render_schema(&state, "ascii"),
            "No SQLAlchemy models found in workspace.\n"
        );
        assert!(render_schema(&state, "mermaid").contains("%% No SQLAlchemy models found"));
        assert!(render_schema(&state, "graphviz").contains("// No SQLAlchemy models found"));
    }

    #[test]
    fn req_schema_08_partial_workspace_omits_unresolved() {
        // Model without table_name should not appear
        let mut no_table = mk_model("Ghost", "ghost_table", vec![], vec![]);
        no_table.table_name = None;
        let resolved = mk_model("User", "users", vec![], vec![]);
        let state = WorkspaceState::new();
        state.update_file(&uri("file:///ghost.py"), vec![no_table]);
        state.update_file(&uri("file:///user.py"), vec![resolved]);
        let ascii = render_schema(&state, "ascii");
        assert!(ascii.contains("User (users)"), "resolved model present");
        assert!(!ascii.contains("Ghost"), "unresolved model absent");
    }

    // ── ASCII foreign keys section ────────────────────────────────────────────

    #[test]
    fn ascii_foreign_keys_section() {
        let post = mk_model(
            "Post",
            "posts",
            vec![
                mk_col("id", MappedType::Int, true, None, false, false, 1),
                mk_col(
                    "author_id",
                    MappedType::Int,
                    false,
                    Some(("users", "id")),
                    false,
                    false,
                    2,
                ),
            ],
            vec![],
        );
        let user = mk_model(
            "User",
            "users",
            vec![mk_col("id", MappedType::Int, true, None, false, false, 1)],
            vec![],
        );
        let state = build_state(vec![(post, "file:///post.py"), (user, "file:///user.py")]);
        let ascii = render_schema(&state, "ascii");
        assert!(ascii.contains("Foreign Keys:"), "FK section header");
        assert!(ascii.contains("Post.author_id"), "FK line");
        assert!(ascii.contains("users.id"), "FK target");
    }

    // ── Unresolved relationship target dropped ────────────────────────────────

    #[test]
    fn unresolved_rel_target_dropped() {
        let post = mk_model(
            "Post",
            "posts",
            vec![mk_col("id", MappedType::Int, true, None, false, false, 1)],
            vec![mk_rel("ghost", "GhostModel", false, None, 5)],
        );
        let state = build_state(vec![(post, "file:///post.py")]);
        let ascii = render_schema(&state, "ascii");
        assert!(
            !ascii.contains("ghost"),
            "unresolved rel dropped from ascii"
        );
        let mmd = render_schema(&state, "mermaid");
        assert!(
            !mmd.contains("GHOSTMODEL"),
            "unresolved rel dropped from mermaid"
        );
    }
}
