/// F02 — Best-practice lints (SQLA-1xx..6xx).
///
/// Fires on code that is valid but carries a hidden cost or is on its way out.
/// Runs in Pass 2 after the full workspace is indexed.
use tower_lsp_server::ls_types::{
    Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range,
};

use crate::{
    model::types::{MappedType, Model},
    state::WorkspaceState,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_lsp(r: crate::model::types::Range) -> Range {
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

fn diag(
    code: &str,
    severity: DiagnosticSeverity,
    msg: String,
    r: crate::model::types::Range,
) -> Diagnostic {
    Diagnostic {
        range: to_lsp(r),
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        source: Some("sqlalchemy-lsp".to_string()),
        message: msg,
        ..Default::default()
    }
}

fn diag_deprecated(
    code: &str,
    severity: DiagnosticSeverity,
    msg: String,
    r: crate::model::types::Range,
) -> Diagnostic {
    Diagnostic {
        range: to_lsp(r),
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        source: Some("sqlalchemy-lsp".to_string()),
        message: msg,
        tags: Some(vec![DiagnosticTag::DEPRECATED]),
        ..Default::default()
    }
}

/// Patterns that identify a mutable literal default.
fn is_mutable_default(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("dict(")
        || t.starts_with("list(")
        || t.starts_with("set(")
        || (t.starts_with('{') && t.ends_with('}'))
        || (t.starts_with('[') && t.ends_with(']'))
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run all F02 lints on the models for one file and return LSP diagnostics.
///
/// Call from `run_pass2` alongside F01.
pub fn check_file(models: &[Model], state: &WorkspaceState) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();

    for model in models {
        // 1xx — structure & constraints
        w104_missing_primary_key(model, &mut out);
        h106_unnamed_constraint(model, &mut out);
        // 2xx — columns & types
        for col in model.columns.values() {
            h202_optional_without_nullable(model.name.as_str(), col, &mut out);
            w203_mutable_default(model.name.as_str(), col, &mut out);
            w204_default_and_server_default(model.name.as_str(), col, &mut out);
            h205_naive_datetime(model.name.as_str(), col, &mut out);
            // H206 dialect-gated — no-op without config
            // I207 off by default and column.doc always None for now — no-op
        }
        // 3xx — foreign keys
        w304_ambiguous_foreign_keys(model, state, &mut out);
        // 4xx — relationships
        for rel in model.relationships.values() {
            w411_missing_remote_side(model, rel, &mut out);
            w413_non_collection_mapped(model, rel, state, &mut out);
            h414_lazy_select_scalar(model, rel, state, &mut out);
            h415_lazy_joined_collection(model.name.as_str(), rel, &mut out);
        }
        // 5xx — modernization
        for rel in model.relationships.values() {
            w501_legacy_backref(model.name.as_str(), rel, &mut out);
        }
        // W502 / I503 / W504 / I505 / I506 — require extractor features not yet
        // wired (module-level calls, imports, method detection).
        //
        // 6xx — ORM extensions
        // H601 / H602 / H603 — require decorator analysis not yet wired.
    }

    out
}

// ── 1xx — Structure & constraints ─────────────────────────────────────────────

fn w104_missing_primary_key(model: &Model, out: &mut Vec<Diagnostic>) {
    if model.table_name.is_none() {
        return; // abstract or base class — W101 handles this separately
    }
    let has_pk_col = model.columns.values().any(|c| c.args.primary_key);
    let has_pk_constraint = model
        .table_args
        .iter()
        .any(|ta| ta.kind == "PrimaryKeyConstraint");
    if !has_pk_col && !has_pk_constraint {
        out.push(diag(
            "SQLA-W104",
            DiagnosticSeverity::WARNING,
            format!(
                "Model `{}` has no primary key; add primary_key=True to a column \
                 or a PrimaryKeyConstraint",
                model.name
            ),
            model.name_range,
        ));
    }
}

fn h106_unnamed_constraint(model: &Model, out: &mut Vec<Diagnostic>) {
    for ta in &model.table_args {
        // PrimaryKeyConstraint rarely needs an explicit name
        if ta.kind == "PrimaryKeyConstraint" {
            continue;
        }
        if ta.name.is_none() {
            out.push(diag(
                "SQLA-H106",
                DiagnosticSeverity::HINT,
                format!(
                    "{} has no name; add name= or set a naming_convention so \
                     migrations can target it",
                    ta.kind
                ),
                ta.full_range,
            ));
        }
    }
}

// ── 2xx — Columns & types ─────────────────────────────────────────────────────

fn h202_optional_without_nullable(
    model_name: &str,
    col: &crate::model::types::Column,
    out: &mut Vec<Diagnostic>,
) {
    // Fires when annotation says Optional but nullable=False was explicit.
    // `ColumnArgs::nullable` defaults to `true`; we can only detect the
    // explicit-false case by checking if it was set to false.
    if col.args.explicit_nullable_false && matches!(col.mapped_type, MappedType::Optional(_)) {
        out.push(diag(
            "SQLA-H202",
            DiagnosticSeverity::HINT,
            format!(
                "Column `{}.{}` is typed Optional but declared nullable=False; \
                 the annotation and the column disagree",
                model_name, col.name
            ),
            col.name_range,
        ));
    }
}

fn w203_mutable_default(
    model_name: &str,
    col: &crate::model::types::Column,
    out: &mut Vec<Diagnostic>,
) {
    let Some(ref default) = col.args.default else {
        return;
    };
    if is_mutable_default(default) {
        out.push(diag(
            "SQLA-W203",
            DiagnosticSeverity::WARNING,
            format!(
                "Mutable default `{}` on `{}.{}` is shared across rows; \
                 wrap it in a callable, e.g. default=list",
                default, model_name, col.name
            ),
            col.name_range,
        ));
    }
}

fn w204_default_and_server_default(
    model_name: &str,
    col: &crate::model::types::Column,
    out: &mut Vec<Diagnostic>,
) {
    if col.args.default.is_some() && col.args.server_default.is_some() {
        out.push(diag(
            "SQLA-W204",
            DiagnosticSeverity::WARNING,
            format!(
                "Column `{}.{}` sets both default and server_default; \
                 they can diverge — keep one",
                model_name, col.name
            ),
            col.name_range,
        ));
    }
}

fn h205_naive_datetime(
    model_name: &str,
    col: &crate::model::types::Column,
    out: &mut Vec<Diagnostic>,
) {
    let is_naive = match &col.mapped_type {
        MappedType::DateTime => true,
        MappedType::Optional(inner) => matches!(**inner, MappedType::DateTime),
        MappedType::SqlType { name, args } => {
            name.eq_ignore_ascii_case("datetime") && !args.iter().any(|a| a.contains("timezone"))
        }
        _ => false,
    };
    if is_naive {
        out.push(diag(
            "SQLA-H205",
            DiagnosticSeverity::HINT,
            format!(
                "DateTime column `{}.{}` is timezone-naive; pass timezone=True",
                model_name, col.name
            ),
            col.name_range,
        ));
    }
}

// ── 3xx — Foreign keys ────────────────────────────────────────────────────────

fn w304_ambiguous_foreign_keys(model: &Model, state: &WorkspaceState, out: &mut Vec<Diagnostic>) {
    for rel in model.relationships.values() {
        if rel.has_foreign_keys {
            continue; // explicitly disambiguated
        }
        if rel.target_model.is_empty() {
            continue;
        }

        // Count FKs in this model pointing to the target's table or name
        let target_table: Option<String> =
            state.model_index.get(&rel.target_model).and_then(|loc| {
                let uri = &loc.uri;
                let models = state.file_models.get(uri)?;
                models
                    .iter()
                    .find(|m| m.name == rel.target_model)
                    .and_then(|m| m.table_name.clone())
            });

        let fk_count = model
            .columns
            .values()
            .filter(|col| {
                col.foreign_key.as_ref().is_some_and(|fk| {
                    fk.table == rel.target_model
                        || target_table.as_deref().is_some_and(|t| t == fk.table)
                })
            })
            .count();

        if fk_count >= 2 {
            out.push(diag(
                "SQLA-W304",
                DiagnosticSeverity::WARNING,
                format!(
                    "Relationship `{}.{}` is ambiguous: `{}` has {} FKs to `{}`; \
                     add foreign_keys=",
                    model.name, rel.name, model.name, fk_count, rel.target_model
                ),
                rel.name_range,
            ));
        }
    }
}

// ── 4xx — Relationships ───────────────────────────────────────────────────────

fn w411_missing_remote_side(
    model: &Model,
    rel: &crate::model::types::Relationship,
    out: &mut Vec<Diagnostic>,
) {
    if !rel.is_list && rel.target_model == model.name && !rel.remote_side {
        out.push(diag(
            "SQLA-W411",
            DiagnosticSeverity::WARNING,
            format!(
                "Self-referential relationship `{}.{}` needs remote_side= \
                 to orient the self-join",
                model.name, rel.name
            ),
            rel.name_range,
        ));
    }
}

fn w413_non_collection_mapped(
    model: &Model,
    rel: &crate::model::types::Relationship,
    state: &WorkspaceState,
    out: &mut Vec<Diagnostic>,
) {
    // W413 fires when a scalar annotation is used but the relationship role
    // implies a collection. Complement to F01's W404 (uselist contradicts annotation).
    // F01's W404 owns the `uselist=` explicit contradiction; W413 owns the case
    // where the *counterpart*'s type implies collection but the annotation is scalar.
    if rel.is_list {
        return; // already a collection annotation
    }
    if rel.uselist.is_some() {
        return; // handled by F01 W404
    }

    // Check counterpart's shape via back_populates
    let Some(ref bp) = rel.back_populates else {
        return;
    };
    let Some(target_loc) = state.model_index.get(&rel.target_model) else {
        return;
    };
    let uri = target_loc.uri.clone();
    drop(target_loc);
    let Some(file_models) = state.file_models.get(&uri) else {
        return;
    };
    let Some(target_model) = file_models.iter().find(|m| m.name == rel.target_model) else {
        return;
    };
    let Some(target_rel) = target_model.relationships.get(bp) else {
        return;
    };

    // If the counterpart is a list relationship and THIS is scalar, the roles are:
    // target = one-to-many, this = many-to-one. That's normal (scalar is correct here).
    //
    // W413 fires when THIS is scalar but should be a list:
    //   this is_list=false, target is_list=false, both are scalar → fine (one-to-one)
    //   this is_list=false, target is_list=true → this is the "one" side → fine
    //   this is_list=true (already handled above)
    //
    // The problematic case: `posts: Mapped["Post"]` (scalar annotation)
    // but `back_populates` counterpart is a many-to-many → should be list.
    // Concrete trigger: `target.is_list == true && target.back_populates == rel.name`
    // AND target is a many-to-many (secondary != None) → this should also be a list.
    //
    // Simpler reliable case: target has `is_list=true` pointing back at this model
    // with a secondary (many-to-many); the "this" side should also be a list.
    if target_rel.is_list && target_rel.secondary.is_some() {
        out.push(diag(
            "SQLA-W413",
            DiagnosticSeverity::WARNING,
            format!(
                "Relationship `{}.{}` is typed as a scalar but `{}.{}` is a \
                 many-to-many collection; use Mapped[list[{}]]",
                model.name, rel.name, rel.target_model, bp, rel.target_model
            ),
            rel.name_range,
        ));
    }
}

fn h414_lazy_select_scalar(
    model: &Model,
    rel: &crate::model::types::Relationship,
    state: &WorkspaceState,
    out: &mut Vec<Diagnostic>,
) {
    if rel.is_list {
        return;
    }
    // One-to-one relationships (where the counterpart is also scalar) carry
    // a lower N+1 risk than many-to-one. Skip when counterpart is resolvable
    // and also scalar — the truly high-risk case is Post.author (scalar) with
    // counterpart User.posts (list).
    if let Some(bp) = &rel.back_populates {
        if let Some(loc) = state.model_index.get(&rel.target_model) {
            let uri = loc.uri.clone();
            drop(loc);
            if let Some(file_models) = state.file_models.get(&uri) {
                if let Some(target) = file_models.iter().find(|m| m.name == rel.target_model) {
                    if let Some(counterpart) = target.relationships.get(bp) {
                        if !counterpart.is_list {
                            return; // one-to-one pair — skip
                        }
                    }
                }
            }
        }
    }
    let lazy = rel.lazy.as_deref().unwrap_or("select");
    if lazy == "select" {
        out.push(diag(
            "SQLA-H414",
            DiagnosticSeverity::HINT,
            format!(
                "Scalar relationship `{}.{}` uses lazy=\"select\" (N+1 risk); \
                 consider lazy=\"joined\"",
                model.name, rel.name
            ),
            rel.name_range,
        ));
    }
}

fn h415_lazy_joined_collection(
    model_name: &str,
    rel: &crate::model::types::Relationship,
    out: &mut Vec<Diagnostic>,
) {
    if !rel.is_list {
        return;
    }
    if rel.lazy.as_deref() == Some("joined") {
        out.push(diag(
            "SQLA-H415",
            DiagnosticSeverity::HINT,
            format!(
                "Collection relationship `{}.{}` uses lazy=\"joined\" (row blow-up); \
                 prefer lazy=\"selectin\"",
                model_name, rel.name
            ),
            rel.name_range,
        ));
    }
}

// ── 5xx — Modernization ───────────────────────────────────────────────────────

fn w501_legacy_backref(
    model_name: &str,
    rel: &crate::model::types::Relationship,
    out: &mut Vec<Diagnostic>,
) {
    if rel.backref.is_some() {
        out.push(diag_deprecated(
            "SQLA-W501",
            DiagnosticSeverity::WARNING,
            format!(
                "`{}.{}` uses `backref` (legacy); declare an explicit \
                 back_populates pair instead",
                model_name, rel.name
            ),
            rel.name_range,
        ));
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tower_lsp_server::ls_types::{DiagnosticTag, NumberOrString, Uri};

    use super::*;
    use crate::model::types::{
        Column, ColumnArgs, ForeignKeyRef, MappedType, Model, Range, Relationship, TableArg,
    };
    use crate::state::WorkspaceState;

    fn def_range() -> Range {
        Range {
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 10,
        }
    }

    fn bare_model(name: &str, table: Option<&str>) -> Model {
        Model {
            name: name.to_string(),
            table_name: table.map(|s| s.to_string()),
            bases: vec!["Base".to_string()],
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: def_range(),
            full_range: def_range(),
        }
    }

    fn pk_col(name: &str) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs {
                primary_key: true,
                nullable: false,
                explicit_nullable_false: false,
                explicit_nullable_true: false,
                unique: false,
                index: false,
                default: None,
                server_default: None,
            },
            foreign_key: None,
            doc: None,
            name_range: def_range(),
            full_range: def_range(),
        }
    }

    fn plain_col(name: &str) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: MappedType::Str,
            args: ColumnArgs::default(),
            foreign_key: None,
            doc: None,
            name_range: def_range(),
            full_range: def_range(),
        }
    }

    fn rel(name: &str, target: &str, is_list: bool) -> Relationship {
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
            is_list,
            backref: None,
            remote_side: false,
            has_foreign_keys: false,
            viewonly: None,
            name_range: def_range(),
            full_range: def_range(),
            target_range: None,
            back_populates_range: None,
            cascade_range: None,
        }
    }

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }

    fn code(d: &Diagnostic) -> &str {
        match &d.code {
            Some(NumberOrString::String(s)) => s.as_str(),
            _ => "",
        }
    }

    // ── W104 ──────────────────────────────────────────────────────────────────

    #[test]
    fn lint_01_missing_primary_key_fires() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Tag", Some("tags"));
        model.columns.insert("name".to_string(), plain_col("name")); // no PK
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W104"), "{diags:?}");
    }

    #[test]
    fn lint_01_missing_primary_key_silent_with_pk_col() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Tag", Some("tags"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let diags = check_file(&[model], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W104"), "{diags:?}");
    }

    #[test]
    fn lint_01_missing_primary_key_silent_with_pk_constraint() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Tag", Some("tags"));
        model.columns.insert("name".to_string(), plain_col("name"));
        model.table_args.push(TableArg {
            kind: "PrimaryKeyConstraint".to_string(),
            columns: vec!["name".to_string()],
            column_ranges: vec![def_range()],
            full_range: def_range(),
            name: None,
        });
        let diags = check_file(&[model], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W104"), "{diags:?}");
    }

    // ── H106 ──────────────────────────────────────────────────────────────────

    #[test]
    fn lint_03_unnamed_constraint_fires() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        model.table_args.push(TableArg {
            kind: "UniqueConstraint".to_string(),
            columns: vec!["title".to_string()],
            column_ranges: vec![def_range()],
            full_range: def_range(),
            name: None,
        });
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-H106"), "{diags:?}");
    }

    #[test]
    fn lint_03_unnamed_constraint_silent_when_named() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        model.table_args.push(TableArg {
            kind: "UniqueConstraint".to_string(),
            columns: vec!["title".to_string()],
            column_ranges: vec![def_range()],
            full_range: def_range(),
            name: Some("uq_posts_title".to_string()),
        });
        let diags = check_file(&[model], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-H106"), "{diags:?}");
    }

    // ── H202 ──────────────────────────────────────────────────────────────────

    #[test]
    fn lint_05_optional_without_nullable_fires() {
        let state = WorkspaceState::new();
        let mut model = bare_model("User", Some("users"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut col = plain_col("bio");
        col.mapped_type = MappedType::Optional(Box::new(MappedType::Str));
        col.args.nullable = false;
        col.args.explicit_nullable_false = true;
        model.columns.insert("bio".to_string(), col);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-H202"), "{diags:?}");
    }

    // ── W203 ──────────────────────────────────────────────────────────────────

    #[test]
    fn lint_06_mutable_default_fires_for_list_literal() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut col = plain_col("tags_cache");
        col.args.default = Some("[]".to_string());
        model.columns.insert("tags_cache".to_string(), col);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W203"), "{diags:?}");
    }

    #[test]
    fn lint_06_mutable_default_silent_for_callable() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut col = plain_col("tags_cache");
        col.args.default = Some("list".to_string());
        model.columns.insert("tags_cache".to_string(), col);
        let diags = check_file(&[model], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W203"), "{diags:?}");
    }

    // ── W204 ──────────────────────────────────────────────────────────────────

    #[test]
    fn lint_07_default_and_server_default_fires() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut col = plain_col("created_at");
        col.args.default = Some("datetime.utcnow".to_string());
        col.args.server_default = Some("func.now()".to_string());
        model.columns.insert("created_at".to_string(), col);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W204"), "{diags:?}");
    }

    // ── H205 ──────────────────────────────────────────────────────────────────

    #[test]
    fn lint_08_naive_datetime_fires_for_datetime_type() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut col = plain_col("created_at");
        col.mapped_type = MappedType::DateTime;
        model.columns.insert("created_at".to_string(), col);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-H205"), "{diags:?}");
    }

    #[test]
    fn lint_08_naive_datetime_fires_for_sql_datetime_without_tz() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut col = plain_col("created_at");
        col.mapped_type = MappedType::SqlType {
            name: "DateTime".to_string(),
            args: vec![],
        };
        model.columns.insert("created_at".to_string(), col);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-H205"), "{diags:?}");
    }

    #[test]
    fn lint_08_naive_datetime_silent_with_timezone_arg() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut col = plain_col("created_at");
        col.mapped_type = MappedType::SqlType {
            name: "DateTime".to_string(),
            args: vec!["timezone=True".to_string()],
        };
        model.columns.insert("created_at".to_string(), col);
        let diags = check_file(&[model], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-H205"), "{diags:?}");
    }

    // ── W304 ──────────────────────────────────────────────────────────────────

    #[test]
    fn lint_11_ambiguous_foreign_keys_fires_for_two_fks() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let mut user = bare_model("User", Some("users"));
        user.columns.insert("id".to_string(), pk_col("id"));
        state.update_file(&u, vec![user]);

        let mut post = bare_model("Post", Some("posts"));
        post.columns.insert("id".to_string(), pk_col("id"));
        // Two FKs to users
        for col_name in ["author_id", "editor_id"] {
            let mut col = plain_col(col_name);
            col.foreign_key = Some(ForeignKeyRef {
                table: "users".to_string(),
                column: "id".to_string(),
                raw_text: "users.id".to_string(),
                range: def_range(),
            });
            post.columns.insert(col_name.to_string(), col);
        }
        post.relationships
            .insert("author".to_string(), rel("author", "User", false));

        let diags = check_file(&[post], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W304"), "{diags:?}");
    }

    #[test]
    fn lint_11_ambiguous_foreign_keys_silent_with_foreign_keys_arg() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let mut user = bare_model("User", Some("users"));
        user.columns.insert("id".to_string(), pk_col("id"));
        state.update_file(&u, vec![user]);

        let mut post = bare_model("Post", Some("posts"));
        post.columns.insert("id".to_string(), pk_col("id"));
        for col_name in ["author_id", "editor_id"] {
            let mut col = plain_col(col_name);
            col.foreign_key = Some(ForeignKeyRef {
                table: "users".to_string(),
                column: "id".to_string(),
                raw_text: "users.id".to_string(),
                range: def_range(),
            });
            post.columns.insert(col_name.to_string(), col);
        }
        let mut r = rel("author", "User", false);
        r.has_foreign_keys = true; // disambiguated
        post.relationships.insert("author".to_string(), r);

        let diags = check_file(&[post], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W304"), "{diags:?}");
    }

    // ── W411 ──────────────────────────────────────────────────────────────────

    #[test]
    fn lint_13_missing_remote_side_fires_for_self_referential() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Comment", Some("comments"));
        model.columns.insert("id".to_string(), pk_col("id"));
        model
            .relationships
            .insert("parent".to_string(), rel("parent", "Comment", false));
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W411"), "{diags:?}");
    }

    #[test]
    fn lint_13_missing_remote_side_silent_with_remote_side() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Comment", Some("comments"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut r = rel("parent", "Comment", false);
        r.remote_side = true;
        model.relationships.insert("parent".to_string(), r);
        let diags = check_file(&[model], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W411"), "{diags:?}");
    }

    // ── H414 / H415 ───────────────────────────────────────────────────────────

    #[test]
    fn lint_16_lazy_select_scalar_fires() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        model
            .relationships
            .insert("author".to_string(), rel("author", "User", false));
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-H414"), "{diags:?}");
    }

    #[test]
    fn lint_16_lazy_select_scalar_silent_when_lazy_set() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut r = rel("author", "User", false);
        r.lazy = Some("joined".to_string());
        model.relationships.insert("author".to_string(), r);
        let diags = check_file(&[model], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-H414"), "{diags:?}");
    }

    #[test]
    fn lint_17_lazy_joined_collection_fires() {
        let state = WorkspaceState::new();
        let mut model = bare_model("User", Some("users"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut r = rel("posts", "Post", true);
        r.lazy = Some("joined".to_string());
        model.relationships.insert("posts".to_string(), r);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-H415"), "{diags:?}");
    }

    // ── W501 ──────────────────────────────────────────────────────────────────

    #[test]
    fn lint_19_legacy_backref_fires() {
        let state = WorkspaceState::new();
        let mut model = bare_model("User", Some("users"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut r = rel("posts", "Post", true);
        r.backref = Some("author".to_string());
        model.relationships.insert("posts".to_string(), r);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W501"), "{diags:?}");
    }

    #[test]
    fn lint_19_legacy_backref_carries_deprecated_tag() {
        let state = WorkspaceState::new();
        let mut model = bare_model("User", Some("users"));
        model.columns.insert("id".to_string(), pk_col("id"));
        let mut r = rel("posts", "Post", true);
        r.backref = Some("author".to_string());
        model.relationships.insert("posts".to_string(), r);
        let diags = check_file(&[model], &state);
        let d = diags.iter().find(|d| code(d) == "SQLA-W501").unwrap();
        assert!(
            d.tags
                .as_ref()
                .is_some_and(|t| t.contains(&DiagnosticTag::DEPRECATED)),
            "SQLA-W501 must carry the Deprecated tag"
        );
    }

    // ── clean-blog baseline ───────────────────────────────────────────────────

    #[test]
    fn lint_clean_blog_zero_f02_findings() {
        use crate::parsing::extractor::extract_models;
        use std::fs;

        let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/e2e/fixtures/clean_blog");

        let state = WorkspaceState::new();
        let model_files = [
            "models/base.py",
            "models/user.py",
            "models/post.py",
            "models/tag.py",
            "models/comment.py",
        ];
        let mut all_uris = vec![];
        for rel_path in &model_files {
            let path = fixtures.join(rel_path);
            let src = fs::read_to_string(&path).unwrap();
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter_python::LANGUAGE.into())
                .unwrap();
            let tree = parser.parse(&src, None).unwrap();
            let models = extract_models(&src, &tree, &dashmap::DashMap::new());
            let uri: tower_lsp_server::ls_types::Uri =
                format!("file://{}", path.display()).parse().unwrap();
            state.update_file(&uri, models);
            all_uris.push(uri);
        }

        for uri in &all_uris {
            if let Some(models) = state.file_models.get(uri) {
                let diags = check_file(&models, &state);
                assert!(
                    diags.is_empty(),
                    "clean_blog/{:?}: unexpected F02 findings: {diags:?}",
                    uri
                );
            }
        }
    }
}
