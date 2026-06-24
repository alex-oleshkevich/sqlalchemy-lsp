use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::task;
use tower_lsp_server::{Client, ls_types::Uri};

use crate::{
    alembic::{MigrationFile, extractor::extract_migration},
    config::{apply_noqa_to_diagnostics, filter_diagnostics},
    features::{alembic_diag, f01, f02},
    model::types::{ColumnArgs, Model},
    parsing::{
        extractor::{extract_annotated_aliases, extract_models},
        python::{has_alembic_indicators, has_sqlalchemy_indicators},
    },
    state::WorkspaceState,
};

type AliasMap = std::collections::HashMap<String, ColumnArgs>;
type Pass1Result = Option<(
    tree_sitter::Tree,
    Vec<Model>,
    Option<MigrationFile>,
    AliasMap,
)>;

// ── CLI headless scan ─────────────────────────────────────────────────────────

/// Synchronously scan all Python files under `root`, extract models and
/// migration metadata, and compute diagnostics — without any LSP client.
/// Returns a fully indexed `WorkspaceState` ready for CLI reporting.
pub fn build_workspace_index(root: &Path) -> Arc<WorkspaceState> {
    let state = Arc::new(WorkspaceState::new());
    let py_files = collect_py_files(root);

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("load python grammar");

    // Pass A: collect annotated type aliases from all SA files into the global index.
    // This must run before Pass B so imported aliases (e.g. `from common import UUIDPk`)
    // are available regardless of filesystem ordering.
    for path in &py_files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        if !has_sqlalchemy_indicators(&source) {
            continue;
        }
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        for (name, args) in extract_annotated_aliases(&source, &tree) {
            state.annotated_alias_index.insert(name, args);
        }
    }

    // Pass B: extract models (with global aliases resolved) and migration metadata.
    for path in &py_files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        if !has_sqlalchemy_indicators(&source) && !has_alembic_indicators(&source) {
            continue;
        }
        let Some(uri) = Uri::from_file_path(path) else {
            continue;
        };
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };

        let models = if has_sqlalchemy_indicators(&source) {
            extract_models(&source, &tree, &state.annotated_alias_index)
        } else {
            vec![]
        };
        let migration = if has_alembic_indicators(&source) {
            extract_migration(&source, &tree)
        } else {
            None
        };

        state.file_sources.insert(uri.clone(), source);
        state.parse_trees.insert(uri.clone(), tree);
        state.update_file(&uri, models);
        if let Some(mf) = migration {
            state.update_migration(&uri, mf);
        }
    }

    // Cross-file diagnostics (no client.publish_diagnostics needed for CLI)
    let model_uris: Vec<Uri> = state.file_models.iter().map(|e| e.key().clone()).collect();
    for uri in &model_uris {
        if let Some(models) = state.file_models.get(uri) {
            let mut d = f01::check_file(&models, &state);
            d.extend(f02::check_file(&models, &state));
            state.diagnostics.insert(uri.clone(), d);
        }
    }

    let heads = alembic_diag::compute_head_set(&state);
    let migration_uris: Vec<Uri> = state
        .migration_files
        .iter()
        .map(|e| e.key().clone())
        .collect();
    for uri in &migration_uris {
        if let Some(mf) = state.migration_files.get(uri) {
            let alembic_diags = alembic_diag::check_migration(&mf, &state, &heads);
            let mut diags = state
                .diagnostics
                .get(uri)
                .map(|d| d.clone())
                .unwrap_or_default();
            diags.extend(alembic_diags);
            state.diagnostics.insert(uri.clone(), diags);
        }
    }

    state
}

/// Recursively collect `.py` files under `root`, skipping hidden and cache dirs.
pub fn collect_py_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skip = path
                .file_name()
                .map(|n| {
                    let s = n.to_string_lossy();
                    s.starts_with('.') || s == "__pycache__" || s == "node_modules"
                })
                .unwrap_or(false);
            if !skip {
                out.extend(collect_py_files(&path));
            }
        } else if path.extension().is_some_and(|e| e == "py") {
            out.push(path);
        }
    }
    out
}

// ── Pass 1: per-file parse and extraction ─────────────────────────────────────

/// Parse `source` for `uri`, extract SA models and Alembic migration metadata,
/// update the workspace index atomically, and push diagnostics.
///
/// The source must already be stored in `state.file_sources` by the caller
/// before invoking this function (so concurrent reads see the latest text).
/// CPU-bound work runs in a blocking thread via `spawn_blocking`.
pub async fn run_pass1(uri: Uri, source: String, state: &Arc<WorkspaceState>, client: &Client) {
    let src = source;
    let alias_snapshot: AliasMap = state
        .annotated_alias_index
        .iter()
        .map(|e| (e.key().clone(), e.value().clone()))
        .collect();

    let result = task::spawn_blocking(move || -> Pass1Result {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .ok()?;
        let tree = parser.parse(src.as_str(), None)?;

        let local_aliases: AliasMap = if has_sqlalchemy_indicators(&src) {
            extract_annotated_aliases(&src, &tree)
        } else {
            AliasMap::new()
        };

        // Merge local aliases into the snapshot so this file's own definitions
        // are available during extraction (relevant when both defined and used in same file).
        let mut merged = alias_snapshot;
        for (k, v) in &local_aliases {
            merged.entry(k.clone()).or_insert_with(|| v.clone());
        }

        let global_map: dashmap::DashMap<String, ColumnArgs> = merged.into_iter().collect();

        let models = if has_sqlalchemy_indicators(&src) {
            extract_models(&src, &tree, &global_map)
        } else {
            vec![]
        };

        let migration = if has_alembic_indicators(&src) {
            extract_migration(&src, &tree)
        } else {
            None
        };

        Some((tree, models, migration, local_aliases))
    })
    .await;

    match result {
        Ok(Some((tree, models, migration, local_aliases))) => {
            tracing::debug!(
                uri = uri.as_str(),
                models = models.len(),
                migration = migration.is_some(),
                "pass1 complete"
            );
            // Update global alias index with any new aliases defined in this file.
            for (name, args) in local_aliases {
                state.annotated_alias_index.insert(name, args);
            }
            state.parse_trees.insert(uri.clone(), tree);
            state.update_file(&uri, models);
            if let Some(mf) = migration {
                state.update_migration(&uri, mf);
            }
            // Publish the last Pass 2 results for this URI; Pass 2 will recompute shortly.
            let diags = state
                .diagnostics
                .get(&uri)
                .map(|d| d.clone())
                .unwrap_or_default();
            client.publish_diagnostics(uri, diags, None).await;
        }
        Ok(None) | Err(_) => {
            tracing::debug!(uri = uri.as_str(), "pass1 parse error");
            // Parse failed — push empty diagnostics so stale squiggles disappear.
            client.publish_diagnostics(uri, vec![], None).await;
        }
    }
}

// ── Pass 2: debounced cross-file relink and publish ───────────────────────────

/// Rebuild cross-file references, publish diagnostics for every indexed file,
/// and fire `inlayHint/refresh` when the client supports it.
///
/// Called by the debounce task spawned in `schedule_relink`.  The caller has
/// already verified that the workspace generation matches before calling here.
pub async fn run_pass2(
    state: &Arc<WorkspaceState>,
    client: &Client,
    supports_inlay_hint_refresh: bool,
) {
    // Snapshot the diagnostic filter config once so the whole pass is consistent.
    let diag_config = {
        let guard = state.config.read().await;
        guard.diagnostics.clone()
    };

    // Run F01/F02 diagnostics for every indexed SA model file, filter, store, then publish.
    let model_uris: Vec<Uri> = state.file_models.iter().map(|e| e.key().clone()).collect();
    tracing::info!(
        sa_files = model_uris.len(),
        models = state.model_index.len(),
        "pass2 relink"
    );
    for uri in &model_uris {
        let raw = if let Some(models) = state.file_models.get(uri) {
            let mut d = f01::check_file(&models, state);
            d.extend(f02::check_file(&models, state));
            d
        } else {
            vec![]
        };
        let diags = filter_diagnostics(raw, &diag_config);
        let diags = if let Some(source) = state.file_sources.get(uri) {
            apply_noqa_to_diagnostics(diags, &source)
        } else {
            diags
        };
        state.diagnostics.insert(uri.clone(), diags.clone());
        client.publish_diagnostics(uri.clone(), diags, None).await;
    }

    // Run Alembic diagnostics for every migration file, filter, merge with SA diags, publish.
    let heads = alembic_diag::compute_head_set(state);
    let migration_uris: Vec<Uri> = state
        .migration_files
        .iter()
        .map(|e| e.key().clone())
        .collect();
    for uri in &migration_uris {
        let alembic_raw = if let Some(mf) = state.migration_files.get(uri) {
            alembic_diag::check_migration(&mf, state, &heads)
        } else {
            vec![]
        };
        let alembic_diags = filter_diagnostics(alembic_raw, &diag_config);
        // Merge with any SA model diagnostics already stored for this URI.
        let mut diags = state
            .diagnostics
            .get(uri)
            .map(|d| d.clone())
            .unwrap_or_default();
        diags.extend(alembic_diags);
        let diags = if let Some(source) = state.file_sources.get(uri) {
            apply_noqa_to_diagnostics(diags, &source)
        } else {
            diags
        };
        state.diagnostics.insert(uri.clone(), diags.clone());
        // Only publish if not already published by the SA model loop above.
        if !state.file_models.contains_key(uri) {
            client.publish_diagnostics(uri.clone(), diags, None).await;
        }
    }

    if supports_inlay_hint_refresh {
        let _ = client.inlay_hint_refresh().await;
    }
}
