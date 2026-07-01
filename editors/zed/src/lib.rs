use zed_extension_api::{self as zed, serde_json, settings::LspSettings, LanguageServerId, Result};

const SERVER_NAME: &str = "sqlalchemy-lsp";

struct SqlAlchemyLspExtension;

impl zed::Extension for SqlAlchemyLspExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let env = worktree.shell_env();

        if let Ok(lsp_settings) = LspSettings::for_worktree(SERVER_NAME, worktree) {
            if let Some(binary) = lsp_settings.binary {
                if let Some(path) = binary.path {
                    let args = binary.arguments.unwrap_or_else(|| vec!["lsp".into()]);
                    return Ok(zed::Command { command: path, args, env });
                }
            }
        }

        let binary = worktree
            .which(SERVER_NAME)
            .ok_or_else(binary_not_found_message)?;
        Ok(zed::Command {
            command: binary,
            args: vec!["lsp".into()],
            env,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.initialization_options.clone())
            .unwrap_or_default();
        Ok(Some(settings))
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.settings.clone())
            .unwrap_or_default();
        Ok(Some(settings))
    }
}

/// Message shown in Zed's LSP logs when the server binary is missing. Kept identical
/// across all alex-oleshkevich LSP extensions (see the lsp-maker skill). These
/// extensions never download the binary — the user installs it manually.
fn binary_not_found_message() -> String {
    format!(
        "{SERVER_NAME} was not found on your PATH.\n\
         This extension does not download it — you must install it manually.\n\
         Repository: https://github.com/alex-oleshkevich/{SERVER_NAME}\n\
         Releases:   https://github.com/alex-oleshkevich/{SERVER_NAME}/releases"
    )
}

zed::register_extension!(SqlAlchemyLspExtension);
