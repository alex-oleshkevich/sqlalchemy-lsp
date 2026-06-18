use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use dashmap::DashMap;
use tokio::task::JoinHandle;
use tower_lsp_server::{
    Client, LanguageServer,
    jsonrpc::Result,
    ls_types::{
        CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams,
        CodeActionProviderCapability, CompletionOptions, CompletionParams, CompletionResponse,
        DiagnosticOptions, DiagnosticServerCapabilities, DidChangeTextDocumentParams,
        DidChangeWatchedFilesParams, DidChangeWatchedFilesRegistrationOptions,
        DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
        DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
        ExecuteCommandOptions, ExecuteCommandParams, FileChangeType, FileSystemWatcher,
        FullDocumentDiagnosticReport, GlobPattern, GotoDefinitionParams, GotoDefinitionResponse,
        Hover, HoverParams, InitializeParams, InitializeResult, InitializedParams, InlayHint,
        InlayHintParams, MessageType, OneOf, PositionEncodingKind, PrepareRenameResponse,
        ReferenceParams, Registration, RelatedFullDocumentDiagnosticReport, RenameOptions,
        RenameParams, ServerCapabilities, ServerInfo, SignatureHelpOptions, SignatureHelpParams,
        TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
        WorkspaceEdit, WorkspaceSymbolOptions, WorkspaceSymbolParams, WorkspaceSymbolResponse,
    },
};

use crate::{
    features::{
        code_action, completion, definition, hover, inlay_hints, references, rename, schema,
        signature_help, symbols,
    },
    pipeline::{run_pass1, run_pass2},
    state::WorkspaceState,
    util::positions::apply_changes,
};

// ── Backend struct ────────────────────────────────────────────────────────────

pub struct Backend {
    client: Client,
    state: Arc<WorkspaceState>,
    /// Monotonic counter bumped on every Pass 1 change; used to detect superseded Pass 2 tasks.
    generation: Arc<AtomicU64>,
    /// True when UTF-8 position encoding was negotiated; false means UTF-16 (the LSP default).
    uses_utf8: Arc<AtomicBool>,
    /// Handle for the current pending debounce task (aborted and replaced on each new change).
    debounce_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// True when the client advertised `workspace.inlayHint.refreshSupport`.
    supports_inlay_hint_refresh: Arc<AtomicBool>,
    /// True when the client advertised `workspace.didChangeWatchedFiles.dynamicRegistration`.
    supports_dynamic_registration: Arc<AtomicBool>,
    /// Per-URI locks to serialize document mutations (didOpen/didChange/didClose) for each URI.
    uri_locks: Arc<DashMap<Uri, Arc<tokio::sync::Mutex<()>>>>,
    /// Root URI discovered during `initialize`, used to drive the background workspace scan.
    root_uri: Arc<Mutex<Option<Uri>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(WorkspaceState::new()),
            generation: Arc::new(AtomicU64::new(0)),
            uses_utf8: Arc::new(AtomicBool::new(false)),
            debounce_handle: Arc::new(Mutex::new(None)),
            supports_inlay_hint_refresh: Arc::new(AtomicBool::new(false)),
            supports_dynamic_registration: Arc::new(AtomicBool::new(false)),
            uri_locks: Arc::new(DashMap::new()),
            root_uri: Arc::new(Mutex::new(None)),
        }
    }

    fn encoding(&self) -> PositionEncodingKind {
        if self.uses_utf8.load(Ordering::Relaxed) {
            PositionEncodingKind::UTF8
        } else {
            PositionEncodingKind::UTF16
        }
    }

    /// Return (or lazily create) the per-URI mutex that serializes document mutations.
    fn uri_lock(&self, uri: &Uri) -> Arc<tokio::sync::Mutex<()>> {
        let entry = self
            .uri_locks
            .entry(uri.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())));
        Arc::clone(&*entry)
    }

    /// Bump the generation counter and schedule a debounced Pass 2 (~300 ms).
    /// Any previously pending Pass 2 task is aborted.
    fn schedule_relink(&self) {
        let target = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let old = {
            let mut guard = self.debounce_handle.lock().unwrap();
            let state = Arc::clone(&self.state);
            let gen_counter = Arc::clone(&self.generation);
            let client = self.client.clone();
            let refresh = self.supports_inlay_hint_refresh.load(Ordering::Relaxed);
            let handle = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                // Discard this Pass 2 if a newer change has superseded it.
                if gen_counter.load(Ordering::SeqCst) != target {
                    return;
                }
                run_pass2(&state, &client, refresh).await;
            });
            guard.replace(handle)
        };
        if let Some(h) = old {
            h.abort();
        }
    }
}

// ── LanguageServer implementation ─────────────────────────────────────────────

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Negotiate position encoding: prefer UTF-8 when the client advertises it.
        let use_utf8 = params
            .capabilities
            .general
            .as_ref()
            .and_then(|g| g.position_encodings.as_ref())
            .map(|encs| encs.contains(&PositionEncodingKind::UTF8))
            .unwrap_or(false);
        self.uses_utf8.store(use_utf8, Ordering::Relaxed);

        // Record inlayHint refresh support for post-relink notifications.
        let refresh = params
            .capabilities
            .workspace
            .as_ref()
            .and_then(|w| w.inlay_hint.as_ref())
            .and_then(|h| h.refresh_support)
            .unwrap_or(false);
        self.supports_inlay_hint_refresh
            .store(refresh, Ordering::Relaxed);

        // Record whether the client supports dynamic file-watcher registration.
        let dyn_reg = params
            .capabilities
            .workspace
            .as_ref()
            .and_then(|w| w.did_change_watched_files.as_ref())
            .and_then(|w| w.dynamic_registration)
            .unwrap_or(false);
        self.supports_dynamic_registration
            .store(dyn_reg, Ordering::Relaxed);

        // Store the root URI for the background workspace scan started in `initialized`.
        #[allow(deprecated)]
        let root = params
            .workspace_folders
            .as_deref()
            .and_then(|f| f.first())
            .map(|f| f.uri.clone())
            .or(params.root_uri.clone());
        *self.root_uri.lock().unwrap() = root;

        let pos_enc = if use_utf8 {
            PositionEncodingKind::UTF8
        } else {
            PositionEncodingKind::UTF16
        };

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                position_encoding: Some(pos_enc),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("sqlalchemy-lsp".to_string()),
                        inter_file_dependencies: true,
                        workspace_diagnostics: false,
                        work_done_progress_options: Default::default(),
                    },
                )),
                inlay_hint_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::REFACTOR,
                        ]),
                        resolve_provider: Some(true),
                        work_done_progress_options: Default::default(),
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "\"".to_string(),
                        "'".to_string(),
                        "(".to_string(),
                        ",".to_string(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Right(WorkspaceSymbolOptions {
                    work_done_progress_options: Default::default(),
                    resolve_provider: None,
                })),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                hover_provider: Some(tower_lsp_server::ls_types::HoverProviderCapability::Simple(
                    true,
                )),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["sqlalchemy.showSchema".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                ..ServerCapabilities::default()
            },
            offset_encoding: if use_utf8 {
                Some("utf-8".to_string())
            } else {
                None
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "sqlalchemy-lsp initialized")
            .await;

        // Register a file watcher only when the client supports dynamic registration.
        if self.supports_dynamic_registration.load(Ordering::Relaxed) {
            let reg_opts = serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                watchers: vec![
                    FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/*.py".to_string()),
                        kind: None,
                    },
                    FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/pyproject.toml".to_string()),
                        kind: None,
                    },
                    FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/alembic.ini".to_string()),
                        kind: None,
                    },
                ],
            })
            .ok();
            if let Err(e) = self
                .client
                .register_capability(vec![Registration {
                    id: "watch-python-files".to_string(),
                    method: "workspace/didChangeWatchedFiles".to_string(),
                    register_options: reg_opts,
                }])
                .await
            {
                tracing::warn!("could not register file watcher: {e}");
            }
        }

        // Kick off the background workspace scan after returning from `initialized`.
        if let Some(root) = self.root_uri.lock().unwrap().clone() {
            let state = Arc::clone(&self.state);
            let client = self.client.clone();
            let generation = Arc::clone(&self.generation);
            let refresh = self.supports_inlay_hint_refresh.load(Ordering::Relaxed);
            tokio::spawn(async move {
                scan_workspace(root, state, client, generation, refresh).await;
            });
        }
    }

    async fn shutdown(&self) -> Result<()> {
        if let Some(h) = self.debounce_handle.lock().unwrap().take() {
            h.abort();
        }
        Ok(())
    }

    // ── Document lifecycle ────────────────────────────────────────────────────

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let source = params.text_document.text;
        self.state.open_uris.insert(uri.clone(), ());
        {
            let lock = self.uri_lock(&uri);
            let _guard = lock.lock().await;
            self.state.file_sources.insert(uri.clone(), source.clone());
        }
        run_pass1(uri.clone(), source, &self.state, &self.client).await;
        self.schedule_relink();
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let encoding = self.encoding();
        // Apply incremental edits atomically: serialize per-URI so burst changes land in order.
        let new_source = {
            let lock = self.uri_lock(&uri);
            let _guard = lock.lock().await;
            let current = self
                .state
                .file_sources
                .get(&uri)
                .map(|s| s.clone())
                .unwrap_or_default();
            let next = apply_changes(&current, &params.content_changes, &encoding);
            self.state.file_sources.insert(uri.clone(), next.clone());
            next
        };
        run_pass1(uri.clone(), new_source, &self.state, &self.client).await;
        self.schedule_relink();
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        // Use the text included with the save event when available; fall back to stored source.
        let source = params
            .text
            .or_else(|| self.state.file_sources.get(&uri).map(|s| s.clone()));
        if let Some(src) = source {
            run_pass1(uri, src, &self.state, &self.client).await;
            self.schedule_relink();
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Remove from open_uris so the watcher can take over for this URI.
        // Facts and diagnostics persist until the file is deleted (REQ-ARCH-11).
        self.state.open_uris.remove(&params.text_document.uri);
    }

    // ── File watching ─────────────────────────────────────────────────────────

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        for event in params.changes {
            let uri = event.uri;
            // Open-buffer overlay: ignore watcher events for files open in the editor.
            if self.state.open_uris.contains_key(&uri) {
                continue;
            }
            if event.typ == FileChangeType::CREATED || event.typ == FileChangeType::CHANGED {
                if let Some(path) = uri.to_file_path() {
                    if let Ok(source) = std::fs::read_to_string(path.as_ref()) {
                        self.state.file_sources.insert(uri.clone(), source.clone());
                        run_pass1(uri, source, &self.state, &self.client).await;
                    }
                }
            } else if event.typ == FileChangeType::DELETED {
                self.state.remove_file(&uri);
                // Explicit empty publish so squiggles disappear (REQ-ARCH-11).
                self.client.publish_diagnostics(uri, vec![], None).await;
            }
        }
        self.schedule_relink();
    }

    // ── Pull diagnostics (LSP 3.17) ───────────────────────────────────────────

    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let uri = params.text_document.uri;
        let items = self
            .state
            .diagnostics
            .get(&uri)
            .map(|d| d.clone())
            .unwrap_or_default();
        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items,
                },
            }),
        ))
    }

    // ── Completions ───────────────────────────────────────────────────────────

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let source = self
            .state
            .file_sources
            .get(&uri)
            .map(|s| s.clone())
            .unwrap_or_default();
        let items = completion::provide_completions(&uri, &source, pos, &self.state);
        Ok(items.map(CompletionResponse::Array))
    }

    // ── Signature help ────────────────────────────────────────────────────────

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> Result<Option<tower_lsp_server::ls_types::SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let source = self
            .state
            .file_sources
            .get(&uri)
            .map(|s| s.clone())
            .unwrap_or_default();
        Ok(signature_help::provide_signature_help(
            &uri,
            &source,
            pos,
            &self.state,
        ))
    }

    // ── Hover ─────────────────────────────────────────────────────────────────

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(hover::provide_hover(&uri, pos, &self.state))
    }

    // ── Go-to-definition ──────────────────────────────────────────────────────

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let source = self
            .state
            .file_sources
            .get(&uri)
            .map(|s| s.clone())
            .unwrap_or_default();
        let loc = definition::resolve_definition(&uri, &source, pos, &self.state);
        Ok(loc.map(GotoDefinitionResponse::Scalar))
    }

    // ── Find references ───────────────────────────────────────────────────────

    async fn references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<tower_lsp_server::ls_types::Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let include_decl = params.context.include_declaration;
        let locs = references::provide_references(&uri, pos, include_decl, &self.state);
        Ok(if locs.is_empty() { None } else { Some(locs) })
    }

    // ── Code actions ──────────────────────────────────────────────────────────

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<Vec<CodeActionOrCommand>>> {
        let actions = code_action::provide_code_actions(&params, &self.state);
        Ok(Some(actions))
    }

    async fn code_action_resolve(&self, action: CodeAction) -> Result<CodeAction> {
        Ok(code_action::resolve_code_action(action, &self.state))
    }

    // ── Inlay hints ───────────────────────────────────────────────────────────

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let hints = inlay_hints::provide_inlay_hints(&uri, &range, &self.state);
        Ok(Some(hints))
    }

    // ── Execute command ───────────────────────────────────────────────────────

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> tower_lsp_server::jsonrpc::Result<Option<tower_lsp_server::ls_types::LSPAny>> {
        if params.command != "sqlalchemy.showSchema" {
            return Ok(None);
        }
        let format = params
            .arguments
            .first()
            .and_then(|v| v.as_str())
            .unwrap_or("mermaid");
        let rendered = schema::render_schema(&self.state, format);
        Ok(Some(serde_json::Value::String(rendered)))
    }

    // ── Workspace symbols ─────────────────────────────────────────────────────

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<WorkspaceSymbolResponse>> {
        let resp = symbols::provide_symbols(&params.query, &self.state);
        Ok(Some(resp))
    }

    // ── Rename ────────────────────────────────────────────────────────────────

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let pos = params.position;
        Ok(rename::prepare_rename(&uri, pos, &self.state))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        Ok(rename::compute_rename(
            &uri,
            pos,
            &params.new_name,
            &self.state,
        ))
    }
}

// ── Background workspace scan ─────────────────────────────────────────────────

async fn scan_workspace(
    root_uri: Uri,
    state: Arc<WorkspaceState>,
    client: Client,
    generation: Arc<AtomicU64>,
    supports_inlay_hint_refresh: bool,
) {
    use crate::parsing::python::{has_alembic_indicators, has_sqlalchemy_indicators};

    let Some(root_path) = root_uri.to_file_path() else {
        return;
    };

    // Collect .py files matching indicators in a blocking thread.
    let root_owned: PathBuf = root_path.as_ref().to_path_buf();
    let Ok(py_files) =
        tokio::task::spawn_blocking(move || crate::pipeline::collect_py_files(&root_owned)).await
    else {
        return;
    };

    // Read and filter files, then run Pass 1 on each matching one.
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
        // Skip files already open in the editor (editor buffer takes precedence).
        if state.open_uris.contains_key(&uri) {
            continue;
        }
        state.file_sources.insert(uri.clone(), source.clone());
        run_pass1(uri, source, &state, &client).await;
    }

    // Single Pass 2 after the full scan.
    run_pass2(&state, &client, supports_inlay_hint_refresh).await;
    // Bump generation so any in-flight editor Pass 2 knows it's superseded.
    generation.fetch_add(1, Ordering::SeqCst);
}
