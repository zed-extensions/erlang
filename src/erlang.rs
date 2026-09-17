mod debugger;
mod language_servers;

use zed_extension_api::{self as zed, Result, Worktree};

use crate::debugger::Edb;
use crate::language_servers::{ErlangLanguagePlatform, ErlangLs};

struct ErlangExtension {
    erlang_ls: Option<ErlangLs>,
    erlang_language_platform: Option<ErlangLanguagePlatform>,
    edb: Edb,
}

impl zed::Extension for ErlangExtension {
    fn new() -> Self {
        Self {
            erlang_ls: None,
            erlang_language_platform: None,
            edb: Edb::new(),
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        match language_server_id.as_ref() {
            ErlangLs::LANGUAGE_SERVER_ID => self
                .erlang_ls
                .get_or_insert_with(ErlangLs::new)
                .language_server_command(language_server_id, worktree),
            ErlangLanguagePlatform::LANGUAGE_SERVER_ID => self
                .erlang_language_platform
                .get_or_insert_with(ErlangLanguagePlatform::new)
                .language_server_command(language_server_id, worktree),
            language_server_id => Err(format!("unknown language server: {language_server_id}")),
        }
    }

    fn get_dap_binary(
        &mut self,
        adapter_name: String,
        config: zed::DebugTaskDefinition,
        user_provided_debug_adapter_path: Option<String>,
        worktree: &Worktree,
    ) -> Result<zed::DebugAdapterBinary> {
        self.edb.get_dap_binary(
            &adapter_name,
            config,
            user_provided_debug_adapter_path,
            worktree,
        )
    }

    fn dap_request_kind(
        &mut self,
        adapter_name: String,
        config: zed::serde_json::Value,
    ) -> Result<zed::StartDebuggingRequestArgumentsRequest> {
        Edb::dap_request_kind(&adapter_name, &config)
    }

    fn dap_config_to_scenario(&mut self, config: zed::DebugConfig) -> Result<zed::DebugScenario> {
        Edb::dap_config_to_scenario(config)
    }

    fn dap_locator_create_scenario(
        &mut self,
        locator_name: String,
        build_task: zed::TaskTemplate,
        resolved_label: String,
        debug_adapter_name: String,
    ) -> Option<zed::DebugScenario> {
        Edb::dap_locator_create_scenario(
            &locator_name,
            build_task,
            resolved_label,
            &debug_adapter_name,
        )
    }
}

zed::register_extension!(ErlangExtension);
