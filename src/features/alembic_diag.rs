/// F13 — Alembic diagnostics (SQLA-7xx).
///
/// Four rules that read the migration index:
///   W701 broken-migration-chain  — dangling down_revision pointer
///   W702 multiple-heads          — two or more unmerged heads
///   H703 unknown-migration-table — op.* names a table no model owns
///   W704 null-constraint-name    — constraint op passes None or omits the name
///
/// `check_migration` is called from Pass 2 for every known migration URI.
use std::collections::HashSet;

use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::{
    alembic::{DownRevision, MigrationFile},
    model::types,
    state::WorkspaceState,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_lsp(r: types::Range) -> Range {
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

fn diag(code: &str, severity: DiagnosticSeverity, msg: String, r: types::Range) -> Diagnostic {
    Diagnostic {
        range: to_lsp(r),
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        source: Some("sqlalchemy-lsp".to_string()),
        message: msg,
        ..Default::default()
    }
}

/// Sentinel range used when a more precise range is unavailable.
fn zero_range() -> types::Range {
    types::Range {
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 0,
    }
}

// ── Head-set computation ──────────────────────────────────────────────────────

/// Compute the set of all revision ids that are heads (not pointed to by any other file).
///
/// A head is a revision that no other file lists as a `down_revision` parent.
pub fn compute_head_set(state: &WorkspaceState) -> HashSet<String> {
    // Collect every revision id that is referenced as a parent.
    let mut all_parents: HashSet<String> = HashSet::new();
    for entry in state.migration_files.iter() {
        match &entry.down_revision {
            DownRevision::None => {}
            DownRevision::Single(rev) => {
                all_parents.insert(rev.clone());
            }
            DownRevision::Multiple(revs) => {
                for rev in revs {
                    all_parents.insert(rev.clone());
                }
            }
        }
    }
    // A head is any revision that is not in all_parents.
    state
        .revision_index
        .iter()
        .filter(|e| !all_parents.contains(e.key()))
        .map(|e| e.key().clone())
        .collect()
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run all F13 lints on a single migration file.
///
/// `heads` is pre-computed by `compute_head_set` and passed in to avoid repeating
/// the cross-file computation for every URI.
pub fn check_migration(
    mf: &MigrationFile,
    state: &WorkspaceState,
    heads: &HashSet<String>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    w701_broken_chain(mf, state, &mut out);
    w702_multiple_heads(mf, heads, &mut out);
    h703_unknown_table(mf, state, &mut out);
    w704_null_constraint_name(mf, &mut out);

    out
}

// ── W701 — broken-migration-chain ────────────────────────────────────────────

fn w701_broken_chain(mf: &MigrationFile, state: &WorkspaceState, out: &mut Vec<Diagnostic>) {
    let range = mf.down_revision_range.unwrap_or_else(zero_range);

    let mut check = |rev: &str| {
        if !state.revision_index.contains_key(rev) {
            out.push(diag(
                "SQLA-W701",
                DiagnosticSeverity::WARNING,
                format!("down_revision `{rev}` not found in migration chain"),
                range,
            ));
        }
    };

    match &mf.down_revision {
        DownRevision::None => {}
        DownRevision::Single(rev) => check(rev),
        DownRevision::Multiple(revs) => {
            for rev in revs {
                check(rev);
            }
        }
    }
}

// ── W702 — multiple-heads ─────────────────────────────────────────────────────

fn w702_multiple_heads(mf: &MigrationFile, heads: &HashSet<String>, out: &mut Vec<Diagnostic>) {
    if heads.len() < 2 {
        return;
    }
    let Some(ref revision) = mf.revision else {
        return;
    };
    if !heads.contains(revision) {
        return; // this file is not a head
    }
    let range = mf.revision_range.unwrap_or_else(zero_range);
    let mut sorted: Vec<&str> = heads.iter().map(|s| s.as_str()).collect();
    sorted.sort(); // deterministic message
    out.push(diag(
        "SQLA-W702",
        DiagnosticSeverity::WARNING,
        format!("Multiple head revisions detected: {}", sorted.join(", ")),
        range,
    ));
}

// ── H703 — unknown-migration-table ───────────────────────────────────────────

fn h703_unknown_table(mf: &MigrationFile, state: &WorkspaceState, out: &mut Vec<Diagnostic>) {
    // Tables created in this very migration — they are new and won't be indexed yet.
    let locally_created: std::collections::HashSet<&str> = mf
        .op_calls
        .iter()
        .filter(|o| o.operation == "create_table")
        .filter_map(|o| o.table_name.as_ref())
        .map(|t| t.name.as_str())
        .collect();

    for op in &mf.op_calls {
        if op.operation == "create_table" {
            continue; // creating a new table — absence is expected
        }
        let Some(ref tref) = op.table_name else {
            continue;
        };
        if locally_created.contains(tref.name.as_str()) {
            continue; // table created in this same migration
        }
        if !state.table_index.contains_key(&tref.name) {
            out.push(diag(
                "SQLA-H703",
                DiagnosticSeverity::HINT,
                format!("Table `{}` not found in indexed models", tref.name),
                tref.range,
            ));
        }
    }
}

// ── W704 — null-constraint-name ───────────────────────────────────────────────

fn w704_null_constraint_name(mf: &MigrationFile, out: &mut Vec<Diagnostic>) {
    for op in &mf.op_calls {
        let Some(r) = op.null_constraint_name_range else {
            continue;
        };
        out.push(diag(
            "SQLA-W704",
            DiagnosticSeverity::WARNING,
            format!(
                "op.{} uses None as the constraint name; name the constraint explicitly \
                 (or rely on a configured naming_convention) so the migration is \
                 reproducible and targetable",
                op.operation
            ),
            r,
        ));
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use tower_lsp_server::ls_types::{NumberOrString, Uri};

    use super::*;
    use crate::alembic::{DownRevision, MigrationFile, OpCall, TableRef};
    use crate::model::types::{Model, Range as MRange};
    use crate::state::WorkspaceState;
    use std::collections::HashMap;

    fn def_range() -> MRange {
        MRange {
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 10,
        }
    }

    fn dr(line: u32, start: u32, end: u32) -> MRange {
        MRange {
            start_line: line,
            start_col: start,
            end_line: line,
            end_col: end,
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

    fn mf_base(revision: &str, down: DownRevision) -> MigrationFile {
        MigrationFile {
            revision: Some(revision.to_string()),
            down_revision: down,
            message: None,
            revision_range: Some(def_range()),
            down_revision_range: Some(def_range()),
            op_calls: vec![],
        }
    }

    fn bare_model(name: &str, table: &str) -> Model {
        Model {
            name: name.to_string(),
            table_name: Some(table.to_string()),
            bases: vec![],
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: def_range(),
            full_range: def_range(),
        }
    }

    fn op_call(operation: &str, table: Option<&str>) -> OpCall {
        OpCall {
            operation: operation.to_string(),
            full_range: def_range(),
            table_name: table.map(|t| TableRef {
                name: t.to_string(),
                range: def_range(),
            }),
            column_name: None,
            null_constraint_name_range: None,
        }
    }

    fn constraint_op_null(operation: &str, table: Option<&str>) -> OpCall {
        OpCall {
            operation: operation.to_string(),
            full_range: def_range(),
            table_name: table.map(|t| TableRef {
                name: t.to_string(),
                range: def_range(),
            }),
            column_name: None,
            null_constraint_name_range: Some(def_range()),
        }
    }

    // ── W701 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_alm_03_broken_chain_fires_w701() {
        let state = WorkspaceState::new();
        // revision_index has "a1" but not "b2"
        state
            .revision_index
            .insert("a1".to_string(), uri("file:///a.py"));
        let mf = MigrationFile {
            down_revision: DownRevision::Single("b2".to_string()),
            down_revision_range: Some(dr(1, 16, 18)),
            ..mf_base("c3", DownRevision::None)
        };
        let heads = HashSet::new();
        let diags = check_migration(&mf, &state, &heads);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W701"), "{diags:?}");
    }

    #[test]
    fn req_alm_03_base_migration_silent() {
        let state = WorkspaceState::new();
        let mf = mf_base("a1", DownRevision::None);
        let heads = HashSet::new();
        let diags = check_migration(&mf, &state, &heads);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W701"), "{diags:?}");
    }

    #[test]
    fn req_alm_03_valid_single_parent_silent() {
        let state = WorkspaceState::new();
        state
            .revision_index
            .insert("a1".to_string(), uri("file:///a.py"));
        let mf = mf_base("b2", DownRevision::Single("a1".to_string()));
        let heads = HashSet::new();
        let diags = check_migration(&mf, &state, &heads);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W701"), "{diags:?}");
    }

    #[test]
    fn req_alm_03_merge_one_missing_fires_once() {
        let state = WorkspaceState::new();
        state
            .revision_index
            .insert("a1".to_string(), uri("file:///a.py"));
        let mf = mf_base(
            "c3",
            DownRevision::Multiple(vec!["a1".to_string(), "b2".to_string()]),
        );
        let heads = HashSet::new();
        let diags = check_migration(&mf, &state, &heads);
        let w701_count = diags.iter().filter(|d| code(d) == "SQLA-W701").count();
        assert_eq!(w701_count, 1, "one missing parent → one finding");
    }

    // ── W702 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_alm_04_multiple_heads_fire_w702() {
        let state = WorkspaceState::new();
        state
            .revision_index
            .insert("h1".to_string(), uri("file:///h1.py"));
        state
            .revision_index
            .insert("h2".to_string(), uri("file:///h2.py"));
        // Neither h1 nor h2 is in any down_revision → both are heads
        let mf = mf_base("h1", DownRevision::None);
        let heads: HashSet<String> = ["h1", "h2"].iter().map(|s| s.to_string()).collect();
        let diags = check_migration(&mf, &state, &heads);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W702"), "{diags:?}");
    }

    #[test]
    fn req_alm_04_single_head_silent() {
        let state = WorkspaceState::new();
        let mf = mf_base("h1", DownRevision::None);
        let heads: HashSet<String> = ["h1"].iter().map(|s| s.to_string()).collect();
        let diags = check_migration(&mf, &state, &heads);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W702"), "{diags:?}");
    }

    #[test]
    fn req_alm_04_w702_silent_when_not_a_head() {
        // File whose revision is not in the head set should not fire
        let state = WorkspaceState::new();
        let mf = mf_base("old", DownRevision::Single("base".to_string()));
        let heads: HashSet<String> = ["h1", "h2"].iter().map(|s| s.to_string()).collect();
        let diags = check_migration(&mf, &state, &heads);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W702"), "{diags:?}");
    }

    // ── H703 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_alm_05_unknown_table_fires_h703() {
        let state = WorkspaceState::new();
        let u = uri("file:///models.py");
        state.update_file(&u, vec![bare_model("Post", "posts")]);
        let mut mf = mf_base("a1", DownRevision::None);
        mf.op_calls = vec![op_call("add_column", Some("typo_posts"))];
        let heads = HashSet::new();
        let diags = check_migration(&mf, &state, &heads);
        assert!(diags.iter().any(|d| code(d) == "SQLA-H703"), "{diags:?}");
    }

    #[test]
    fn req_alm_05_known_table_silent() {
        let state = WorkspaceState::new();
        let u = uri("file:///models.py");
        state.update_file(&u, vec![bare_model("Post", "posts")]);
        let mut mf = mf_base("a1", DownRevision::None);
        mf.op_calls = vec![op_call("add_column", Some("posts"))];
        let heads = HashSet::new();
        let diags = check_migration(&mf, &state, &heads);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-H703"), "{diags:?}");
    }

    #[test]
    fn req_alm_05_create_table_skipped() {
        let state = WorkspaceState::new();
        // "new_table" doesn't exist in the index but create_table is exempt
        let mut mf = mf_base("a1", DownRevision::None);
        mf.op_calls = vec![op_call("create_table", Some("new_table"))];
        let heads = HashSet::new();
        let diags = check_migration(&mf, &state, &heads);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-H703"), "{diags:?}");
    }

    // ── W704 ──────────────────────────────────────────────────────────────────

    #[test]
    fn req_alm_11_null_constraint_name_fires_w704() {
        let state = WorkspaceState::new();
        let mut mf = mf_base("a1", DownRevision::None);
        mf.op_calls = vec![constraint_op_null("drop_constraint", Some("posts"))];
        let heads = HashSet::new();
        let diags = check_migration(&mf, &state, &heads);
        assert!(diags.iter().any(|d| code(d) == "SQLA-W704"), "{diags:?}");
    }

    #[test]
    fn req_alm_11_named_constraint_is_silent() {
        let state = WorkspaceState::new();
        let mut mf = mf_base("a1", DownRevision::None);
        // null_constraint_name_range is None → named → silent
        mf.op_calls = vec![op_call("drop_constraint", Some("posts"))];
        let heads = HashSet::new();
        let diags = check_migration(&mf, &state, &heads);
        assert!(!diags.iter().any(|d| code(d) == "SQLA-W704"), "{diags:?}");
    }

    // ── REQ-ALM-01/02: extraction coverage (using extractor module) ───────────

    #[test]
    fn req_alm_01_migration_extracts_and_indexes_revision() {
        use crate::alembic::extractor::extract_migration;
        let src = r#"
revision = "abc123"
down_revision = None
def upgrade(): pass
def downgrade(): pass
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        let mf = extract_migration(src, &tree).unwrap();
        assert_eq!(mf.revision.as_deref(), Some("abc123"));

        let state = WorkspaceState::new();
        let u = uri("file:///v1.py");
        state.update_migration(&u, mf);
        assert!(state.revision_index.contains_key("abc123"));
    }

    #[test]
    fn req_alm_02_opcall_carries_table_and_column() {
        use crate::alembic::extractor::extract_migration;
        let src = r#"
revision = "abc123"
down_revision = None
def upgrade():
    op.add_column("posts", "title")
def downgrade(): pass
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        let mf = extract_migration(src, &tree).unwrap();
        let op = mf
            .op_calls
            .iter()
            .find(|o| o.operation == "add_column")
            .unwrap();
        assert_eq!(
            op.table_name.as_ref().map(|t| t.name.as_str()),
            Some("posts")
        );
        assert_eq!(
            op.column_name.as_ref().map(|c| c.name.as_str()),
            Some("title")
        );
    }

    // ── W704 via extractor ────────────────────────────────────────────────────

    #[test]
    fn req_alm_11_extractor_detects_none_constraint_name() {
        use crate::alembic::extractor::extract_migration;
        let src = r#"
revision = "abc123"
down_revision = None
def upgrade():
    op.drop_constraint(None, "posts", type_="unique")
def downgrade(): pass
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        let mf = extract_migration(src, &tree).unwrap();
        let op = mf
            .op_calls
            .iter()
            .find(|o| o.operation == "drop_constraint")
            .unwrap();
        assert!(
            op.null_constraint_name_range.is_some(),
            "None constraint should be detected"
        );
    }

    #[test]
    fn req_alm_11_extractor_named_constraint_not_flagged() {
        use crate::alembic::extractor::extract_migration;
        let src = r#"
revision = "abc123"
down_revision = None
def upgrade():
    op.drop_constraint("uq_posts_slug", "posts", type_="unique")
def downgrade(): pass
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        let mf = extract_migration(src, &tree).unwrap();
        let op = mf
            .op_calls
            .iter()
            .find(|o| o.operation == "drop_constraint")
            .unwrap();
        assert!(
            op.null_constraint_name_range.is_none(),
            "named constraint should not be flagged"
        );
    }

    // ── clean-blog baseline ───────────────────────────────────────────────────

    #[test]
    fn req_alm_clean_blog_zero_alembic_findings() {
        use crate::alembic::extractor::extract_migration;
        use crate::parsing::extractor::extract_models;
        use std::fs;

        let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/e2e/fixtures/clean_blog");

        let state = WorkspaceState::new();

        // Load models first so table_index is populated
        for rel_path in &[
            "models/base.py",
            "models/user.py",
            "models/post.py",
            "models/tag.py",
            "models/comment.py",
        ] {
            let path = fixtures.join(rel_path);
            let src = fs::read_to_string(&path).unwrap();
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter_python::LANGUAGE.into())
                .unwrap();
            let tree = parser.parse(&src, None).unwrap();
            let models = extract_models(&src, &tree);
            let u: tower_lsp_server::ls_types::Uri =
                format!("file://{}", path.display()).parse().unwrap();
            state.update_file(&u, models);
        }

        // Load migration files
        let migrations_dir = fixtures.join("migrations/versions");
        let mut mig_uris = vec![];
        for entry in fs::read_dir(&migrations_dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "py") {
                let src = fs::read_to_string(&path).unwrap();
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_python::LANGUAGE.into())
                    .unwrap();
                let tree = parser.parse(&src, None).unwrap();
                if let Some(mf) = extract_migration(&src, &tree) {
                    let u: tower_lsp_server::ls_types::Uri =
                        format!("file://{}", path.display()).parse().unwrap();
                    state.update_migration(&u, mf);
                    mig_uris.push(u);
                }
            }
        }

        let heads = compute_head_set(&state);
        for uri in &mig_uris {
            let mf = state.migration_files.get(uri).unwrap();
            let diags = check_migration(&mf, &state, &heads);
            assert!(
                diags.is_empty(),
                "clean_blog migration {:?}: unexpected findings: {diags:?}",
                uri
            );
        }
    }
}
