/// F01 — ORM correctness diagnostics (SQLA-1xx..4xx).
///
/// Pure function of the workspace index — takes the models for one file plus
/// the global state and returns LSP diagnostics.  Runs in Pass 2 so that
/// cross-file lookups (FK/relationship resolution) see the full index.
use std::collections::{HashMap, HashSet};

use tower_lsp_server::ls_types::{
    Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range,
};

use crate::{
    model::types::{MappedType, Model},
    state::WorkspaceState,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const SA_ABSTRACT_BASES: &[&str] =
    &["DeclarativeBase", "DeclarativeBaseNoMeta", "MappedAsDataclass"];

const VALID_CASCADE: &[&str] = &[
    "all",
    "save-update",
    "merge",
    "expunge",
    "delete",
    "delete-orphan",
    "refresh-expire",
];

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_lsp(r: crate::model::types::Range) -> Range {
    Range {
        start: Position { line: r.start_line, character: r.start_col },
        end: Position { line: r.end_line, character: r.end_col },
    }
}

fn d(
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

fn is_optional(t: &MappedType) -> bool {
    matches!(t, MappedType::Optional(_))
}

fn base_type(t: &MappedType) -> &MappedType {
    if let MappedType::Optional(inner) = t { base_type(inner) } else { t }
}

/// Compare two MappedType values by their outer discriminant (ignoring Optional).
/// Returns `None` when either side is Unknown (per P4: silence on the unresolvable).
fn types_match(a: &MappedType, b: &MappedType) -> Option<bool> {
    let a = base_type(a);
    let b = base_type(b);
    match (a, b) {
        (MappedType::Unknown(_), _) | (_, MappedType::Unknown(_)) => None,
        _ => Some(std::mem::discriminant(a) == std::mem::discriminant(b)),
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run all F01 rules on the models for one file and return LSP diagnostics.
///
/// Call from `run_pass2` after the full workspace is indexed.
pub fn check_file(models: &[Model], state: &WorkspaceState) -> Vec<Diagnostic> {
    // Build a workspace-wide table → owners map for E102 (last-writer-wins
    // table_index is not enough — we need to detect when two models claim the
    // same table, regardless of index order).
    let mut table_owners: HashMap<String, Vec<String>> = HashMap::new();
    for entry in state.file_models.iter() {
        for m in entry.value().iter() {
            if let Some(ref t) = m.table_name {
                table_owners.entry(t.clone()).or_default().push(m.name.clone());
            }
        }
    }

    let mut out: Vec<Diagnostic> = Vec::new();

    for model in models {
        // 1xx — structure & constraints
        w101_missing_tablename(model, &mut out);
        e102_duplicate_tablename(model, &table_owners, &mut out);
        e103_duplicate_column(model, &mut out);
        e105_table_arg_column_not_found(model, &mut out);
        // 2xx — columns & types
        w201_nullable_not_optional(model, &mut out);
        // 3xx — foreign keys
        e301_unknown_fk_table(model, state, &mut out);
        e302_fk_column_not_found(model, state, &mut out);
        w303_fk_type_mismatch(model, state, &mut out);
        // 4xx — relationships
        e401_rel_target_not_found(model, state, &mut out);
        w402_w403_back_populates(model, state, &mut out);
        w404_uselist_mismatch(model, &mut out);
        w405_target_mismatch(model, &mut out);
        h406_missing_fk_for_relationship(model, state, &mut out);
        h407_unique_missing_one_to_one(model, state, &mut out);
        w408_unknown_cascade(model, &mut out);
        w409_orphan_without_delete(model, &mut out);
    }

    // H410 — whole-file cycle pass
    h410_circular_relationship(models, &mut out);

    out
}

// ── 1xx — Structure & constraints ─────────────────────────────────────────────

fn w101_missing_tablename(model: &Model, out: &mut Vec<Diagnostic>) {
    if model.table_name.is_some() {
        return;
    }
    let is_abstract = model.bases.iter().any(|b| SA_ABSTRACT_BASES.contains(&b.as_str()));
    if !is_abstract {
        out.push(d(
            "SQLA-W101",
            DiagnosticSeverity::WARNING,
            format!("Model `{}` has no __tablename__", model.name),
            model.name_range,
        ));
    }
}

fn e102_duplicate_tablename(
    model: &Model,
    table_owners: &HashMap<String, Vec<String>>,
    out: &mut Vec<Diagnostic>,
) {
    let Some(ref table) = model.table_name else { return };
    let Some(owners) = table_owners.get(table) else { return };
    if owners.len() <= 1 {
        return;
    }
    if let Some(other) = owners.iter().find(|n| *n != &model.name) {
        out.push(d(
            "SQLA-E102",
            DiagnosticSeverity::ERROR,
            format!("Duplicate table name `{}` (also used by `{}`)", table, other),
            model.name_range,
        ));
    }
}

fn e103_duplicate_column(model: &Model, out: &mut Vec<Diagnostic>) {
    for (col_name, range) in &model.duplicate_columns {
        out.push(d(
            "SQLA-E103",
            DiagnosticSeverity::ERROR,
            format!("Duplicate column `{}` on model `{}`", col_name, model.name),
            *range,
        ));
    }
}

fn e105_table_arg_column_not_found(model: &Model, out: &mut Vec<Diagnostic>) {
    for table_arg in &model.table_args {
        for (col_name, range) in
            table_arg.columns.iter().zip(table_arg.column_ranges.iter())
        {
            if !model.columns.contains_key(col_name) {
                out.push(d(
                    "SQLA-E105",
                    DiagnosticSeverity::ERROR,
                    format!(
                        "Column `{}` not found on model `{}` (in {})",
                        col_name, model.name, table_arg.kind
                    ),
                    *range,
                ));
            }
        }
    }
}

// ── 2xx — Columns & types ─────────────────────────────────────────────────────

fn w201_nullable_not_optional(model: &Model, out: &mut Vec<Diagnostic>) {
    for col in model.columns.values() {
        if col.foreign_key.is_some()
            && col.args.nullable
            && !col.args.primary_key
            && !is_optional(&col.mapped_type)
            && !matches!(col.mapped_type, MappedType::Unknown(_))
        {
            out.push(d(
                "SQLA-W201",
                DiagnosticSeverity::WARNING,
                format!("Column `{}` is nullable but type is not Optional", col.name),
                col.name_range,
            ));
        }
    }
}

// ── 3xx — Foreign keys ────────────────────────────────────────────────────────

fn e301_unknown_fk_table(model: &Model, state: &WorkspaceState, out: &mut Vec<Diagnostic>) {
    for col in model.columns.values() {
        let Some(ref fk) = col.foreign_key else { continue };
        if !state.table_index.contains_key(&fk.table)
            && !state.model_index.contains_key(&fk.table)
        {
            out.push(d(
                "SQLA-E301",
                DiagnosticSeverity::ERROR,
                format!("Foreign key references unknown table `{}`", fk.table),
                fk.range,
            ));
        }
    }
}

fn e302_fk_column_not_found(model: &Model, state: &WorkspaceState, out: &mut Vec<Diagnostic>) {
    for col in model.columns.values() {
        let Some(ref fk) = col.foreign_key else { continue };
        // Resolve the target model name
        let target_name = state
            .table_index
            .get(&fk.table)
            .map(|v| v.value().clone())
            .or_else(|| {
                if state.model_index.contains_key(&fk.table) {
                    Some(fk.table.clone())
                } else {
                    None
                }
            });
        let Some(target_name) = target_name else { continue };
        // Find the target model and check its columns
        let Some(target_loc) = state.model_index.get(&target_name) else { continue };
        let uri = target_loc.uri.clone();
        drop(target_loc);
        let Some(file_models) = state.file_models.get(&uri) else { continue };
        let Some(target_model) = file_models.iter().find(|m| m.name == target_name) else {
            continue;
        };
        if !target_model.columns.contains_key(&fk.column) {
            out.push(d(
                "SQLA-E302",
                DiagnosticSeverity::ERROR,
                format!(
                    "Foreign key column `{}` not found on `{}`",
                    fk.column, target_name
                ),
                fk.range,
            ));
        }
    }
}

fn w303_fk_type_mismatch(model: &Model, state: &WorkspaceState, out: &mut Vec<Diagnostic>) {
    for col in model.columns.values() {
        let Some(ref fk) = col.foreign_key else { continue };
        let target_name = state
            .table_index
            .get(&fk.table)
            .map(|v| v.value().clone())
            .or_else(|| {
                if state.model_index.contains_key(&fk.table) {
                    Some(fk.table.clone())
                } else {
                    None
                }
            });
        let Some(target_name) = target_name else { continue };
        let Some(target_loc) = state.model_index.get(&target_name) else { continue };
        let uri = target_loc.uri.clone();
        drop(target_loc);
        let Some(file_models) = state.file_models.get(&uri) else { continue };
        let Some(target_model) = file_models.iter().find(|m| m.name == target_name) else {
            continue;
        };
        let Some(target_col) = target_model.columns.get(&fk.column) else { continue };
        if let Some(false) = types_match(&col.mapped_type, &target_col.mapped_type) {
            out.push(d(
                "SQLA-W303",
                DiagnosticSeverity::WARNING,
                format!(
                    "FK type mismatch: `{}` is `{}` but `{}.{}` is `{}`",
                    col.name,
                    col.mapped_type,
                    target_name,
                    fk.column,
                    target_col.mapped_type,
                ),
                fk.range,
            ));
        }
    }
}

// ── 4xx — Relationships ───────────────────────────────────────────────────────

fn e401_rel_target_not_found(model: &Model, state: &WorkspaceState, out: &mut Vec<Diagnostic>) {
    for rel in model.relationships.values() {
        if rel.target_model.is_empty() {
            continue;
        }
        if !state.model_index.contains_key(&rel.target_model)
            && !state.table_index.contains_key(&rel.target_model)
        {
            let range = rel.target_range.unwrap_or(rel.full_range);
            out.push(d(
                "SQLA-E401",
                DiagnosticSeverity::ERROR,
                format!("Relationship target `{}` not found", rel.target_model),
                range,
            ));
        }
    }
}

/// W402 and W403 share a single lookup path — both check the counterpart's back_populates.
fn w402_w403_back_populates(model: &Model, state: &WorkspaceState, out: &mut Vec<Diagnostic>) {
    for rel in model.relationships.values() {
        let Some(ref bp) = rel.back_populates else { continue };
        // Skip if target doesn't exist (E401 already fired there).
        let Some(target_loc) = state.model_index.get(&rel.target_model) else { continue };
        let uri = target_loc.uri.clone();
        drop(target_loc);
        let Some(file_models) = state.file_models.get(&uri) else { continue };
        let Some(target_model) = file_models.iter().find(|m| m.name == rel.target_model) else {
            continue;
        };

        let range = rel.back_populates_range.unwrap_or(rel.full_range);

        match target_model.relationships.get(bp) {
            None => {
                out.push(d(
                    "SQLA-W403",
                    DiagnosticSeverity::WARNING,
                    format!("Attribute `{}` not found on `{}`", bp, rel.target_model),
                    range,
                ));
            }
            Some(target_rel) => {
                if target_rel.back_populates.as_deref() != Some(&rel.name) {
                    out.push(d(
                        "SQLA-W402",
                        DiagnosticSeverity::WARNING,
                        format!(
                            "`{}.{}.back_populates` should be \"{}\" but is {}",
                            rel.target_model,
                            bp,
                            rel.name,
                            target_rel
                                .back_populates
                                .as_deref()
                                .map(|s| format!("\"{s}\""))
                                .unwrap_or_else(|| "absent".to_string()),
                        ),
                        range,
                    ));
                }
            }
        }
    }
}

fn w404_uselist_mismatch(model: &Model, out: &mut Vec<Diagnostic>) {
    for rel in model.relationships.values() {
        if let Some(uselist) = rel.uselist {
            if uselist != rel.is_list {
                out.push(d(
                    "SQLA-W404",
                    DiagnosticSeverity::WARNING,
                    format!(
                        "`{}` has uselist={} but annotation is {}",
                        rel.name,
                        uselist,
                        if rel.is_list { "List" } else { "scalar" },
                    ),
                    rel.full_range,
                ));
            }
        }
    }
}

fn w405_target_mismatch(model: &Model, out: &mut Vec<Diagnostic>) {
    for rel in model.relationships.values() {
        let Some(ref explicit) = rel.explicit_target else { continue };
        if *explicit != rel.target_model {
            let range = rel.target_range.unwrap_or(rel.full_range);
            out.push(d(
                "SQLA-W405",
                DiagnosticSeverity::WARNING,
                format!(
                    "Annotation says `{}` but relationship() argument says `{}`",
                    rel.target_model, explicit
                ),
                range,
            ));
        }
    }
}

fn h406_missing_fk_for_relationship(
    model: &Model,
    state: &WorkspaceState,
    out: &mut Vec<Diagnostic>,
) {
    for rel in model.relationships.values() {
        if rel.is_list {
            continue; // Only scalar relationships
        }
        if rel.target_model.is_empty() {
            continue;
        }
        // Skip if target doesn't exist (E401)
        if !state.model_index.contains_key(&rel.target_model) {
            continue;
        }

        // Find the target's table name
        let target_table: Option<String> = state
            .model_index
            .get(&rel.target_model)
            .and_then(|loc| {
                let uri = &loc.uri;
                let models = state.file_models.get(uri)?;
                models
                    .iter()
                    .find(|m| m.name == rel.target_model)
                    .and_then(|m| m.table_name.clone())
            });

        // Check if any FK column links to the target model/table
        let has_fk = model.columns.values().any(|col| {
            let Some(ref fk) = col.foreign_key else { return false };
            fk.table == rel.target_model
                || target_table.as_deref().is_some_and(|t| t == fk.table)
        });
        if has_fk {
            continue;
        }

        // Naming-convention fallback: {rel_name}_id or {target_lower}_id
        let naming_present = model
            .columns
            .contains_key(&format!("{}_id", rel.name))
            || model
                .columns
                .contains_key(&format!("{}_id", rel.target_model.to_lowercase()));
        if naming_present {
            continue;
        }

        // The FK may legitimately live on the other side (e.g. one-to-one "back side").
        // Check whether the target model has a FK pointing back to this model's table.
        if target_has_fk_to(state, &rel.target_model, model) {
            continue;
        }

        out.push(d(
            "SQLA-H406",
            DiagnosticSeverity::HINT,
            format!("No foreign key found for relationship `{}`", rel.name),
            rel.name_range,
        ));
    }
}

/// Return true when `target_model_name` has a column whose FK points at `current`'s table.
fn target_has_fk_to(state: &WorkspaceState, target_model_name: &str, current: &Model) -> bool {
    let Some(loc) = state.model_index.get(target_model_name) else { return false };
    let uri = loc.uri.clone();
    drop(loc);
    let Some(file_models) = state.file_models.get(&uri) else { return false };
    let Some(target) = file_models.iter().find(|m| m.name == target_model_name) else {
        return false;
    };
    let current_table = current.table_name.as_deref().unwrap_or(&current.name);
    target.columns.values().any(|col| {
        col.foreign_key
            .as_ref()
            .is_some_and(|fk| fk.table == current.name || fk.table == current_table)
    })
}

fn h407_unique_missing_one_to_one(
    model: &Model,
    state: &WorkspaceState,
    out: &mut Vec<Diagnostic>,
) {
    for rel in model.relationships.values() {
        // Explicit one-to-one: uselist=false and scalar annotation
        if rel.uselist != Some(false) || rel.is_list {
            continue;
        }

        let target_table: Option<String> = state
            .model_index
            .get(&rel.target_model)
            .and_then(|loc| {
                let uri = &loc.uri;
                let models = state.file_models.get(uri)?;
                models
                    .iter()
                    .find(|m| m.name == rel.target_model)
                    .and_then(|m| m.table_name.clone())
            });

        for col in model.columns.values() {
            let Some(ref fk) = col.foreign_key else { continue };
            let links_to_target = fk.table == rel.target_model
                || target_table.as_deref().is_some_and(|t| t == fk.table);
            if !links_to_target {
                continue;
            }
            if !col.args.unique && !col.args.primary_key {
                out.push(d(
                    "SQLA-H407",
                    DiagnosticSeverity::HINT,
                    format!(
                        "One-to-one relationship `{}` but FK column `{}` is not unique",
                        rel.name, col.name
                    ),
                    col.name_range,
                ));
            }
        }
    }
}

fn w408_unknown_cascade(model: &Model, out: &mut Vec<Diagnostic>) {
    for rel in model.relationships.values() {
        let Some(ref cascade) = rel.cascade else { continue };
        let range = rel.cascade_range.unwrap_or(rel.full_range);
        for token in cascade.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            if !VALID_CASCADE.contains(&token) {
                out.push(d(
                    "SQLA-W408",
                    DiagnosticSeverity::WARNING,
                    format!("Unknown cascade option `{}`", token),
                    range,
                ));
            }
        }
    }
}

fn w409_orphan_without_delete(model: &Model, out: &mut Vec<Diagnostic>) {
    for rel in model.relationships.values() {
        let Some(ref cascade) = rel.cascade else { continue };
        let tokens: Vec<&str> =
            cascade.split(',').map(str::trim).filter(|t| !t.is_empty()).collect();
        if tokens.contains(&"delete-orphan")
            && !tokens.contains(&"delete")
            && !tokens.contains(&"all")
        {
            let range = rel.cascade_range.unwrap_or(rel.full_range);
            out.push(d(
                "SQLA-W409",
                DiagnosticSeverity::WARNING,
                "`delete-orphan` cascade requires `delete` or `all`".to_string(),
                range,
            ));
        }
    }
}

// ── H410 — Circular relationships ─────────────────────────────────────────────

fn h410_circular_relationship(models: &[Model], out: &mut Vec<Diagnostic>) {
    // Build graph: model_name → Vec<(target_model_name, rel_name_range)>
    // Edges with back_populates are intentional bidirectional pairs — skip them.
    let mut adj: HashMap<String, Vec<(String, crate::model::types::Range)>> = HashMap::new();
    let model_names: HashSet<String> = models.iter().map(|m| m.name.clone()).collect();

    for model in models {
        for rel in model.relationships.values() {
            if rel.back_populates.is_some() {
                continue;
            }
            if rel.target_model.is_empty() || !model_names.contains(&rel.target_model) {
                continue; // only within-file edges
            }
            adj.entry(model.name.clone())
                .or_default()
                .push((rel.target_model.clone(), rel.name_range));
        }
    }

    // DFS with white(0)/gray(1)/black(2) coloring
    let mut color: HashMap<String, u8> = model_names.iter().map(|n| (n.clone(), 0)).collect();
    let mut path: Vec<String> = Vec::new();
    let mut cycle_ranges: Vec<crate::model::types::Range> = Vec::new();

    for start in &model_names {
        if color.get(start).copied().unwrap_or(0) == 0 {
            dfs_cycles(start, &adj, &mut color, &mut path, &mut cycle_ranges);
        }
    }

    // Deduplicate ranges (a node may appear in multiple found cycle prefixes)
    let mut seen: HashSet<(u32, u32, u32, u32)> = HashSet::new();
    for range in cycle_ranges {
        let key = (range.start_line, range.start_col, range.end_line, range.end_col);
        if seen.insert(key) {
            out.push(d(
                "SQLA-H410",
                DiagnosticSeverity::HINT,
                "Circular relationship detected".to_string(),
                range,
            ));
        }
    }
}

fn dfs_cycles(
    node: &str,
    adj: &HashMap<String, Vec<(String, crate::model::types::Range)>>,
    color: &mut HashMap<String, u8>,
    path: &mut Vec<String>,
    cycles: &mut Vec<crate::model::types::Range>,
) {
    *color.get_mut(node).unwrap() = 1; // gray
    path.push(node.to_string());

    if let Some(edges) = adj.get(node) {
        for (target, _range) in edges {
            match color.get(target.as_str()).copied().unwrap_or(2) {
                1 => {
                    // Back-edge: found a cycle
                    if let Some(start_idx) = path.iter().position(|n| n == target) {
                        let cycle_len = path.len() - start_idx; // number of models in cycle
                        if cycle_len > 2 {
                            // Collect the range of each edge in the cycle
                            for i in start_idx..path.len() {
                                let from = &path[i];
                                let to = if i + 1 < path.len() {
                                    path[i + 1].as_str()
                                } else {
                                    target.as_str()
                                };
                                if let Some(edges_from) = adj.get(from.as_str()) {
                                    if let Some((_, r)) =
                                        edges_from.iter().find(|(t, _)| t == to)
                                    {
                                        cycles.push(*r);
                                    }
                                }
                            }
                        }
                    }
                }
                0 => {
                    dfs_cycles(target, adj, color, path, cycles);
                }
                _ => {} // black — already fully processed
            }
        }
    }

    path.pop();
    *color.get_mut(node).unwrap() = 2; // black
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tower_lsp_server::ls_types::Uri;

    use super::*;
    use crate::model::types::{
        Column, ColumnArgs, ForeignKeyRef, MappedType, Model, Range, Relationship, TableArg,
    };
    use crate::state::WorkspaceState;

    fn def_range() -> Range {
        Range { start_line: 0, start_col: 0, end_line: 0, end_col: 10 }
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

    fn int_col(name: &str) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: MappedType::Int,
            args: ColumnArgs { primary_key: false, nullable: false, unique: false, index: false, default: None, server_default: None },
            foreign_key: None,
            doc: None,
            name_range: def_range(),
            full_range: def_range(),
        }
    }

    fn fk_col(name: &str, table: &str, col: &str, nullable: bool) -> Column {
        Column {
            name: name.to_string(),
            key: None,
            mapped_type: if nullable { MappedType::Optional(Box::new(MappedType::Int)) } else { MappedType::Int },
            args: ColumnArgs { primary_key: false, nullable, unique: false, index: false, default: None, server_default: None },
            foreign_key: Some(ForeignKeyRef {
                table: table.to_string(),
                column: col.to_string(),
                raw_text: format!("{table}.{col}"),
                range: def_range(),
            }),
            doc: None,
            name_range: def_range(),
            full_range: def_range(),
        }
    }

    fn rel(name: &str, target: &str, is_list: bool, back_pop: Option<&str>) -> Relationship {
        Relationship {
            name: name.to_string(),
            target_model: target.to_string(),
            explicit_target: None,
            back_populates: back_pop.map(|s| s.to_string()),
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

    fn populate_state(state: &WorkspaceState, uri: &Uri, models: Vec<Model>) {
        state.update_file(uri, models);
    }

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }

    // ── W101 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_01_missing_tablename_fires_for_concrete() {
        let state = WorkspaceState::new();
        let model = bare_model("User", None);
        let diags = check_file(&[model], &state);
        assert_eq!(diags.len(), 1);
        assert_eq!(code(&diags[0]), "SQLA-W101");
    }

    #[test]
    fn req_diag_01_missing_tablename_silent_for_abstract_base() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Base", None);
        model.bases = vec!["DeclarativeBase".to_string()];
        let diags = check_file(&[model], &state);
        assert!(diags.is_empty());
    }

    // ── E102 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_02_duplicate_tablename() {
        let state = WorkspaceState::new();
        let u1 = uri("file:///a.py");
        let u2 = uri("file:///b.py");
        let m1 = bare_model("User", Some("users"));
        let m2 = bare_model("Account", Some("users"));
        populate_state(&state, &u1, vec![m1.clone()]);
        populate_state(&state, &u2, vec![m2.clone()]);

        let diags = check_file(&[m2], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-E102"), "{diags:?}");
    }

    // ── E103 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_03_duplicate_column() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.duplicate_columns.push(("title".to_string(), def_range()));
        let diags = check_file(&[model], &state);
        assert_eq!(code(&diags[0]), "SQLA-E103");
    }

    // ── E105 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_04_table_arg_column_not_found() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.table_args.push(TableArg {
            kind: "Index".to_string(),
            columns: vec!["titel".to_string()], // typo
            column_ranges: vec![def_range()],
            full_range: def_range(),
            name: None,
        });
        model.columns.insert("title".to_string(), int_col("title"));
        let diags = check_file(&[model], &state);
        assert_eq!(code(&diags[0]), "SQLA-E105");
    }

    // ── W201 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_05_nullable_not_optional_fires() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        // FK column, nullable=true but type is NOT Optional
        let mut col = fk_col("author_id", "users", "id", false);
        col.args.nullable = true;
        col.mapped_type = MappedType::Int; // not Optional → should fire
        model.columns.insert("author_id".to_string(), col);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W201"), "{diags:?}");
    }

    #[test]
    fn req_diag_05_nullable_not_optional_silent_for_optional_type() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        let col = fk_col("author_id", "users", "id", true); // nullable=true, Optional type
        model.columns.insert("author_id".to_string(), col);
        let diags = check_file(&[model], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W201"), "{diags:?}");
    }

    // ── E301 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_06_unknown_fk_table() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.columns.insert("author_id".to_string(), fk_col("author_id", "user", "id", false));
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-E301"), "{diags:?}");
    }

    #[test]
    fn req_diag_06_fk_no_false_positive_when_target_indexed() {
        let state = WorkspaceState::new();
        let user = bare_model("User", Some("users"));
        populate_state(&state, &uri("file:///user.py"), vec![user]);

        let mut post = bare_model("Post", Some("posts"));
        post.columns.insert("author_id".to_string(), fk_col("author_id", "users", "id", false));
        let diags = check_file(&[post], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-E301"), "{diags:?}");
    }

    // ── E302 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_07_fk_column_not_found() {
        let state = WorkspaceState::new();
        let mut user = bare_model("User", Some("users"));
        user.columns.insert("id".to_string(), int_col("id"));
        populate_state(&state, &uri("file:///user.py"), vec![user]);

        let mut post = bare_model("Post", Some("posts"));
        post.columns.insert(
            "author_id".to_string(),
            fk_col("author_id", "users", "uuid", false), // "uuid" doesn't exist
        );
        let diags = check_file(&[post], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-E302"), "{diags:?}");
    }

    // ── W303 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_08_fk_type_mismatch() {
        let state = WorkspaceState::new();
        let mut user = bare_model("User", Some("users"));
        let mut uid = int_col("id");
        uid.mapped_type = MappedType::Str; // target is Str
        user.columns.insert("id".to_string(), uid);
        populate_state(&state, &uri("file:///user.py"), vec![user]);

        let mut post = bare_model("Post", Some("posts"));
        let mut col = fk_col("author_id", "users", "id", false);
        col.mapped_type = MappedType::Int; // local is Int → mismatch
        post.columns.insert("author_id".to_string(), col);
        let diags = check_file(&[post], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W303"), "{diags:?}");
    }

    #[test]
    fn req_diag_08_fk_type_mismatch_silent_for_unknown() {
        let state = WorkspaceState::new();
        let mut user = bare_model("User", Some("users"));
        let mut uid = int_col("id");
        uid.mapped_type = MappedType::Unknown("X".to_string());
        user.columns.insert("id".to_string(), uid);
        populate_state(&state, &uri("file:///user.py"), vec![user]);

        let mut post = bare_model("Post", Some("posts"));
        let mut col = fk_col("author_id", "users", "id", false);
        col.mapped_type = MappedType::Int;
        post.columns.insert("author_id".to_string(), col);
        let diags = check_file(&[post], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W303"), "{diags:?}");
    }

    // ── E401 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_09_rel_target_not_found() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        model.relationships.insert("author".to_string(), rel("author", "Ghost", false, None));
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-E401"), "{diags:?}");
    }

    // ── W402/W403 ─────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_10_back_populates_mismatch() {
        let state = WorkspaceState::new();
        // Post.author has back_populates="posts"
        // User.posts has back_populates="wrong" instead of "author"
        let mut post = bare_model("Post", Some("posts"));
        post.relationships.insert("author".to_string(), rel("author", "User", false, Some("posts")));

        let mut user = bare_model("User", Some("users"));
        user.relationships.insert("posts".to_string(), rel("posts", "Post", true, Some("wrong")));

        let u_uri = uri("file:///user.py");
        populate_state(&state, &u_uri, vec![user]);

        let diags = check_file(&[post], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W402"), "{diags:?}");
    }

    #[test]
    fn req_diag_11_back_populates_not_found() {
        let state = WorkspaceState::new();
        let mut post = bare_model("Post", Some("posts"));
        post.relationships
            .insert("author".to_string(), rel("author", "User", false, Some("posts")));

        let user = bare_model("User", Some("users")); // User has no .posts relationship
        populate_state(&state, &uri("file:///user.py"), vec![user]);

        let diags = check_file(&[post], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W403"), "{diags:?}");
    }

    // ── W404 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_12_uselist_mismatch() {
        let state = WorkspaceState::new();
        let mut model = bare_model("User", Some("users"));
        let mut r = rel("posts", "Post", true, None); // is_list=true
        r.uselist = Some(false); // but uselist=False → mismatch
        model.relationships.insert("posts".to_string(), r);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W404"), "{diags:?}");
    }

    // ── W405 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_13_target_mismatch() {
        let state = WorkspaceState::new();
        let mut model = bare_model("Post", Some("posts"));
        let mut r = rel("author", "User", false, None);
        r.explicit_target = Some("Account".to_string()); // disagrees with annotation
        model.relationships.insert("author".to_string(), r);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W405"), "{diags:?}");
    }

    // ── W408/W409 ─────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_16_unknown_cascade() {
        let state = WorkspaceState::new();
        let mut model = bare_model("User", Some("users"));
        let mut r = rel("posts", "Post", true, None);
        r.cascade = Some("save-update, explode".to_string()); // "explode" is invalid
        model.relationships.insert("posts".to_string(), r);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W408"), "{diags:?}");
    }

    #[test]
    fn req_diag_17_orphan_without_delete() {
        let state = WorkspaceState::new();
        let mut model = bare_model("User", Some("users"));
        let mut r = rel("posts", "Post", true, None);
        r.cascade = Some("save-update, delete-orphan".to_string()); // missing delete
        model.relationships.insert("posts".to_string(), r);
        let diags = check_file(&[model], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W409"), "{diags:?}");
    }

    // ── H410 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_diag_18_circular_relationship_three_models() {
        let state = WorkspaceState::new();
        let mut a = bare_model("A", Some("a"));
        a.relationships.insert("b".to_string(), rel("b", "B", false, None));
        let mut b = bare_model("B", Some("b_t"));
        b.relationships.insert("c".to_string(), rel("c", "C", false, None));
        let mut c = bare_model("C", Some("c_t"));
        c.relationships.insert("a".to_string(), rel("a", "A", false, None));

        let diags = check_file(&[a, b, c], &state);
        assert!(diags.iter().any(|d| code(d) == "SQLA-H410"), "{diags:?}");
    }

    #[test]
    fn req_diag_18_two_model_loop_silent() {
        let state = WorkspaceState::new();
        // A→B and B→A without back_populates: length-2 cycle — should NOT fire
        let mut a = bare_model("A", Some("a"));
        a.relationships.insert("b".to_string(), rel("b", "B", false, None));
        let mut b = bare_model("B", Some("b_t"));
        b.relationships.insert("a".to_string(), rel("a", "A", false, None));

        let diags = check_file(&[a, b], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-H410"), "{diags:?}");
    }

    #[test]
    fn req_diag_18_back_populates_pair_silent() {
        let state = WorkspaceState::new();
        let mut user = bare_model("User", Some("users"));
        user.relationships
            .insert("posts".to_string(), rel("posts", "Post", true, Some("author")));
        let mut post = bare_model("Post", Some("posts"));
        post.relationships
            .insert("author".to_string(), rel("author", "User", false, Some("posts")));

        let diags = check_file(&[user, post], &state);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-H410"), "{diags:?}");
    }

    // ── clean-blog baseline ───────────────────────────────────────────────────

    #[test]
    fn req_diag_clean_blog_zero_findings() {
        use crate::parsing::extractor::extract_models;
        use std::fs;

        let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/e2e/fixtures/clean_blog");

        let state = WorkspaceState::new();

        // Index every model file into the state first
        let model_files = ["models/base.py", "models/user.py", "models/post.py",
                           "models/tag.py", "models/comment.py"];
        let mut all_uris = vec![];
        for rel_path in &model_files {
            let path = fixtures.join(rel_path);
            let src = fs::read_to_string(&path).unwrap();
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();
            let tree = parser.parse(&src, None).unwrap();
            let models = extract_models(&src, &tree);
            let uri: Uri = format!("file://{}", path.display()).parse().unwrap();
            state.update_file(&uri, models);
            all_uris.push(uri);
        }

        // Now check each file — all should produce zero findings
        for uri in &all_uris {
            if let Some(models) = state.file_models.get(uri) {
                let diags = check_file(&models, &state);
                assert!(
                    diags.is_empty(),
                    "clean_blog/{:?}: unexpected diagnostics: {diags:?}",
                    uri
                );
            }
        }
    }

    fn code(d: &Diagnostic) -> &str {
        match &d.code {
            Some(NumberOrString::String(s)) => s.as_str(),
            _ => "",
        }
    }
}
