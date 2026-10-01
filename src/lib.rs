use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

struct LeetCode;

impl zed::Extension for LeetCode {
    fn new() -> Self {
        LeetCode
    }

    /// `lsp.leetcode.binary.path` from settings, else `leetcode-zed` on $PATH.
    // ponytail: no auto-download; add GitHub-release fetching once binaries are published.
    fn language_server_command(&mut self, id: &LanguageServerId, worktree: &zed::Worktree) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(id.as_ref(), worktree).ok().and_then(|s| s.binary);
        let command = binary
            .as_ref()
            .and_then(|b| b.path.clone())
            .or_else(|| worktree.which("leetcode-zed"))
            .ok_or("leetcode-zed not found: run `cargo install --path server` from the zed-leetcode repo, or set lsp.leetcode.binary.path")?;
        Ok(zed::Command {
            command,
            args: binary.and_then(|b| b.arguments).unwrap_or_else(|| vec!["lsp".into()]),
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(LeetCode);
