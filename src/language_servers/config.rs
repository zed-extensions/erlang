use zed_extension_api::{Worktree, serde_json::Value, settings::LspSettings};

pub(super) fn get_lsp_settings(
    language_server_id: &'static str,
    worktree: &Worktree,
) -> Option<Value> {
    LspSettings::for_worktree(language_server_id, worktree)
        .ok()
        .and_then(|lsp_settings| lsp_settings.settings)
}

pub(super) fn get_otp_version(lsp_settings: &Option<Value>) -> Option<String> {
    if let Some(otp_version) = lsp_settings {
        otp_version
            .pointer("/otp_version")?
            .as_str()
            .map(|x| x.to_string())
    } else {
        None
    }
}
