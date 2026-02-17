use std::fs;

use zed_extension_api::{self as zed, LanguageServerId, Result};

use crate::language_servers::util;

pub struct ErlangLanguagePlatform {
    cached_binary_path: Option<String>,
}

impl ErlangLanguagePlatform {
    pub const LANGUAGE_SERVER_ID: &'static str = "elp";

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

        if let Some(path) = &self.cached_binary_path
            && fs::metadata(path).is_ok_and(|stat| stat.is_file())
        {
            return Ok(path.clone());
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let (platform, arch) = zed::current_platform();
        let otp_version = "28";

        let release = match zed::latest_github_release(
            "WhatsApp/erlang-language-platform",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        ) {
            Ok(release) => release,
            Err(_) => {
                if let Some(binary_path) = util::find_existing_binary(
                    Self::LANGUAGE_SERVER_ID,
                    otp_version,
                    Self::LANGUAGE_SERVER_ID,
                ) {
                    self.cached_binary_path = Some(binary_path.clone());
                    return Ok(binary_path);
                }
                return Err("failed to download latest github release".to_string());
            }
        };

        let asset_name = {
            let (os, os_target) = match platform {
                zed::Os::Mac => ("macos", "apple-darwin"),
                zed::Os::Linux => ("linux", "unknown-linux-gnu"),
                zed::Os::Windows => ("windows", "pc-windows-msvc"),
            };

            format!(
                "{}-{os}-{arch}-{os_target}-otp-{otp_version}.tar.gz",
                Self::LANGUAGE_SERVER_ID,
                arch = match arch {
                    zed::Architecture::Aarch64 => "aarch64",
                    zed::Architecture::X8664 => "x86_64",
                    zed::Architecture::X86 =>
                        return Err(format!("unsupported architecture: {arch:?}")),
                },
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
        let binary_path = format!("{}/{}", version_dir, Self::LANGUAGE_SERVER_ID);

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

            util::remove_outdated_versions(Self::LANGUAGE_SERVER_ID, otp_version, &version_dir)?;
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}
