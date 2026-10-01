use std::fs;
use zed_extension_api::{
    self as zed, settings::LspSettings, Architecture, DownloadedFileType, GithubReleaseOptions,
    LanguageServerId, LanguageServerInstallationStatus as Status, Os, Result,
};

const REPO: &str = "azhou555/zed-leetcode";

struct LeetCode {
    cached: Option<String>,
}

impl LeetCode {
    /// Release asset name for the current platform (must match .github/workflows/release.yml).
    fn asset_name() -> Result<String> {
        let (os, arch) = zed::current_platform();
        let arch = match arch {
            Architecture::Aarch64 => "aarch64",
            Architecture::X8664 => "x86_64",
            Architecture::X86 => return Err("unsupported architecture: x86".into()),
        };
        let (os_name, ext) = match os {
            Os::Mac => ("macos", ""),
            Os::Linux => ("linux", ""),
            Os::Windows => ("windows", ".exe"),
        };
        Ok(format!("leetcode-zed-{arch}-{os_name}{ext}.gz"))
    }

    /// Download the companion binary from GitHub releases, caching it in the extension dir.
    fn download(&mut self, id: &LanguageServerId) -> Result<String> {
        if let Some(p) = &self.cached {
            if fs::metadata(p).is_ok() {
                return Ok(p.clone());
            }
        }
        zed::set_language_server_installation_status(id, &Status::CheckingForUpdate);
        let release = zed::latest_github_release(
            REPO,
            GithubReleaseOptions { require_assets: true, pre_release: false },
        )?;
        let want = Self::asset_name()?;
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == want)
            .ok_or_else(|| format!("no release asset `{want}` in {}", release.version))?;

        let (os, _) = zed::current_platform();
        let bin = if matches!(os, Os::Windows) { "leetcode-zed.exe" } else { "leetcode-zed" };
        let path = format!("leetcode-zed-{}/{bin}", release.version);
        if fs::metadata(&path).is_err() {
            zed::set_language_server_installation_status(id, &Status::Downloading);
            zed::download_file(&asset.download_url, &path, DownloadedFileType::Gzip)?;
            zed::make_file_executable(&path)?;
        }
        self.cached = Some(path.clone());
        Ok(path)
    }
}

impl zed::Extension for LeetCode {
    fn new() -> Self {
        LeetCode { cached: None }
    }

    /// Resolve the `leetcode-zed` binary: settings path, then $PATH, then a GitHub release.
    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(id.as_ref(), worktree).ok().and_then(|s| s.binary);
        let args = binary
            .as_ref()
            .and_then(|b| b.arguments.clone())
            .unwrap_or_else(|| vec!["lsp".into()]);
        let command = match binary.and_then(|b| b.path).or_else(|| worktree.which("leetcode-zed")) {
            Some(path) => path,
            None => self.download(id)?,
        };
        Ok(zed::Command { command, args, env: worktree.shell_env() })
    }
}

zed::register_extension!(LeetCode);
