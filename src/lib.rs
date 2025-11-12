use zed_extension_api::{self as zed, LanguageServerId, Result};
use std::fs;

struct KanataExtension {
    cached_binary_path: Option<String>,
}

impl KanataExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        // First check if kanata-lsp is installed in PATH
        if let Some(path) = worktree.which("kanata-lsp") {
            return Ok(path);
        }

        // Check if we have a cached binary from a previous installation
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).map(|stat| stat.is_file()).unwrap_or(false) {
                return Ok(path.clone());
            }
        }

        // Download pre-built binary from GitHub releases
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            "willpuckett/zed-kanata",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (platform, arch) = zed::current_platform();
        
        // Construct the asset name based on platform and architecture
        let asset_name = format!(
            "kanata-lsp-{os}-{arch}{ext}",
            os = match platform {
                zed::Os::Mac => "macos",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "windows",
            },
            arch = match arch {
                zed::Architecture::Aarch64 => "aarch64",
                zed::Architecture::X8664 => "x86_64",
                zed::Architecture::X86 => "x86",
            },
            ext = if platform == zed::Os::Windows { ".exe" } else { "" }
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {asset_name:?}"))?;

        let version_dir = format!("kanata-lsp-{}", release.version);
        let binary_name = format!("kanata-lsp{}", 
            if platform == zed::Os::Windows { ".exe" } else { "" }
        );
        let binary_path = format!("{version_dir}/{binary_name}");

        if !fs::metadata(&binary_path).map(|stat| stat.is_file()).unwrap_or(false) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            // Create version directory
            fs::create_dir_all(&version_dir)
                .map_err(|e| format!("failed to create directory: {e}"))?;

            // Download directly to the binary path
            zed::download_file(
                &asset.download_url,
                &binary_path,
                zed::DownloadedFileType::Uncompressed,
            )
            .map_err(|e| format!("failed to download file: {e}"))?;

            zed::make_file_executable(&binary_path)?;

            // Clean up old versions
            let entries = fs::read_dir(".")
                .map_err(|e| format!("failed to list working directory: {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("failed to load directory entry: {e}"))?;
                if entry.file_name().to_str() != Some(&version_dir)
                    && entry.file_name().to_string_lossy().starts_with("kanata-lsp-")
                {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::None,
        );

        let full_path = std::env::current_dir()
            .map_err(|e| format!("failed to get current directory: {}", e))?
            .join(&binary_path)
            .to_string_lossy()
            .to_string();

        self.cached_binary_path = Some(full_path.clone());
        Ok(full_path)
    }
}

impl zed::Extension for KanataExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let command = self.language_server_binary_path(language_server_id, worktree)?;
        
        Ok(zed::Command {
            command,
            args: vec![],
            env: worktree.shell_env(),
        })
    }
}

zed_extension_api::register_extension!(KanataExtension);
