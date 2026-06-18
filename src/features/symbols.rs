use tower_lsp_server::ls_types::{
    Location, Position, Range, SymbolInformation, SymbolKind, WorkspaceSymbolResponse,
};

use crate::{alembic::DownRevision, state::WorkspaceState};

fn lsp_range(r: crate::model::types::Range) -> Range {
    Range {
        start: Position { line: r.start_line, character: r.start_col },
        end: Position { line: r.end_line, character: r.end_col },
    }
}

/// Build a symbol name from revision id + optional message.
fn symbol_name(revision: &str, message: Option<&str>) -> String {
    match message {
        Some(msg) if !msg.is_empty() => format!("{revision} · {msg}"),
        _ => revision.to_string(),
    }
}

pub fn provide_symbols(query: &str, state: &WorkspaceState) -> WorkspaceSymbolResponse {
    let query_lower = query.to_lowercase();
    let mut symbols: Vec<SymbolInformation> = Vec::new();

    for entry in state.migration_files.iter() {
        let uri = entry.key().clone();
        let mf = entry.value();

        let revision = match &mf.revision {
            Some(r) => r.clone(),
            None => continue,
        };

        let message = mf.message.as_deref();

        // REQ-SYM-02: match by revision id or message, case-insensitively
        if !query_lower.is_empty() {
            let id_match = revision.to_lowercase().contains(&query_lower);
            let msg_match = message.is_some_and(|m| m.to_lowercase().contains(&query_lower));
            if !id_match && !msg_match {
                continue;
            }
        }

        let name = symbol_name(&revision, message);
        let location = match mf.revision_range {
            Some(r) => Location { uri, range: lsp_range(r) },
            None => Location {
                uri,
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 0 },
                },
            },
        };

        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name,
            kind: SymbolKind::EVENT,
            tags: None,
            deprecated: None,
            location,
            container_name: match &mf.down_revision {
                DownRevision::Single(s) => Some(s.clone()),
                _ => None,
            },
        });
    }

    WorkspaceSymbolResponse::Flat(symbols)
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alembic::{DownRevision, MigrationFile};
    use crate::model::types::Range;
    use crate::state::WorkspaceState;
    use tower_lsp_server::ls_types::Uri;

    fn uri(s: &str) -> Uri { s.parse().unwrap() }
    fn rng(sl: u32, sc: u32, el: u32, ec: u32) -> Range { Range { start_line: sl, start_col: sc, end_line: el, end_col: ec } }

    fn migration(rev: &str, msg: Option<&str>, rev_rng: Range) -> MigrationFile {
        MigrationFile {
            revision: Some(rev.to_string()),
            down_revision: DownRevision::None,
            message: msg.map(|m| m.to_string()),
            revision_range: Some(rev_rng),
            down_revision_range: None,
            op_calls: vec![],
        }
    }

    fn seed(state: &WorkspaceState) {
        state.update_migration(&uri("file:///v1.py"), migration("a1b2c3d4", Some("add user table"), rng(0, 11, 0, 21)));
        state.update_migration(&uri("file:///v2.py"), migration("f9e8d7c6", Some("add audit table"), rng(0, 11, 0, 21)));
        state.update_migration(&uri("file:///v3.py"), migration("00112233", Some("init schema"), rng(0, 11, 0, 21)));
    }

    // ── REQ-SYM-01: each migration emitted as a symbol ───────────────────────

    #[test]
    fn req_sym_01_revisions_emitted_as_symbols() {
        let state = WorkspaceState::new();
        seed(&state);

        let WorkspaceSymbolResponse::Flat(syms) = provide_symbols("", &state) else { panic!() };
        assert_eq!(syms.len(), 3);
        assert!(syms.iter().any(|s| s.name.contains("a1b2c3d4") && s.name.contains("add user table")));
    }

    // ── REQ-SYM-02: match by revision id ─────────────────────────────────────

    #[test]
    fn req_sym_02_match_by_revision_id() {
        let state = WorkspaceState::new();
        seed(&state);

        let WorkspaceSymbolResponse::Flat(syms) = provide_symbols("a1b2", &state) else { panic!() };
        assert_eq!(syms.len(), 1);
        assert!(syms[0].name.contains("a1b2c3d4"));
    }

    // ── REQ-SYM-02: match by message, case-insensitive ────────────────────────

    #[test]
    fn req_sym_02_match_by_message_case_insensitive() {
        let state = WorkspaceState::new();
        seed(&state);

        let WorkspaceSymbolResponse::Flat(syms) = provide_symbols("AUDIT", &state) else { panic!() };
        assert_eq!(syms.len(), 1);
        assert!(syms[0].name.contains("add audit table"));
    }

    // ── REQ-SYM-03: empty query returns all revisions ─────────────────────────

    #[test]
    fn req_sym_03_empty_query_returns_all() {
        let state = WorkspaceState::new();
        seed(&state);

        let WorkspaceSymbolResponse::Flat(syms) = provide_symbols("", &state) else { panic!() };
        assert_eq!(syms.len(), 3);
    }

    // ── REQ-SYM-03: no match → empty list ────────────────────────────────────

    #[test]
    fn req_sym_03_no_match_returns_empty() {
        let state = WorkspaceState::new();
        seed(&state);

        let WorkspaceSymbolResponse::Flat(syms) = provide_symbols("ghost_query_xyz", &state) else { panic!() };
        assert!(syms.is_empty());
    }

    // ── REQ-SYM-04: no migrations → no symbols ───────────────────────────────

    #[test]
    fn req_sym_04_no_migrations_no_symbols() {
        let state = WorkspaceState::new();
        // No migrations registered
        let WorkspaceSymbolResponse::Flat(syms) = provide_symbols("", &state) else { panic!() };
        assert!(syms.is_empty());
    }
}
