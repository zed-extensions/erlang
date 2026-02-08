use std::fs;

use zed_extension_api::{self as zed, LanguageServerId, Result};

pub struct ErlangLs {
    cached_binary_path: Option<String>,
}

impl ErlangLs {
    pub const LANGUAGE_SERVER_ID: &'static str = "erlang-ls";

    pub fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    pub fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: self.language_server_binary_path(language_server_id, worktree)?,
            args: vec!["server".to_string()],
            env: Default::default(),
        })
    }

    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        if let Some(path) = worktree.which(Self::LANGUAGE_SERVER_ID) {
            return Ok(path);
        }

        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).is_ok_and(|stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = zed::latest_github_release(
            "erlang-ls/erlang_ls",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (platform, _arch) = zed::current_platform();
        let otp_version = match platform {
            zed::Os::Mac | zed::Os::Linux => "27",
            zed::Os::Windows => "26.2.5.3",
        };
        let asset_name = {
            let os = match platform {
                zed::Os::Mac => "macos",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "windows",
            };

            format!(
                "{}-{os}-{otp_version}.tar.gz",
                Self::LANGUAGE_SERVER_ID.replace("-", "_"),
            )
        };

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

        let version_dir = format!(
            "{}-v{}-otp-{}",
            Self::LANGUAGE_SERVER_ID,
            release.version,
            otp_version,
        );
        let binary_path = format!(
            "{}/{}",
            version_dir,
            Self::LANGUAGE_SERVER_ID.replace("-", "_"),
        );

        if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(
                &asset.download_url,
                &version_dir,
                zed::DownloadedFileType::GzipTar,
            )
            .map_err(|e| format!("failed to download file: {e}"))?;

            let entries =
                fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                if entry.file_name().to_str() != Some(&version_dir) {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}
