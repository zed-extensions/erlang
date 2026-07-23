# Zed Erlang

An [Erlang](https://www.erlang.org/) extension for [Zed](https://zed.dev).

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
