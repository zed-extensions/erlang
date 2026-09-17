# Zed Erlang

An [Erlang](https://www.erlang.org/) extension for [Zed](https://zed.dev).

## Debugging with EDB

The extension supports [EDB](https://github.com/WhatsApp/edb) when the `erl` executable on Zed's project PATH is Erlang/OTP 29 or newer. It works independently of the selected Erlang language server.

The extension checks the active OTP version before every debug session. The ELP `settings.otp_version` option only chooses an ELP binary and does not change the OTP installation used by EDB.

EDB requires debug information in the modules you want to inspect. For a Rebar3 project, add the following test profile to `rebar.config`:

```erlang
{profiles, [
    {test, [
        {erl_opts, [debug_info, beam_debug_info, beam_debug_stack]}
    ]}
]}.
```

The Rebar3 shell, EUnit, and Common Test tasks can be launched through Zed's debugger. They are automatically run in the `test` profile so they include the required debug information.

### Debugging a Rebar3 project

Once the `test` profile above is in place, your project is ready to debug. There are no source files, startup hooks, or extra dependencies to add.

Use Zed's **Debug** action on a Rebar3 shell, EUnit test, or Common Test runnable. The extension compiles the project with the `test` profile, connects the debugger, and starts the selected test automatically. Set a regular breakpoint in your Erlang code and inspect local variables when it pauses.

### Custom launch configuration

Most projects do not need this section. Use `.zed/debug.json` only when you want a named debug scenario, a custom startup command, or an application entry point.

The following macOS/Linux configuration opens a debug-enabled Rebar3 shell. Copy the debugger-related values exactly as shown; Zed fills them in when the session starts.

```jsonc
[
  {
    "adapter": "erlang-edb",
    "label": "Launch Rebar3 shell",
    "request": "launch",
    "runInTerminal": {
      "kind": "integrated",
      "cwd": "$ZED_WORKTREE_ROOT",
      "args": [
        "sh",
        "-c",
        "exec \"$0\" \"$@\" --eval=\"$EDB_DAP_NODE_INIT\"",
        "rebar3",
        "as",
        "test",
        "shell"
      ]
    },
    "config": {
      "nameDomain": "shortnames",
      "nodeInitCodeInEnvVar": "EDB_DAP_NODE_INIT",
      "timeout": 60
    }
  }
]
```

To start your OTP application automatically, add it to the launch command:

```jsonc
"exec \"$0\" \"$@\" --eval=\"$EDB_DAP_NODE_INIT, application:ensure_all_started(my_app).\""
```

Replace `my_app` with your OTP application name. Keep the rest of the command unchanged so the debugger connects before your application starts. For EUnit and Common Test, prefer Zed's generated debug scenarios because they already start the correct Rebar3 provider.

EDB does not currently advertise logpoint support, so use a regular breakpoint when inspecting variables.

### Attaching to an existing node

To attach to an existing node, start that node with the `+D` emulator flag and use:

```jsonc
[
  {
    "adapter": "erlang-edb",
    "label": "Attach to Erlang node",
    "request": "attach",
    "config": {
      "node": "devel@localhost",
      "cookie": "my-cookie",
      "cwd": "$ZED_WORKTREE_ROOT"
    }
  }
]
```

The extension uses an `edb` executable from the project PATH when available. Otherwise, it downloads a pinned, OTP 29-compatible EDB source revision and builds the escript once with Rebar3. Later debug sessions reuse that cached build. You can override the executable with Zed's `dap.erlang-edb.binary` setting.

## Development

To develop this extension, see the [Developing Extensions](https://zed.dev/docs/extensions/developing-extensions) section of the Zed docs.

## Erlang/OTP version configuration option

By default, the extension downloads a language server binary for the latest supported Erlang/OTP version. The `settings.otp_version` option only selects which OTP-targeted language server binary the extension downloads; it does not configure the OTP installation used by your project or by a custom command.

```jsonc
  // Example for `erlang-ls`
  "lsp": {
    "erlang-ls": {
      "settings": {
        "otp_version": "25"
      }
    }
  }
```

```jsonc
  // Example for `elp`
  "lsp": {
    "elp": {
      "settings": {
        "otp_version": "26.2"
      }
    }
  }
```

**NOTE:** On Windows, the automatic download for `erlang-ls` currently only provides a binary for `Erlang/OTP 26.2.5.3`, so `settings.otp_version` cannot select another version.

### Using a custom `erlang-ls` command

When the automatically selected binary does not match the OTP version you need, especially on Windows when that version is unavailable through automatic downloads, use Zed's generic binary override for the existing `erlang-ls` language server:

```jsonc
{
  "lsp": {
    "erlang-ls": {
      "binary": {
        "path": "C:/Program Files/Erlang OTP/erl-23/bin/escript.exe",
        "arguments": [
          "C:/path/to/compatible/erlang_ls",
          "--transport",
          "stdio"
        ]
      }
    }
  }
}
```

The `binary.path` and `binary.arguments` values override the language server command and arguments. You are responsible for installing or building an `erlang_ls` escript that is compatible with your project's OTP version.
