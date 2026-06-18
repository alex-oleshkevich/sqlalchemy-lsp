use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower_lsp_server::ls_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Diagnostic, NumberOrString,
    Position, Range, TextEdit, Uri, WorkspaceEdit,
};

use crate::{model::types::Range as MRange, state::WorkspaceState};

// ── LSP range helpers ─────────────────────────────────────────────────────────

fn lsp_range(r: MRange) -> Range {
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

fn diag_overlaps(diag: &Diagnostic, r: MRange) -> bool {
    let ds = diag.range.start.line;
    let de = diag.range.end.line;
    ds <= r.end_line && de >= r.start_line
}

// ── Text helpers ──────────────────────────────────────────────────────────────

/// Convert "CamelCase" → "camel_case" (from rename.rs, duplicated here to avoid coupling).
fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i != 0 {
            out.push('_');
        }
        out.push(ch.to_lowercase().next().unwrap());
    }
    out
}

/// Detect the quote character at `(line, col)` in `source`.
fn quote_char_at(source: &str, line: u32, col: u32) -> char {
    let bytes = source.as_bytes();
    let mut cur_line = 0u32;
    let mut pos = 0usize;
    while pos < bytes.len() && cur_line < line {
        if bytes[pos] == b'\n' {
            cur_line += 1;
        }
        pos += 1;
    }
    let col_pos = pos + col as usize;
    if col_pos < bytes.len() {
        let b = bytes[col_pos];
        if b == b'\'' { '\'' } else { '"' }
    } else {
        '"'
    }
}

/// Wrap `name` in the detected quote style.
fn quoted(source: &str, line: u32, col: u32, name: &str) -> String {
    let q = quote_char_at(source, line, col);
    format!("{q}{name}{q}")
}

// ── Resolve data ──────────────────────────────────────────────────────────────

/// Opaque data stored in `CodeAction.data` and round-tripped via `codeAction/resolve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionData {
    pub action_id: String,
    pub uri: String,
    pub model_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rel_name: Option<String>,
}

fn meta_action(
    title: &str,
    kind: CodeActionKind,
    is_preferred: bool,
    diag: Option<Diagnostic>,
    data: ActionData,
) -> CodeAction {
    CodeAction {
        title: title.to_string(),
        kind: Some(kind),
        diagnostics: diag.map(|d| vec![d]),
        edit: None,
        command: None,
        is_preferred: if is_preferred { Some(true) } else { None },
        disabled: None,
        data: Some(serde_json::to_value(&data).unwrap_or(Value::Null)),
    }
}

// ── textDocument/codeAction ───────────────────────────────────────────────────

pub fn provide_code_actions(
    params: &CodeActionParams,
    state: &WorkspaceState,
) -> Vec<CodeActionOrCommand> {
    let uri = &params.text_document.uri;
    let diags = &params.context.diagnostics;

    let mut actions: Vec<CodeActionOrCommand> = Vec::new();

    let file_models = match state.file_models.get(uri) {
        Some(m) => m,
        None => return actions,
    };

    // For each reported diagnostic, check if we have a fix
    for diag in diags {
        let code = match &diag.code {
            Some(NumberOrString::String(s)) => s.clone(),
            _ => continue,
        };

        for model in file_models.iter() {
            match code.as_str() {
                // REQ-CA-03: generate __tablename__ (Unsafe)
                "SQLA-W101" if diag_overlaps(diag, model.name_range) => {
                    let table_name = to_snake_case(&model.name) + "s";
                    let title = format!("Add `__tablename__ = \"{table_name}\"`");
                    let data = ActionData {
                        action_id: "CA-03".into(),
                        uri: uri.to_string(),
                        model_name: model.name.clone(),
                        rel_name: None,
                    };
                    actions.push(CodeActionOrCommand::CodeAction(meta_action(
                        &title,
                        CodeActionKind::QUICKFIX,
                        false,
                        Some(diag.clone()),
                        data,
                    )));
                }

                // REQ-CA-04: fix back_populates (Safe)
                "SQLA-W402" | "SQLA-W403" => {
                    for rel in model.relationships.values() {
                        if let Some(bp_range) = rel.back_populates_range {
                            if diag_overlaps(diag, bp_range) {
                                // Resolve counterpart
                                if let Some(counterpart) = resolve_back_populates_counterpart(
                                    &model.name,
                                    &rel.name,
                                    state,
                                ) {
                                    let title = format!("Fix `back_populates` to `{counterpart}`");
                                    let data = ActionData {
                                        action_id: "CA-04".into(),
                                        uri: uri.to_string(),
                                        model_name: model.name.clone(),
                                        rel_name: Some(rel.name.clone()),
                                    };
                                    actions.push(CodeActionOrCommand::CodeAction(meta_action(
                                        &title,
                                        CodeActionKind::QUICKFIX,
                                        true,
                                        Some(diag.clone()),
                                        data,
                                    )));
                                }
                            }
                        }
                    }
                }

                // REQ-CA-09: rewrite cascade to all, delete-orphan (Unsafe)
                "SQLA-W409" => {
                    for rel in model.relationships.values() {
                        if let Some(c_range) = rel.cascade_range {
                            if diag_overlaps(diag, c_range) {
                                let data = ActionData {
                                    action_id: "CA-09".into(),
                                    uri: uri.to_string(),
                                    model_name: model.name.clone(),
                                    rel_name: Some(rel.name.clone()),
                                };
                                actions.push(CodeActionOrCommand::CodeAction(meta_action(
                                    "Rewrite cascade to `all, delete-orphan`",
                                    CodeActionKind::QUICKFIX,
                                    false,
                                    Some(diag.clone()),
                                    data,
                                )));
                            }
                        }
                    }
                }

                _ => {}
            }
        }
    }

    // REQ-CA-06: proactive back_populates refactor (Safe, no diagnostic)
    // Offered when cursor is inside a relationship with no back_populates but counterpart exists
    for model in file_models.iter() {
        for rel in model.relationships.values() {
            if rel.back_populates.is_some() {
                continue;
            }
            if !range_overlaps_lsp(&params.range, rel.full_range) {
                continue;
            }
            if let Some(counterpart) =
                resolve_back_populates_counterpart(&model.name, &rel.name, state)
            {
                let title = format!("Add `back_populates=\"{counterpart}\"`");
                let data = ActionData {
                    action_id: "CA-06".into(),
                    uri: uri.to_string(),
                    model_name: model.name.clone(),
                    rel_name: Some(rel.name.clone()),
                };
                actions.push(CodeActionOrCommand::CodeAction(meta_action(
                    &title,
                    CodeActionKind::REFACTOR,
                    true,
                    None,
                    data,
                )));
            }
        }
    }

    actions
}

fn range_overlaps_lsp(lsp: &Range, model_r: MRange) -> bool {
    lsp.start.line <= model_r.end_line && lsp.end.line >= model_r.start_line
}

fn resolve_back_populates_counterpart(
    model_name: &str,
    rel_name: &str,
    state: &WorkspaceState,
) -> Option<String> {
    // Find the model
    let loc = state.model_index.get(model_name)?;
    let uri = loc.uri.clone();
    drop(loc);
    let file_models = state.file_models.get(&uri)?;
    let model = file_models.iter().find(|m| m.name == model_name)?;
    let rel = model.relationships.get(rel_name)?;
    let target_name = rel.target_model.clone();
    drop(file_models);

    // Find the target model
    let target_loc = state.model_index.get(&target_name)?;
    let target_uri = target_loc.uri.clone();
    drop(target_loc);
    let target_models = state.file_models.get(&target_uri)?;
    let target = target_models.iter().find(|m| m.name == target_name)?;

    // Find the counterpart relationship: one that targets the source model
    let counterpart = target
        .relationships
        .values()
        .find(|r| r.target_model == model_name)?;
    Some(counterpart.name.clone())
}

// ── codeAction/resolve ────────────────────────────────────────────────────────

pub fn resolve_code_action(mut action: CodeAction, state: &WorkspaceState) -> CodeAction {
    let data: ActionData = match action
        .data
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
    {
        Some(d) => d,
        None => return action,
    };

    let uri: Uri = match data.uri.parse() {
        Ok(u) => u,
        Err(_) => return action,
    };

    let edit = match data.action_id.as_str() {
        "CA-03" => build_ca_03_edit(&uri, &data.model_name, state),
        "CA-04" => build_ca_04_edit(
            &uri,
            &data.model_name,
            data.rel_name.as_deref().unwrap_or(""),
            state,
        ),
        "CA-06" => build_ca_06_edit(
            &uri,
            &data.model_name,
            data.rel_name.as_deref().unwrap_or(""),
            state,
        ),
        "CA-09" => build_ca_09_edit(
            &uri,
            &data.model_name,
            data.rel_name.as_deref().unwrap_or(""),
            state,
        ),
        _ => None,
    };

    if let Some(we) = edit {
        action.edit = Some(we);
    }
    action
}

// ── Fix implementations ───────────────────────────────────────────────────────

/// REQ-CA-03: insert `    __tablename__ = "table_name"\n` after the class header.
fn build_ca_03_edit(uri: &Uri, model_name: &str, state: &WorkspaceState) -> Option<WorkspaceEdit> {
    let source = state.file_sources.get(uri)?.clone();
    let file_models = state.file_models.get(uri)?;
    let model = file_models.iter().find(|m| m.name == model_name)?;

    let table_name = to_snake_case(model_name) + "s";

    // Find the end of the class header: scan from name_range.start_line for ':'
    let header_line = model.name_range.start_line;
    let insert_line = find_class_header_end(&source, header_line) + 1;

    let edit = TextEdit {
        range: Range {
            start: Position {
                line: insert_line,
                character: 0,
            },
            end: Position {
                line: insert_line,
                character: 0,
            },
        },
        new_text: format!("    __tablename__ = \"{table_name}\"\n"),
    };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);
    Some(WorkspaceEdit::new(changes))
}

/// Scan source lines starting from `start_line` to find the line containing the class header `:`.
fn find_class_header_end(source: &str, start_line: u32) -> u32 {
    for (line_num, line) in source.lines().enumerate() {
        let line_num = line_num as u32;
        if line_num >= start_line && line.trim_end().ends_with(':') {
            return line_num;
        }
    }
    start_line
}

/// REQ-CA-04: replace back_populates string with the resolved counterpart name.
fn build_ca_04_edit(
    uri: &Uri,
    model_name: &str,
    rel_name: &str,
    state: &WorkspaceState,
) -> Option<WorkspaceEdit> {
    let source = state.file_sources.get(uri)?.clone();
    let file_models = state.file_models.get(uri)?;
    let model = file_models.iter().find(|m| m.name == model_name)?;
    let rel = model.relationships.get(rel_name)?;
    let bp_range = rel.back_populates_range?;

    let counterpart = resolve_back_populates_counterpart(model_name, rel_name, state)?;
    let new_text = quoted(
        &source,
        bp_range.start_line,
        bp_range.start_col,
        &counterpart,
    );

    let edit = TextEdit {
        range: lsp_range(bp_range),
        new_text,
    };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);
    Some(WorkspaceEdit::new(changes))
}

/// REQ-CA-06: append `, back_populates="…"` inside the relationship's closing paren.
fn build_ca_06_edit(
    uri: &Uri,
    model_name: &str,
    rel_name: &str,
    state: &WorkspaceState,
) -> Option<WorkspaceEdit> {
    let source = state.file_sources.get(uri)?.clone();
    let file_models = state.file_models.get(uri)?;
    let model = file_models.iter().find(|m| m.name == model_name)?;
    let rel = model.relationships.get(rel_name)?;

    let counterpart = resolve_back_populates_counterpart(model_name, rel_name, state)?;

    // Find the closing `)` of the relationship call by scanning from full_range end backwards
    let close_pos = find_closing_paren_of_call(&source, rel.full_range)?;

    let new_text = format!(", back_populates=\"{counterpart}\"");
    let edit = TextEdit {
        range: Range {
            start: close_pos,
            end: close_pos,
        },
        new_text,
    };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);
    Some(WorkspaceEdit::new(changes))
}

/// REQ-CA-09: replace cascade string with `"all, delete-orphan"`.
fn build_ca_09_edit(
    uri: &Uri,
    model_name: &str,
    rel_name: &str,
    state: &WorkspaceState,
) -> Option<WorkspaceEdit> {
    let source = state.file_sources.get(uri)?.clone();
    let file_models = state.file_models.get(uri)?;
    let model = file_models.iter().find(|m| m.name == model_name)?;
    let rel = model.relationships.get(rel_name)?;
    let c_range = rel.cascade_range?;

    let new_text = quoted(
        &source,
        c_range.start_line,
        c_range.start_col,
        "all, delete-orphan",
    );
    let edit = TextEdit {
        range: lsp_range(c_range),
        new_text,
    };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);
    Some(WorkspaceEdit::new(changes))
}

/// Scan `source` from `full_range.end` backward to find the position of the last `)` that
/// closes the `relationship(...)` call. Returns the position just before that `)`.
fn find_closing_paren_of_call(source: &str, full_range: MRange) -> Option<Position> {
    // Collect lines from start to end of the full_range
    let end_line = full_range.end_line as usize;
    let lines: Vec<&str> = source.lines().collect();
    if end_line >= lines.len() {
        return None;
    }

    // Scan the end line backwards for ')'
    let line = lines[end_line];
    let end_col = if end_line == full_range.end_line as usize {
        full_range.end_col as usize
    } else {
        line.len()
    };
    let search_col = end_col.min(line.len());

    // Search backwards for the last ')' on the end line
    let pos = line[..search_col].rfind(')')?;
    Some(Position {
        line: full_range.end_line,
        character: pos as u32,
    })
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::{Model, Range as MRange, Relationship};
    use crate::state::WorkspaceState;
    use std::collections::HashMap;
    use tower_lsp_server::ls_types::{CodeActionContext, NumberOrString, Position, Range, Uri};

    fn uri(s: &str) -> Uri {
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
    fn lsp_rng(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range {
            start: Position {
                line: sl,
                character: sc,
            },
            end: Position {
                line: el,
                character: ec,
            },
        }
    }

    fn dummy_diag(code: &str, range: Range) -> Diagnostic {
        Diagnostic {
            range,
            severity: None,
            code: Some(NumberOrString::String(code.to_string())),
            code_description: None,
            source: None,
            message: String::new(),
            related_information: None,
            tags: None,
            data: None,
        }
    }

    fn rel(
        name: &str,
        target: &str,
        bp: Option<&str>,
        bp_range: Option<MRange>,
        cascade: Option<&str>,
        c_range: Option<MRange>,
        r: MRange,
    ) -> Relationship {
        Relationship {
            name: name.to_string(),
            target_model: target.to_string(),
            explicit_target: None,
            back_populates: bp.map(String::from),
            lazy: None,
            uselist: None,
            secondary: None,
            cascade: cascade.map(String::from),
            is_list: true,
            backref: None,
            remote_side: false,
            has_foreign_keys: false,
            viewonly: None,
            name_range: r,
            full_range: r,
            target_range: None,
            back_populates_range: bp_range,
            cascade_range: c_range,
        }
    }

    fn base_model(name: &str, table: &str, line: u32) -> Model {
        Model {
            name: name.to_string(),
            table_name: Some(table.to_string()),
            bases: vec![],
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: rng(line, 6, line, 6 + name.len() as u32),
            full_range: rng(line, 0, line + 10, 0),
        }
    }

    fn mk_params(uri: Uri, diags: Vec<Diagnostic>, cursor: Range) -> CodeActionParams {
        CodeActionParams {
            text_document: tower_lsp_server::ls_types::TextDocumentIdentifier { uri },
            range: cursor,
            context: CodeActionContext {
                diagnostics: diags,
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    // ── REQ-CA-03: generate __tablename__ ────────────────────────────────────

    #[test]
    fn req_ca_03_offered_for_w101() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let model = base_model("User", "users", 5);
        state.update_file(&u, vec![model]);
        state
            .file_sources
            .insert(u.clone(), "class User(Base):\n    pass\n".to_string());

        let diag = dummy_diag("SQLA-W101", lsp_rng(5, 6, 5, 10));
        let params = mk_params(u, vec![diag], lsp_rng(5, 6, 5, 10));
        let actions = provide_code_actions(&params, &state);
        assert_eq!(actions.len(), 1);
        if let CodeActionOrCommand::CodeAction(a) = &actions[0] {
            assert!(a.title.contains("users"), "title: {}", a.title);
            assert!(a.title.contains("__tablename__"));
            assert_eq!(a.kind, Some(CodeActionKind::QUICKFIX));
        } else {
            panic!("expected CodeAction");
        }
    }

    #[test]
    fn req_ca_03_resolve_inserts_tablename() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let model = base_model("User", "users", 0);
        state.update_file(&u, vec![model]);
        let src = "class User(Base):\n    id = None\n".to_string();
        state.file_sources.insert(u.clone(), src);

        let diag = dummy_diag("SQLA-W101", lsp_rng(0, 6, 0, 10));
        let params = mk_params(u.clone(), vec![diag], lsp_rng(0, 0, 0, 0));
        let actions = provide_code_actions(&params, &state);
        assert_eq!(actions.len(), 1);
        let action = if let CodeActionOrCommand::CodeAction(a) = actions.into_iter().next().unwrap()
        {
            a
        } else {
            panic!()
        };

        let resolved = resolve_code_action(action, &state);
        let edit = resolved.edit.expect("edit should be present after resolve");
        let changes = edit.changes.expect("changes expected");
        let edits = &changes[&u];
        assert_eq!(edits.len(), 1);
        assert!(
            edits[0].new_text.contains("__tablename__"),
            "new_text: {}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains("users"),
            "new_text: {}",
            edits[0].new_text
        );
        // Inserted at line 1 (the first body line)
        assert_eq!(edits[0].range.start.line, 1);
    }

    // ── REQ-CA-04: fix back_populates ────────────────────────────────────────

    #[test]
    fn req_ca_04_offered_for_w402() {
        let state = WorkspaceState::new();

        // Set up Post model with "author" rel pointing to User
        let post_uri = uri("file:///post.py");
        let mut post = base_model("Post", "posts", 2);
        post.relationships.insert(
            "author".into(),
            rel(
                "author",
                "User",
                Some("wrong_posts"),     // wrong back_populates
                Some(rng(4, 31, 4, 44)), // range of "wrong_posts" string including quotes
                None,
                None,
                rng(4, 0, 4, 60),
            ),
        );
        state.update_file(&post_uri, vec![post]);

        // Set up User model with "posts" rel pointing to Post
        let user_uri = uri("file:///user.py");
        let mut user = base_model("User", "users", 0);
        user.relationships.insert(
            "posts".into(),
            rel(
                "posts",
                "Post",
                Some("author"),
                None,
                None,
                None,
                rng(2, 0, 2, 50),
            ),
        );
        state.update_file(&user_uri, vec![user]);

        let diag = dummy_diag("SQLA-W402", lsp_rng(4, 31, 4, 44));
        let params = mk_params(post_uri, vec![diag], lsp_rng(4, 0, 4, 60));
        let actions = provide_code_actions(&params, &state);
        assert_eq!(actions.len(), 1);
        if let CodeActionOrCommand::CodeAction(a) = &actions[0] {
            assert!(a.title.contains("posts"), "title: {}", a.title);
            assert_eq!(a.is_preferred, Some(true)); // Safe fix
        } else {
            panic!();
        }
    }

    #[test]
    fn req_ca_04_resolve_replaces_string() {
        let state = WorkspaceState::new();

        let post_uri = uri("file:///post.py");
        let mut post = base_model("Post", "posts", 2);
        post.relationships.insert(
            "author".into(),
            rel(
                "author",
                "User",
                Some("wrong"),
                Some(rng(4, 31, 4, 38)),
                None,
                None,
                rng(4, 0, 4, 60),
            ),
        );
        state.update_file(&post_uri, vec![post]);

        let user_uri = uri("file:///user.py");
        let mut user = base_model("User", "users", 0);
        user.relationships.insert(
            "posts".into(),
            rel(
                "posts",
                "Post",
                Some("author"),
                None,
                None,
                None,
                rng(2, 0, 2, 50),
            ),
        );
        state.update_file(&user_uri, vec![user]);

        let src = "class Post(Base):\n    pass\n    author = relationship(\"User\", back_populates=\"wrong\")\n".to_string();
        state.file_sources.insert(post_uri.clone(), src);

        let diag = dummy_diag("SQLA-W402", lsp_rng(4, 31, 4, 38));
        let params = mk_params(post_uri.clone(), vec![diag], lsp_rng(4, 0, 4, 60));
        let actions = provide_code_actions(&params, &state);
        let action = if let CodeActionOrCommand::CodeAction(a) = actions.into_iter().next().unwrap()
        {
            a
        } else {
            panic!()
        };

        let resolved = resolve_code_action(action, &state);
        let edit = resolved.edit.expect("edit");
        let changes = edit.changes.expect("changes");
        let edits = &changes[&post_uri];
        assert!(
            edits[0].new_text.contains("posts"),
            "new_text: {}",
            edits[0].new_text
        );
    }

    // ── REQ-CA-09: rewrite cascade ────────────────────────────────────────────

    #[test]
    fn req_ca_09_offered_for_w409() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let mut post = base_model("Post", "posts", 0);
        post.relationships.insert(
            "comments".into(),
            rel(
                "comments",
                "Comment",
                None,
                None,
                Some("delete-orphan"),
                Some(rng(3, 28, 3, 44)),
                rng(3, 0, 3, 60),
            ),
        );
        state.update_file(&u, vec![post]);

        let diag = dummy_diag("SQLA-W409", lsp_rng(3, 28, 3, 44));
        let params = mk_params(u, vec![diag], lsp_rng(3, 0, 3, 60));
        let actions = provide_code_actions(&params, &state);
        assert_eq!(actions.len(), 1);
        if let CodeActionOrCommand::CodeAction(a) = &actions[0] {
            assert!(a.title.contains("delete-orphan"), "title: {}", a.title);
        } else {
            panic!();
        }
    }

    #[test]
    fn req_ca_09_resolve_replaces_cascade() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let mut post = base_model("Post", "posts", 0);
        post.relationships.insert(
            "comments".into(),
            rel(
                "comments",
                "Comment",
                None,
                None,
                Some("delete-orphan"),
                Some(rng(3, 28, 3, 44)),
                rng(3, 0, 3, 60),
            ),
        );
        state.update_file(&u, vec![post]);

        let src = "class Post(Base):\n    pass\n    pass\n    comments = relationship(\"delete-orphan\")\n".to_string();
        state.file_sources.insert(u.clone(), src);

        let diag = dummy_diag("SQLA-W409", lsp_rng(3, 28, 3, 44));
        let params = mk_params(u.clone(), vec![diag], lsp_rng(3, 0, 3, 60));
        let actions = provide_code_actions(&params, &state);
        let action = if let CodeActionOrCommand::CodeAction(a) = actions.into_iter().next().unwrap()
        {
            a
        } else {
            panic!()
        };

        let resolved = resolve_code_action(action, &state);
        let edit = resolved.edit.expect("edit");
        let changes = edit.changes.expect("changes");
        let edits = &changes[&u];
        assert!(
            edits[0].new_text.contains("all, delete-orphan"),
            "new_text: {}",
            edits[0].new_text
        );
    }

    // ── REQ-CA-06: proactive back_populates ──────────────────────────────────

    #[test]
    fn req_ca_06_offered_for_one_sided_relationship() {
        let state = WorkspaceState::new();

        let post_uri = uri("file:///post.py");
        let mut post = base_model("Post", "posts", 0);
        post.relationships.insert(
            "author".into(),
            rel(
                "author",
                "User",
                None,
                None, // no back_populates
                None,
                None,
                rng(3, 0, 3, 60),
            ),
        );
        state.update_file(&post_uri, vec![post]);

        let user_uri = uri("file:///user.py");
        let mut user = base_model("User", "users", 0);
        user.relationships.insert(
            "posts".into(),
            rel(
                "posts",
                "Post",
                Some("author"),
                None,
                None,
                None,
                rng(2, 0, 2, 50),
            ),
        );
        state.update_file(&user_uri, vec![user]);

        // Cursor inside the relationship range
        let params = mk_params(post_uri, vec![], lsp_rng(3, 0, 3, 60));
        let actions = provide_code_actions(&params, &state);
        assert_eq!(actions.len(), 1);
        if let CodeActionOrCommand::CodeAction(a) = &actions[0] {
            assert_eq!(a.kind, Some(CodeActionKind::REFACTOR));
            assert!(a.title.contains("back_populates"), "title: {}", a.title);
            assert!(a.title.contains("posts"), "title: {}", a.title);
        } else {
            panic!();
        }
    }

    #[test]
    fn req_ca_06_not_offered_when_already_wired() {
        let state = WorkspaceState::new();
        let u = uri("file:///post.py");
        let mut post = base_model("Post", "posts", 0);
        post.relationships.insert(
            "author".into(),
            rel(
                "author",
                "User",
                Some("posts"),
                Some(rng(3, 30, 3, 37)), // already has back_populates
                None,
                None,
                rng(3, 0, 3, 60),
            ),
        );
        state.update_file(&u, vec![post]);

        let params = mk_params(u, vec![], lsp_rng(3, 0, 3, 60));
        let actions = provide_code_actions(&params, &state);
        assert!(
            actions.is_empty(),
            "should not offer CA-06 when already wired"
        );
    }

    // ── no fix for unfixable codes ────────────────────────────────────────────

    #[test]
    fn no_fix_for_unfixable_code() {
        let state = WorkspaceState::new();
        let u = uri("file:///user.py");
        let model = base_model("User", "users", 0);
        state.update_file(&u, vec![model]);

        // SQLA-W104 (missing primary key) has no fix
        let diag = dummy_diag("SQLA-W104", lsp_rng(0, 0, 0, 10));
        let params = mk_params(u, vec![diag], lsp_rng(0, 0, 0, 10));
        let actions = provide_code_actions(&params, &state);
        assert!(actions.is_empty());
    }
}
