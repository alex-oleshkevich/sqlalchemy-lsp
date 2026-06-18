use tower_lsp_server::ls_types::{
    ParameterInformation, ParameterLabel, Position, SignatureHelp, SignatureInformation, Uri,
};

use crate::{
    model::types::{ColumnArgs, MappedType},
    state::WorkspaceState,
};

// ── Static signatures ─────────────────────────────────────────────────────────

struct SigInfo {
    label: &'static str,
    params: &'static [(&'static str, &'static str)],
}

const OP_SIGNATURES: &[SigInfo] = &[
    SigInfo {
        label: "op.add_column(table_name, column, *, schema=None)",
        params: &[
            ("table_name", "Table to add the column to."),
            ("column", "sa.Column() object to add."),
            ("schema=None", "Optional schema name."),
        ],
    },
    SigInfo {
        label: "op.drop_column(table_name, column_name, *, schema=None)",
        params: &[
            ("table_name", "Table to remove the column from."),
            ("column_name", "Name of the column to remove."),
            ("schema=None", "Optional schema name."),
        ],
    },
    SigInfo {
        label: "op.alter_column(table_name, column_name, *, nullable=None, new_column_name=None, type_=None, existing_type=None)",
        params: &[
            ("table_name", "Table containing the column."),
            ("column_name", "Column to alter."),
            ("nullable=None", "New nullable setting."),
            ("new_column_name=None", "Rename the column."),
            ("type_=None", "New type for the column."),
            ("existing_type=None", "Current type (required by some DBs)."),
        ],
    },
    SigInfo {
        label: "op.create_table(table_name, *columns, **kw)",
        params: &[
            ("table_name", "Name of the new table."),
            ("*columns", "sa.Column() objects."),
            ("**kw", "Additional keyword arguments."),
        ],
    },
    SigInfo {
        label: "op.drop_table(table_name, *, schema=None)",
        params: &[
            ("table_name", "Table to drop."),
            ("schema=None", "Optional schema name."),
        ],
    },
    SigInfo {
        label: "op.create_index(index_name, table_name, columns, *, unique=False, schema=None)",
        params: &[
            ("index_name", "Index name."),
            ("table_name", "Table to index."),
            ("columns", "List of column names or expressions."),
            ("unique=False", "Create a unique index."),
            ("schema=None", "Optional schema name."),
        ],
    },
    SigInfo {
        label: "op.drop_index(index_name, table_name=None, *, schema=None)",
        params: &[
            ("index_name", "Index to drop."),
            ("table_name=None", "Table (required for some dialects)."),
            ("schema=None", "Optional schema name."),
        ],
    },
    SigInfo {
        label: "op.create_unique_constraint(constraint_name, table_name, columns, *, schema=None)",
        params: &[
            ("constraint_name", "Constraint name."),
            ("table_name", "Table to constrain."),
            ("columns", "Columns to include."),
            ("schema=None", "Optional schema name."),
        ],
    },
    SigInfo {
        label: "op.drop_constraint(constraint_name, table_name, *, type_=None, schema=None)",
        params: &[
            ("constraint_name", "Constraint to drop."),
            ("table_name", "Table holding the constraint."),
            ("type_=None", "Constraint type hint."),
            ("schema=None", "Optional schema name."),
        ],
    },
    SigInfo {
        label: "op.create_foreign_key(constraint_name, source_table, referent_table, local_cols, remote_cols)",
        params: &[
            ("constraint_name", "Constraint name."),
            ("source_table", "Table holding the FK."),
            ("referent_table", "Table being referenced."),
            ("local_cols", "Local column names."),
            ("remote_cols", "Referenced column names."),
        ],
    },
    SigInfo {
        label: "op.create_check_constraint(constraint_name, table_name, condition, *, schema=None)",
        params: &[
            ("constraint_name", "Constraint name."),
            ("table_name", "Table to constrain."),
            ("condition", "SQL condition expression."),
            ("schema=None", "Optional schema name."),
        ],
    },
    SigInfo {
        label: "op.rename_table(old_table_name, new_table_name, *, schema=None)",
        params: &[
            ("old_table_name", "Current table name."),
            ("new_table_name", "New table name."),
            ("schema=None", "Optional schema name."),
        ],
    },
    SigInfo {
        label: "op.execute(sqltext, *, execution_options=immutabledict({}))",
        params: &[
            ("sqltext", "SQL string or ClauseElement to execute."),
            ("execution_options=immutabledict({})", "Execution options."),
        ],
    },
    SigInfo {
        label: "op.bulk_insert(table, rows, *, multiinsert=True)",
        params: &[
            ("table", "Table object to insert into."),
            ("rows", "List of dicts to insert."),
            ("multiinsert=True", "Use multi-row INSERT syntax."),
        ],
    },
];

const FK_SIG: SigInfo = SigInfo {
    label: "ForeignKey(target: str)",
    params: &[("target: str", r#"Target column in "table.column" format."#)],
};

const REL_SIG: SigInfo = SigInfo {
    label: "relationship(target, *, back_populates=, lazy=, uselist=, secondary=, cascade=, order_by=, foreign_keys=, viewonly=)",
    params: &[
        ("target", "The target model class or its name as a string."),
        (
            "back_populates=",
            "Attribute name on the target that mirrors this relationship.",
        ),
        (
            "lazy=",
            "Loading strategy: select, joined, subquery, selectin, raise, dynamic, noload.",
        ),
        (
            "uselist=",
            "True for collection, False for scalar (defaults based on cardinality).",
        ),
        (
            "secondary=",
            "Association table for many-to-many relationships.",
        ),
        (
            "cascade=",
            "Cascade rules: save-update, merge, delete, delete-orphan, all.",
        ),
        ("order_by=", "Ordering applied to the collection."),
        (
            "foreign_keys=",
            "Explicit FK columns when multiple FKs ambiguate the join.",
        ),
        ("viewonly=", "True makes the relationship read-only."),
    ],
};

const MC_SIG: SigInfo = SigInfo {
    label: "mapped_column(type_=, *, primary_key=, nullable=, unique=, index=, default=, server_default=, name=, ForeignKey())",
    params: &[
        (
            "type_=",
            "Column type (e.g. String, Integer). Often inferred from Mapped[…].",
        ),
        (
            "primary_key=",
            "True to mark as primary key (default: False).",
        ),
        (
            "nullable=",
            "Allow NULL values (default: True unless primary_key).",
        ),
        ("unique=", "Add a unique constraint (default: False)."),
        ("index=", "Add a column index (default: False)."),
        ("default=", "Python-side default value or callable."),
        ("server_default=", "Server-side default SQL expression."),
        (
            "name=",
            "Explicit DB column name when different from the attribute name.",
        ),
        ("ForeignKey()", "Foreign-key constraint for this column."),
    ],
};

// ── Text helpers ──────────────────────────────────────────────────────────────

/// Return the base name of the innermost unclosed call in `prefix`, plus the
/// byte offset of its opening `(`.
fn innermost_call(prefix: &str) -> Option<(String, usize)> {
    let mut stack: Vec<usize> = Vec::new();
    let mut in_str = false;
    let mut str_char = b'"';
    let bytes = prefix.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if b == str_char {
                in_str = false;
            }
        } else {
            match b {
                b'"' | b'\'' => {
                    in_str = true;
                    str_char = b;
                }
                b'(' => stack.push(i),
                b')' => {
                    stack.pop();
                }
                _ => {}
            }
        }
    }

    let open = *stack.last()?;
    let before = &prefix[..open];
    let trimmed = before.trim_end_matches(|c: char| c.is_whitespace());
    let start = trimmed
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let name = &trimmed[start..];
    if name.is_empty() {
        return None;
    }
    let base = name.split('.').next_back().unwrap_or(name).to_string();
    Some((base, open))
}

/// Count top-level commas from `open` (the `(` position) to the end of `prefix`.
fn active_param_from_open(prefix: &str, open: usize) -> u32 {
    let mut depth = 0i32;
    let mut commas = 0u32;
    let mut in_str = false;
    let mut str_char = b'"';
    for &b in &prefix.as_bytes()[open..] {
        if in_str {
            if b == str_char {
                in_str = false;
            }
        } else {
            match b {
                b'"' | b'\'' => {
                    in_str = true;
                    str_char = b;
                }
                b'(' => depth += 1,
                b')' => depth -= 1,
                b',' if depth == 1 => commas += 1,
                _ => {}
            }
        }
    }
    commas
}

/// When cursor is inside a keyword argument (`key=`), return the key name so
/// we can match it against parameter labels for REQ-SIG-09.
fn kwarg_key_at_cursor(prefix: &str, open: usize) -> Option<String> {
    let inner = &prefix[open..];
    let after_last_comma = inner
        .rfind([',', '('])
        .map(|i| &inner[i + 1..])
        .unwrap_or(inner);
    let trimmed = after_last_comma.trim_start();
    if let Some(eq_pos) = trimmed.find('=') {
        let key: String = trimmed[..eq_pos]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !key.is_empty() {
            return Some(key);
        }
    }
    None
}

fn col_param_label(name: &str, mt: &MappedType, args: &ColumnArgs) -> String {
    let optional = args.nullable || args.default.is_some() || args.server_default.is_some();
    if optional {
        format!("{name}: Optional[{mt}] = …")
    } else {
        format!("{name}: {mt}")
    }
}

fn col_param_doc(args: &ColumnArgs) -> &'static str {
    let optional = args.nullable || args.default.is_some() || args.server_default.is_some();
    if optional { "optional" } else { "required" }
}

// ── Synthesized constructor ───────────────────────────────────────────────────

fn synthesize_constructor(
    model_name: &str,
    prefix: &str,
    open: usize,
    state: &WorkspaceState,
) -> Option<SignatureHelp> {
    let loc = state.model_index.get(model_name)?;
    let uri = loc.uri.clone();
    drop(loc);
    let file_models = state.file_models.get(&uri)?;
    let model = file_models.iter().find(|m| m.name == model_name)?;

    // Sort columns by declaration line for stable order
    let mut cols: Vec<_> = model.columns.values().collect();
    cols.sort_by_key(|c| (c.name_range.start_line, c.name_range.start_col));

    let param_labels: Vec<String> = cols
        .iter()
        .map(|c| col_param_label(&c.name, &c.mapped_type, &c.args))
        .collect();
    let label = format!("{}({})", model_name, param_labels.join(", "));

    let parameters: Vec<ParameterInformation> = cols
        .iter()
        .map(|c| ParameterInformation {
            label: ParameterLabel::Simple(col_param_label(&c.name, &c.mapped_type, &c.args)),
            documentation: Some(tower_lsp_server::ls_types::Documentation::String(
                col_param_doc(&c.args).into(),
            )),
        })
        .collect();

    // REQ-SIG-09: if inside a kwarg, find that column's index
    let positional = active_param_from_open(prefix, open);
    let active = if let Some(key) = kwarg_key_at_cursor(prefix, open) {
        cols.iter()
            .position(|c| c.name == key)
            .map(|i| i as u32)
            .unwrap_or(positional)
    } else {
        positional.min(cols.len().saturating_sub(1) as u32)
    };

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: Some(tower_lsp_server::ls_types::Documentation::String(format!(
                "Synthesized from {model_name}'s mapped columns."
            ))),
            parameters: Some(parameters),
            active_parameter: Some(active),
        }],
        active_signature: Some(0),
        active_parameter: Some(active),
    })
}

// ── Static signature builder ──────────────────────────────────────────────────

fn static_sig(sig: &SigInfo, active: u32) -> SignatureHelp {
    let clamped = active.min(sig.params.len().saturating_sub(1) as u32);
    let parameters = sig
        .params
        .iter()
        .map(|(label, doc)| ParameterInformation {
            label: ParameterLabel::Simple(label.to_string()),
            documentation: Some(tower_lsp_server::ls_types::Documentation::String(
                doc.to_string(),
            )),
        })
        .collect();

    SignatureHelp {
        signatures: vec![SignatureInformation {
            label: sig.label.to_string(),
            documentation: None,
            parameters: Some(parameters),
            active_parameter: Some(clamped),
        }],
        active_signature: Some(0),
        active_parameter: Some(clamped),
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide_signature_help(
    _uri: &Uri,
    source: &str,
    pos: Position,
    state: &WorkspaceState,
) -> Option<SignatureHelp> {
    let line = source.lines().nth(pos.line as usize)?;
    let col = pos.character as usize;
    let prefix = &line[..col.min(line.len())];

    let (base_name, open) = innermost_call(prefix)?;
    let active = active_param_from_open(prefix, open);

    // Dispatch on base name
    match base_name.as_str() {
        "ForeignKey" => Some(static_sig(&FK_SIG, active)),
        "relationship" => {
            // REQ-SIG-09: kwarg-aware active param for relationship too
            let active_kw = if let Some(key) = kwarg_key_at_cursor(prefix, open) {
                REL_SIG
                    .params
                    .iter()
                    .position(|(p, _)| p.starts_with(&key))
                    .map(|i| i as u32)
                    .unwrap_or(active)
            } else {
                active
            };
            Some(static_sig(&REL_SIG, active_kw))
        }
        "mapped_column" => Some(static_sig(&MC_SIG, active)),
        // op.* — only in Alembic context
        name if source.contains("op.") => {
            // Check if this is actually an op.* call by looking at the prefix
            let full_name = {
                // Re-extract to get the dotted name
                let before = &prefix[..open];
                let trimmed = before.trim_end_matches(char::is_whitespace);
                let start = trimmed
                    .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
                    .map(|i| i + 1)
                    .unwrap_or(0);
                trimmed[start..].to_string()
            };
            if let Some(op_name) = full_name.strip_prefix("op.") {
                let sig = OP_SIGNATURES
                    .iter()
                    .find(|s| s.label.starts_with(&format!("op.{op_name}(")))?;
                Some(static_sig(sig, active))
            } else {
                // Not an op call — try model constructor
                synthesize_constructor(name, prefix, open, state)
            }
        }
        name => synthesize_constructor(name, prefix, open, state),
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::{Column, ColumnArgs, MappedType, Range};
    use crate::state::WorkspaceState;
    use std::collections::HashMap;

    fn uri(s: &str) -> tower_lsp_server::ls_types::Uri {
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

    fn col(name: &str, mt: MappedType, nullable: bool, rng: Range) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: mt,
            args: ColumnArgs {
                nullable,
                primary_key: false,
                unique: false,
                index: false,
                default: None,
                server_default: None,
            },
            foreign_key: None,
            doc: None,
            name_range: rng,
            full_range: rng,
        }
    }

    // ── REQ-SIG-04: ForeignKey signature ─────────────────────────────────────

    #[test]
    fn req_sig_04_foreignkey_signature() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let src = "    x = ForeignKey(\"users.|id\")";
        // Cursor after "users. (pos 25 = inside the string)
        let help = provide_signature_help(&u, src, pos(0, 24), &state).unwrap();
        assert!(help.signatures[0].label.contains("ForeignKey(target"));
        assert_eq!(help.active_parameter, Some(0));
    }

    // ── REQ-SIG-05: relationship signature ───────────────────────────────────

    #[test]
    fn req_sig_05_relationship_signature() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let src = "    posts = relationship(\"Post\", back_populates=";
        let help = provide_signature_help(&u, src, pos(0, src.len() as u32), &state).unwrap();
        assert!(help.signatures[0].label.contains("relationship(target"));
        // back_populates is param index 1
        assert_eq!(help.active_parameter, Some(1));
    }

    // ── REQ-SIG-06: mapped_column signature ──────────────────────────────────

    #[test]
    fn req_sig_06_mapped_column_signature() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let src = "    id = mapped_column(";
        let help = provide_signature_help(&u, src, pos(0, src.len() as u32), &state).unwrap();
        assert!(help.signatures[0].label.contains("mapped_column(type_="));
        assert_eq!(help.active_parameter, Some(0));
    }

    // ── REQ-SIG-07/08: synthesized constructor ────────────────────────────────

    #[test]
    fn req_sig_07_08_constructor_synthesized() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");

        let id_col = col("id", MappedType::Int, false, rng(1, 4, 1, 6));
        let email_col = col("email", MappedType::Str, false, rng(2, 4, 2, 9));
        let bio_col = col("bio", MappedType::Str, true, rng(3, 4, 3, 7));

        let mut model = crate::model::types::Model {
            name: "User".to_string(),
            table_name: Some("users".to_string()),
            bases: vec![],
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(0, 6, 0, 10),
            full_range: rng(0, 0, 20, 0),
        };
        model.columns.insert("id".into(), id_col);
        model.columns.insert("email".into(), email_col);
        model.columns.insert("bio".into(), bio_col);
        state.update_file(&u, vec![model]);

        let src = "u = User(";
        let help = provide_signature_help(&u, src, pos(0, src.len() as u32), &state).unwrap();
        let label = &help.signatures[0].label;
        assert!(label.starts_with("User("), "label: {label}");
        assert!(label.contains("id: int"), "label: {label}");
        assert!(label.contains("email: str"), "label: {label}");
        assert!(
            label.contains("Optional"),
            "nullable bio should be optional: {label}"
        );
    }

    // ── REQ-SIG-09: kwarg-aware active param ─────────────────────────────────

    #[test]
    fn req_sig_09_kwarg_active_param() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");

        let id_col = col("id", MappedType::Int, false, rng(1, 4, 1, 6));
        let email_col = col("email", MappedType::Str, false, rng(2, 4, 2, 9));

        let mut model = crate::model::types::Model {
            name: "User".to_string(),
            table_name: Some("users".to_string()),
            bases: vec![],
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(0, 6, 0, 10),
            full_range: rng(0, 0, 20, 0),
        };
        model.columns.insert("id".into(), id_col);
        model.columns.insert("email".into(), email_col);
        state.update_file(&u, vec![model]);

        // Cursor inside email= kwarg (second column alphabetically: email(0), id(1) by sort order)
        // Actually sorted by line: id is line 1, email is line 2 → id=0, email=1
        let src = "u = User(email=";
        let help = provide_signature_help(&u, src, pos(0, src.len() as u32), &state).unwrap();
        // email is the second param (index 1 when sorted by line: id=0, email=1)
        assert_eq!(
            help.active_parameter,
            Some(1),
            "email kwarg should highlight param 1"
        );
    }

    // ── REQ-SIG-10: op.* signatures ──────────────────────────────────────────

    #[test]
    fn req_sig_10_op_add_column() {
        let state = WorkspaceState::new();
        let u = uri("file:///mig.py");
        let src = "    op.add_column(\"posts\", ";
        // source must contain "op." to hit the Alembic branch
        let src_with_op = format!("# alembic op.\n{src}");
        let help =
            provide_signature_help(&u, &src_with_op, pos(1, (src.len()) as u32), &state).unwrap();
        assert!(
            help.signatures[0]
                .label
                .contains("op.add_column(table_name, column")
        );
        assert_eq!(help.active_parameter, Some(1)); // after 1 comma
    }

    // ── REQ-SIG-11: plain Python → None ──────────────────────────────────────

    #[test]
    fn req_sig_11_plain_python_no_signature() {
        let state = WorkspaceState::new();
        let u = uri("file:///plain.py");
        let src = "result = print(";
        assert!(provide_signature_help(&u, src, pos(0, src.len() as u32), &state).is_none());
    }
}
