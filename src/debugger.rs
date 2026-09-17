use std::{env, fs, path::Path};

use zed_extension_api::{
    self as zed, DebugAdapterBinary, DebugConfig, DebugRequest, DebugScenario, DebugTaskDefinition,
    Result, StartDebuggingRequestArguments, StartDebuggingRequestArgumentsRequest, TaskTemplate,
    Worktree,
    serde_json::{self, Map, Value, json},
};

struct CachedEdbBinary {
    otp_version: String,
    path: String,
}

pub struct Edb {
    cached_binary: Option<CachedEdbBinary>,
}

impl Edb {
    pub const ADAPTER_NAME: &'static str = "erlang-edb";
    pub const LOCATOR_NAME: &'static str = "erlang";

    const BINARY_NAME: &'static str = "edb";
    const EDB_COMMIT: &'static str = "301739580fd2f768235a13599b6f9aa8c58acb05";
    const EDB_REPOSITORY: &'static str = "WhatsApp/edb";
    const MINIMUM_OTP_MAJOR: u32 = 29;
    const NODE_INIT_ENV_VAR: &'static str = "EDB_DAP_NODE_INIT";
    const TEST_PROVIDER_ENV_VAR: &'static str = "ZED_ERLANG_TEST_PROVIDER";
    const TEST_ARGUMENTS_ENV_VAR: &'static str = "ZED_ERLANG_TEST_ARGUMENTS";
    const OTP_RELEASE_EXPRESSION: &'static str =
        r#"io:format("~s", [erlang:system_info(otp_release)]), halt()."#;
    const EVAL_NODE_INIT_EXPRESSION: &'static str = concat!(
        "EdbNodeInit = os:getenv(\"EDB_DAP_NODE_INIT\"), ",
        "{ok, EdbNodeInitTokens, _} = erl_scan:string(EdbNodeInit ++ \".\"), ",
        "{ok, EdbNodeInitExpressions} = erl_parse:parse_exprs(EdbNodeInitTokens), ",
        "{value, _, _} = erl_eval:exprs(EdbNodeInitExpressions, erl_eval:new_bindings())"
    );
    const RUN_TEST_EXPRESSION: &'static str = concat!(
        "Agent = self(), ",
        "Provider = list_to_atom(os:getenv(\"ZED_ERLANG_TEST_PROVIDER\")), ",
        "ProviderArguments = os:getenv(\"ZED_ERLANG_TEST_ARGUMENTS\"), ",
        "spawn(fun() -> ",
        "catch gen_server:call(Agent, {cmd, default, Provider, ProviderArguments}, infinity), ",
        "erlang:halt(0) end)"
    );

    pub fn new() -> Self {
        Self {
            cached_binary: None,
        }
    }

    pub fn get_dap_binary(
        &mut self,
        adapter_name: &str,
        config: DebugTaskDefinition,
        user_provided_debug_adapter_path: Option<String>,
        worktree: &Worktree,
    ) -> Result<DebugAdapterBinary> {
        Self::ensure_adapter(adapter_name)?;
        let otp_version = Self::ensure_debugger_requirements(worktree)?;

        let configuration: Value = serde_json::from_str(&config.config)
            .map_err(|error| format!("invalid EDB configuration: {error}"))?;
        let request = Self::dap_request_kind(adapter_name, &configuration)?;
        let escript = worktree.which("escript").ok_or_else(|| {
            "EDB requires `escript` from an Erlang/OTP 29 or newer installation on PATH".to_string()
        })?;
        let edb =
            self.debug_adapter_binary(user_provided_debug_adapter_path, &otp_version, worktree)?;

        Ok(DebugAdapterBinary {
            command: Some(escript),
            arguments: vec![edb, "dap".to_string()],
            envs: worktree.shell_env(),
            cwd: Some(worktree.root_path()),
            connection: None,
            request_args: StartDebuggingRequestArguments {
                configuration: config.config,
                request,
            },
        })
    }

    pub fn dap_request_kind(
        adapter_name: &str,
        config: &Value,
    ) -> Result<StartDebuggingRequestArgumentsRequest> {
        Self::ensure_adapter(adapter_name)?;

        match config.get("request").and_then(Value::as_str) {
            Some("launch") => Ok(StartDebuggingRequestArgumentsRequest::Launch),
            Some("attach") => Ok(StartDebuggingRequestArgumentsRequest::Attach),
            Some(request) => Err(format!(
                "unsupported EDB request {request:?}; expected `launch` or `attach`"
            )),
            None => Err("missing required `request` field in EDB configuration".to_string()),
        }
    }

    pub fn dap_config_to_scenario(config: DebugConfig) -> Result<DebugScenario> {
        Self::ensure_adapter(&config.adapter)?;

        let adapter_config = match config.request {
            DebugRequest::Launch(launch) => {
                if launch.program.is_empty() {
                    return Err("select a command to launch with EDB".to_string());
                }

                let program = launch.program;
                let program_arguments = launch.args;
                let mut environment = launch
                    .envs
                    .into_iter()
                    .map(|(key, value)| (key, Value::String(value)))
                    .collect::<Map<_, _>>();
                let rebar3_arguments = if Self::is_rebar3_command(&program) {
                    Self::rebar3_debug_command(&program, &program_arguments, &mut environment)
                } else {
                    None
                };
                let uses_node_init_env = rebar3_arguments.is_some();
                let arguments = rebar3_arguments.unwrap_or_else(|| {
                    let mut arguments = vec![program];
                    arguments.extend(program_arguments);
                    arguments
                });
                let edb_config = Self::edb_launch_config(uses_node_init_env);

                json!({
                    "request": "launch",
                    "runInTerminal": {
                        "kind": "integrated",
                        "cwd": launch.cwd.unwrap_or_else(|| "$ZED_WORKTREE_ROOT".to_string()),
                        "args": arguments,
                        "env": environment,
                    },
                    "config": edb_config,
                })
            }
            DebugRequest::Attach(_) => {
                return Err(
                    "EDB attach configurations require a node name; add one to `.zed/debug.json`"
                        .to_string(),
                );
            }
        };

        Ok(DebugScenario {
            label: config.label,
            adapter: config.adapter,
            build: None,
            config: adapter_config.to_string(),
            tcp_connection: None,
        })
    }

    pub fn dap_locator_create_scenario(
        locator_name: &str,
        task: TaskTemplate,
        resolved_label: String,
        debug_adapter_name: &str,
    ) -> Option<DebugScenario> {
        if locator_name != Self::LOCATOR_NAME
            || debug_adapter_name != Self::ADAPTER_NAME
            || !Self::is_debuggable_rebar3_task(&task)
        {
            return None;
        }

        let mut environment = task
            .env
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect::<Map<_, _>>();
        let arguments = Self::rebar3_debug_command(&task.command, &task.args, &mut environment)?;
        let cwd = task.cwd.unwrap_or_else(|| "$ZED_WORKTREE_ROOT".to_string());
        let adapter_config = json!({
            "request": "launch",
            "runInTerminal": {
                "kind": "integrated",
                "title": resolved_label,
                "cwd": cwd,
                "args": arguments,
                "env": environment,
            },
            "config": Self::edb_launch_config(true),
        });

        Some(DebugScenario {
            label: resolved_label,
            adapter: debug_adapter_name.to_string(),
            build: None,
            config: adapter_config.to_string(),
            tcp_connection: None,
        })
    }

    fn ensure_adapter(adapter_name: &str) -> Result<()> {
        if adapter_name == Self::ADAPTER_NAME {
            Ok(())
        } else {
            Err(format!("unknown debug adapter: {adapter_name}"))
        }
    }

    fn ensure_debugger_requirements(worktree: &Worktree) -> Result<String> {
        if worktree.which("erl").is_none() {
            return Err(
                "EDB requires Erlang/OTP 29 or newer, but `erl` was not found on PATH".to_string(),
            );
        }

        let output = zed::process::Command::new("erl")
            .args(["-noshell", "-eval", Self::OTP_RELEASE_EXPRESSION])
            .envs(worktree.shell_env())
            .output()
            .map_err(|error| format!("failed to detect the Erlang/OTP version: {error}"))?;

        if output.status != Some(0) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "failed to detect the Erlang/OTP version with `erl`: {}",
                stderr.trim()
            ));
        }

        let release = String::from_utf8(output.stdout)
            .map_err(|error| format!("`erl` returned an invalid OTP version: {error}"))?;
        let major = Self::otp_major_version(&release)
            .ok_or_else(|| format!("could not parse Erlang/OTP release {release:?}"))?;

        if major < Self::MINIMUM_OTP_MAJOR {
            return Err(format!(
                "EDB requires Erlang/OTP {} or newer; the active `erl` reports OTP {}",
                Self::MINIMUM_OTP_MAJOR,
                release.trim()
            ));
        }

        Ok(major.to_string())
    }

    fn otp_major_version(release: &str) -> Option<u32> {
        release.trim().split('.').next()?.parse().ok()
    }

    fn is_rebar3_command(command: &str) -> bool {
        Path::new(command)
            .file_stem()
            .and_then(|name| name.to_str())
            == Some("rebar3")
    }

    fn is_debuggable_rebar3_task(task: &TaskTemplate) -> bool {
        Self::is_rebar3_command(&task.command)
            && Self::rebar3_provider(&task.args)
                .is_some_and(|(provider, _)| matches!(provider, "shell" | "eunit" | "ct"))
    }

    fn rebar3_provider(arguments: &[String]) -> Option<(&str, &[String])> {
        if arguments.first().is_some_and(|argument| argument == "as") {
            Some((arguments.get(2)?.as_str(), arguments.get(3..)?))
        } else {
            Some((arguments.first()?.as_str(), arguments.get(1..)?))
        }
    }

    fn rebar3_debug_command(
        command: &str,
        arguments: &[String],
        environment: &mut Map<String, Value>,
    ) -> Option<Vec<String>> {
        let (provider, provider_arguments) = Self::rebar3_provider(arguments)?;
        let mut launch_arguments = vec![
            command.to_string(),
            "as".to_string(),
            "test".to_string(),
            "shell".to_string(),
        ];

        match provider {
            "shell" => {
                launch_arguments.push(format!("--eval={}", Self::EVAL_NODE_INIT_EXPRESSION));
                launch_arguments.extend(provider_arguments.iter().cloned());
            }
            "eunit" | "ct" => {
                environment.insert(
                    Self::TEST_PROVIDER_ENV_VAR.to_string(),
                    Value::String(provider.to_string()),
                );
                environment.insert(
                    Self::TEST_ARGUMENTS_ENV_VAR.to_string(),
                    Value::String(provider_arguments.join(" ")),
                );
                launch_arguments.push(format!(
                    "--eval={}, {}",
                    Self::EVAL_NODE_INIT_EXPRESSION,
                    Self::RUN_TEST_EXPRESSION
                ));
            }
            _ => return None,
        }

        Some(launch_arguments)
    }

    fn edb_launch_config(uses_node_init_env: bool) -> Value {
        if uses_node_init_env {
            json!({
                "nameDomain": "shortnames",
                "nodeInitCodeInEnvVar": Self::NODE_INIT_ENV_VAR,
                "timeout": 60,
            })
        } else {
            json!({
                "nameDomain": "shortnames",
            })
        }
    }

    fn debug_adapter_binary(
        &mut self,
        user_provided_debug_adapter_path: Option<String>,
        otp_version: &str,
        worktree: &Worktree,
    ) -> Result<String> {
        if let Some(path) = user_provided_debug_adapter_path.filter(|path| !path.is_empty()) {
            return Ok(path);
        }

        if let Some(path) = worktree.which(Self::BINARY_NAME) {
            return Ok(path);
        }

        if let Some(cached_binary) = &self.cached_binary
            && cached_binary.otp_version == otp_version
            && fs::metadata(&cached_binary.path).is_ok_and(|metadata| metadata.is_file())
        {
            return Ok(cached_binary.path.clone());
        }

        self.download_and_build_edb(otp_version, worktree)
    }

    fn download_and_build_edb(&mut self, otp_version: &str, worktree: &Worktree) -> Result<String> {
        let extension_dir = env::current_dir()
            .map_err(|error| format!("failed to resolve the extension directory: {error}"))?;
        let version_dir = format!("edb-src-{}-otp-{otp_version}", Self::EDB_COMMIT);
        let source_dir = format!("{version_dir}/edb-{}", Self::EDB_COMMIT);
        let relative_binary_path = format!("{source_dir}/_build/default/bin/{}", Self::BINARY_NAME);
        let source_path = extension_dir.join(&source_dir);
        let binary_path = extension_dir.join(&relative_binary_path);

        if !binary_path.is_file() {
            if !source_path.is_dir() {
                let archive_url = format!(
                    "https://github.com/{}/archive/{}.zip",
                    Self::EDB_REPOSITORY,
                    Self::EDB_COMMIT
                );
                zed::download_file(&archive_url, &version_dir, zed::DownloadedFileType::Zip)
                    .map_err(|error| format!("failed to download the EDB source: {error}"))?;
            }

            if worktree.which("rebar3").is_none() {
                return Err(
                    "building the OTP 29-compatible EDB adapter requires `rebar3` on PATH"
                        .to_string(),
                );
            }
            if worktree.which("env").is_none() {
                return Err(
                    "building the EDB adapter requires an `env` command with `-C` support; install EDB manually and set `dap.erlang-edb.binary`"
                        .to_string(),
                );
            }

            let source_path = source_path.to_string_lossy().to_string();
            let output = zed::process::Command::new("env")
                .args(["-C", &source_path, "rebar3", "escriptize"])
                .envs(worktree.shell_env())
                .output()
                .map_err(|error| format!("failed to build EDB with Rebar3: {error}"))?;

            if output.status != Some(0) {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!(
                    "failed to build EDB with Rebar3: {}",
                    stderr.trim()
                ));
            }
            if !binary_path.is_file() {
                return Err(format!(
                    "Rebar3 completed without creating the EDB adapter at {}",
                    binary_path.display()
                ));
            }

            zed::make_file_executable(&relative_binary_path)?;
            Self::remove_outdated_versions(otp_version, &version_dir)?;
        }

        let path = binary_path.to_string_lossy().to_string();
        self.cached_binary = Some(CachedEdbBinary {
            otp_version: otp_version.to_string(),
            path: path.clone(),
        });
        Ok(path)
    }

    fn remove_outdated_versions(otp_version: &str, current_version_dir: &str) -> Result<()> {
        let entries = fs::read_dir(".")
            .map_err(|error| format!("failed to list the extension working directory: {error}"))?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                format!("failed to read an extension working directory entry: {error}")
            })?;
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };

            if (file_name.starts_with("edb-src-") || file_name.starts_with("edb-elp-v"))
                && file_name.ends_with(&format!("-otp-{otp_version}"))
                && file_name != current_version_dir
            {
                fs::remove_dir_all(entry.path()).map_err(|error| {
                    format!("failed to remove outdated EDB version {file_name}: {error}")
                })?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_otp_major_versions() {
        assert_eq!(Edb::otp_major_version("29"), Some(29));
        assert_eq!(Edb::otp_major_version(" 29.1 \n"), Some(29));
        assert_eq!(Edb::otp_major_version("28.5"), Some(28));
        assert_eq!(Edb::otp_major_version("OTP 29"), None);
    }

    #[test]
    fn determines_dap_request_kind() {
        assert_eq!(
            Edb::dap_request_kind(Edb::ADAPTER_NAME, &json!({ "request": "launch" })),
            Ok(StartDebuggingRequestArgumentsRequest::Launch)
        );
        assert_eq!(
            Edb::dap_request_kind(Edb::ADAPTER_NAME, &json!({ "request": "attach" })),
            Ok(StartDebuggingRequestArgumentsRequest::Attach)
        );
        assert!(Edb::dap_request_kind(Edb::ADAPTER_NAME, &json!({})).is_err());
    }

    #[test]
    fn creates_debug_scenarios_for_rebar3_tasks() -> Result<()> {
        let scenario = Edb::dap_locator_create_scenario(
            Edb::LOCATOR_NAME,
            TaskTemplate {
                label: "Rebar3: Shell".to_string(),
                command: "rebar3".to_string(),
                args: vec!["shell".to_string()],
                env: vec![],
                cwd: None,
            },
            "Rebar3: Shell".to_string(),
            Edb::ADAPTER_NAME,
        )
        .ok_or_else(|| "rebar3 tasks should produce EDB scenarios".to_string())?;

        let config: Value = serde_json::from_str(&scenario.config)
            .map_err(|error| format!("invalid generated EDB configuration: {error}"))?;
        assert_eq!(config["request"], "launch");
        let shell_arguments = config["runInTerminal"]["args"]
            .as_array()
            .ok_or_else(|| "generated shell arguments should be an array".to_string())?;
        let expected_shell_prefix = json!(["rebar3", "as", "test", "shell"]);
        assert_eq!(
            &shell_arguments[..4],
            expected_shell_prefix
                .as_array()
                .ok_or_else(|| "expected shell prefix should be an array".to_string())?
        );
        assert_eq!(
            shell_arguments[4],
            format!("--eval={}", Edb::EVAL_NODE_INIT_EXPRESSION)
        );
        assert_eq!(config["config"]["nameDomain"], "shortnames");
        assert_eq!(
            config["config"]["nodeInitCodeInEnvVar"],
            Edb::NODE_INIT_ENV_VAR
        );

        let eunit_scenario = Edb::dap_locator_create_scenario(
            Edb::LOCATOR_NAME,
            TaskTemplate {
                label: "EUnit: Run all tests".to_string(),
                command: "rebar3".to_string(),
                args: vec!["eunit".to_string()],
                env: vec![],
                cwd: None,
            },
            "EUnit: Run all tests".to_string(),
            Edb::ADAPTER_NAME,
        )
        .ok_or_else(|| "EUnit tasks should produce EDB scenarios".to_string())?;
        let eunit_config: Value = serde_json::from_str(&eunit_scenario.config)
            .map_err(|error| format!("invalid generated EDB configuration: {error}"))?;
        let eunit_arguments = eunit_config["runInTerminal"]["args"]
            .as_array()
            .ok_or_else(|| "generated EUnit arguments should be an array".to_string())?;
        assert_eq!(
            &eunit_arguments[..4],
            expected_shell_prefix
                .as_array()
                .ok_or_else(|| "expected shell prefix should be an array".to_string())?
        );
        assert_eq!(
            eunit_config["runInTerminal"]["env"][Edb::TEST_PROVIDER_ENV_VAR],
            "eunit"
        );
        assert_eq!(eunit_arguments.len(), 5);
        assert_eq!(
            eunit_arguments[4],
            format!(
                "--eval={}, {}",
                Edb::EVAL_NODE_INIT_EXPRESSION,
                Edb::RUN_TEST_EXPRESSION
            )
        );
        Ok(())
    }

    #[test]
    fn ignores_non_rebar3_tasks() {
        let scenario = Edb::dap_locator_create_scenario(
            Edb::LOCATOR_NAME,
            TaskTemplate {
                label: "Other".to_string(),
                command: "erl".to_string(),
                args: vec![],
                env: vec![],
                cwd: None,
            },
            "Other".to_string(),
            Edb::ADAPTER_NAME,
        );

        assert!(scenario.is_none());

        let compile_scenario = Edb::dap_locator_create_scenario(
            Edb::LOCATOR_NAME,
            TaskTemplate {
                label: "Rebar3: Compile".to_string(),
                command: "rebar3".to_string(),
                args: vec!["compile".to_string()],
                env: vec![],
                cwd: None,
            },
            "Rebar3: Compile".to_string(),
            Edb::ADAPTER_NAME,
        );

        assert!(compile_scenario.is_none());
    }
}
