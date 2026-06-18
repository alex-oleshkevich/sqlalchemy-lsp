use std::sync::Arc;

use tokio::task;
use tower_lsp_server::{Client, ls_types::Uri};

use crate::{
    alembic::{extractor::extract_migration, MigrationFile},
    features::{alembic_diag, f01, f02},
    model::types::Model,
    parsing::{
        extractor::extract_models,
        python::{has_alembic_indicators, has_sqlalchemy_indicators},
    },
    state::WorkspaceState,
};

// ── Pass 1: per-file parse and extraction ─────────────────────────────────────

/// Parse `source` for `uri`, extract SA models and Alembic migration metadata,
/// update the workspace index atomically, and push diagnostics.
///
/// The source must already be stored in `state.file_sources` by the caller
/// before invoking this function (so concurrent reads see the latest text).
/// CPU-bound work runs in a blocking thread via `spawn_blocking`.
pub async fn run_pass1(uri: Uri, source: String, state: &Arc<WorkspaceState>, client: &Client) {
    let src = source;

    let result = task::spawn_blocking(move || -> Option<(tree_sitter::Tree, Vec<Model>, Option<MigrationFile>)> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into()).ok()?;
        let tree = parser.parse(src.as_str(), None)?;

        let models = if has_sqlalchemy_indicators(&src) {
            extract_models(&src, &tree)
        } else {
            vec![]
        };

        let migration = if has_alembic_indicators(&src) {
            extract_migration(&src, &tree)
        } else {
            None
        };

        Some((tree, models, migration))
    })
    .await;

    match result {
        Ok(Some((tree, models, migration))) => {
            state.parse_trees.insert(uri.clone(), tree);
            state.update_file(&uri, models);
            if let Some(mf) = migration {
                state.update_migration(&uri, mf);
            }
            // Publish the last Pass 2 results for this URI; Pass 2 will recompute shortly.
            let diags = state.diagnostics.get(&uri).map(|d| d.clone()).unwrap_or_default();
            client.publish_diagnostics(uri, diags, None).await;
        }
        Ok(None) | Err(_) => {
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
pub async fn run_pass2(state: &Arc<WorkspaceState>, client: &Client, supports_inlay_hint_refresh: bool) {
    // Run F01 diagnostics for every indexed SA model file, store results, then publish.
    let model_uris: Vec<Uri> = state.file_models.iter().map(|e| e.key().clone()).collect();
    for uri in &model_uris {
        let diags = if let Some(models) = state.file_models.get(uri) {
            let mut d = f01::check_file(&models, state);
            d.extend(f02::check_file(&models, state));
            d
        } else {
            vec![]
        };
        state.diagnostics.insert(uri.clone(), diags.clone());
        client.publish_diagnostics(uri.clone(), diags, None).await;
    }

    // Run Alembic diagnostics for every migration file, then publish.
    let heads = alembic_diag::compute_head_set(state);
    let migration_uris: Vec<Uri> =
        state.migration_files.iter().map(|e| e.key().clone()).collect();
    for uri in &migration_uris {
        let alembic_diags = if let Some(mf) = state.migration_files.get(uri) {
            alembic_diag::check_migration(&mf, state, &heads)
        } else {
            vec![]
        };
        // Merge with any SA model diagnostics already computed for this URI.
        let mut diags = state.diagnostics.get(uri).map(|d| d.clone()).unwrap_or_default();
        diags.extend(alembic_diags);
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
