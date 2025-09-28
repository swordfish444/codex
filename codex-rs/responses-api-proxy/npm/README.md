# @openai/codex-responses-api-proxy

This package distributes the prebuilt Codex responses API proxy binary for macOS, Linux, and Windows.

The binary originates from the `codex-responses-api-proxy` crate in this repository and is intended for internal automation. When installed, the launcher script in `bin/codex-responses-api-proxy.js` selects the correct native executable from the `vendor/` directory for the current platform.

To see available options, run:

```
node ./bin/codex-responses-api-proxy.js --help
```

Refer to `codex-rs/responses-api-proxy/README.md` for additional documentation.
