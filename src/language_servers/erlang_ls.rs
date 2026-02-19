use std::fs;

use zed_extension_api::{self as zed, LanguageServerId, Result, Worktree};

use crate::language_servers::{config, util};

struct ErlangLsBinary {
    path: String,
    args: Vec<String>,
}

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
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        let erlang_ls = self.language_server_binary(language_server_id, worktree)?;

        Ok(zed::Command {
            command: erlang_ls.path,
            args: erlang_ls.args,
            env: Default::default(),
        })
    }

    fn language_server_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<ErlangLsBinary> {
        let (platform, _arch) = zed::current_platform();
        let lsp_settings = config::get_lsp_settings(Self::LANGUAGE_SERVER_ID, worktree);
        let otp_version = match platform {
            zed::Os::Mac | zed::Os::Linux => {
                config::get_otp_version(&lsp_settings).unwrap_or("27".to_string())
            }
            zed::Os::Windows => "26.2.5.3".to_string(),
        };

        let binary_name = Self::LANGUAGE_SERVER_ID.replace("-", "_");
        let binary_settings = config::get_binary_settings(Self::LANGUAGE_SERVER_ID, worktree);
        let binary_args = config::get_binary_args(&binary_settings)
            .unwrap_or_else(|| vec!["--transport".to_string(), "stdio".to_string()]);

        if let Some(binary_path) = config::get_binary_path(&binary_settings) {
            return Ok(ErlangLsBinary {
                path: binary_path,
                args: binary_args,
            });
        }

        if let Some(binary_path) = worktree.which(&binary_name) {
            return Ok(ErlangLsBinary {
                path: binary_path,
                args: binary_args,
            });
        }

        if let Some(binary_path) = &self.cached_binary_path
            && fs::metadata(binary_path).is_ok_and(|stat| stat.is_file())
            && binary_path.ends_with(&format!("otp-{otp_version}/{}", binary_name))
        {
            return Ok(ErlangLsBinary {
                path: binary_path.clone(),
                args: binary_args,
            });
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = match zed::latest_github_release(
            "erlang-ls/erlang_ls",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        ) {
            Ok(release) => release,
            Err(_) => {
                if let Some(binary_path) =
                    util::find_existing_binary(Self::LANGUAGE_SERVER_ID, &otp_version, &binary_name)
                {
                    self.cached_binary_path = Some(binary_path.clone());
                    return Ok(ErlangLsBinary {
                        path: binary_path,
                        args: binary_args,
                    });
                }
                return Err("failed to download latest github release".to_string());
            }
        };

        let asset_name = {
            let os = match platform {
                zed::Os::Mac => "macos",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "windows",
            };

            format!("{binary_name}-{os}-{otp_version}.tar.gz")
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
        let binary_path = format!("{}/{}", version_dir, binary_name);

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
        Ok(ErlangLsBinary {
            path: binary_path,
            args: binary_args,
        })
    }
}
