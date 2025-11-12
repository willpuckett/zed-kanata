use zed_extension_api::{self as zed, LanguageServerId, Result};

struct KanataExtension;

impl zed::Extension for KanataExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        // Use the system-installed kanata-lsp binary
        Ok(zed::Command {
            command: "/usr/local/bin/kanata-lsp".to_string(),
            args: vec![],
            env: worktree.shell_env(),
        })
    }
}

zed_extension_api::register_extension!(KanataExtension);
