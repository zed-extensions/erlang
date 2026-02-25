use std::fs;

use zed_extension_api::{self as zed, LanguageServerId, Result, Worktree};

use crate::language_servers::{config, util};

struct ErlangLanguagePlatformBinary {
    path: String,
    args: Vec<String>,
}

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
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        let elp = self.language_server_binary(language_server_id, worktree)?;

        Ok(zed::Command {
            command: elp.path,
            args: elp.args,
            env: Default::default(),
        })
    }

    fn language_server_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<ErlangLanguagePlatformBinary> {
        let (platform, arch) = zed::current_platform();
        let configuration = config::get_workspace_configuration(Self::LANGUAGE_SERVER_ID, worktree);
        let otp_version = config::get_otp_version(&configuration).unwrap_or("28".to_string());

        let binary_settings = config::get_binary_settings(Self::LANGUAGE_SERVER_ID, worktree);
        let binary_args =
            config::get_binary_args(&binary_settings).unwrap_or_else(|| vec!["server".to_string()]);

        if let Some(binary_path) = config::get_binary_path(&binary_settings) {
            return Ok(ErlangLanguagePlatformBinary {
                path: binary_path,
                args: binary_args,
            });
        }

        if let Some(binary_path) = worktree.which(Self::LANGUAGE_SERVER_ID) {
            return Ok(ErlangLanguagePlatformBinary {
                path: binary_path,
                args: binary_args,
            });
        }

        if let Some(binary_path) = &self.cached_binary_path
            && fs::metadata(binary_path).is_ok_and(|stat| stat.is_file())
            && binary_path.ends_with(&format!("otp-{otp_version}/{}", Self::LANGUAGE_SERVER_ID))
        {
            return Ok(ErlangLanguagePlatformBinary {
                path: binary_path.clone(),
                args: binary_args,
            });
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

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
                    &otp_version,
                    Self::LANGUAGE_SERVER_ID,
                ) {
                    self.cached_binary_path = Some(binary_path.clone());
                    return Ok(ErlangLanguagePlatformBinary {
                        path: binary_path,
                        args: binary_args,
                    });
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
            &otp_version,
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

            util::remove_outdated_versions(Self::LANGUAGE_SERVER_ID, &otp_version, &version_dir)?;
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(ErlangLanguagePlatformBinary {
            path: binary_path,
            args: binary_args,
        })
    }
}
