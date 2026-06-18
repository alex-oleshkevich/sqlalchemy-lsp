use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree};

struct SqlAlchemyLspExtension;

impl zed::Extension for SqlAlchemyLspExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let env = worktree.shell_env();
        let binary = worktree
            .which("sqlalchemy-lsp")
            .ok_or_else(|| "sqlalchemy-lsp not found in PATH".to_string())?;
        Ok(Command {
            command: binary,
            args: vec!["lsp".into()],
            env,
        })
    }
}

zed::register_extension!(SqlAlchemyLspExtension);
