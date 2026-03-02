# Zed Erlang

An [Erlang](https://www.erlang.org/) extension for [Zed](https://zed.dev).

## Development

To develop this extension, see the [Developing Extensions](https://zed.dev/docs/extensions/developing-extensions) section of the Zed docs.

## Erlang/OTP version configuration option

By default, the extension will download binaries corresponding to the latest Erlang/OTP version supported by the language server. You can specify which version to use instead via the `otp_version` option:

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

**NOTE:** This option _will not_ work for `erlang-ls` on Windows; the only binaries supplied for this platform are for `Erlang/OTP 26.2.5.3`.
