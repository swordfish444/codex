# codex-responses-api-proxy

A minimal HTTP proxy that only forwards POST requests to `/v1/responses` to the OpenAI API, injecting the `Authorization: Bearer $OPENAI_API_KEY` header. Everything else is rejected with `403 Forbidden`.

**IMPORTANT:** This is designed to be used with `CODEX_SECURE_MODE=1` so that an unprivileged user cannot inspect or tamper with this process. Though if `--http-shutdown` is specified, an unprivileged user _can_ shutdown the server.

## Behavior

- Reads `OPENAI_API_KEY` from the environment at startup. On Unix platforms, it attempts to `mlock(2)` the memory holding the key so it is not swapped to disk.
- Immediately removes `OPENAI_API_KEY` from the process environment after reading it to avoid leaving the key in unprotected env storage. The shared `arg0_dispatch_or_else()` helper handles this before Tokio spins up.
- Listens on the provided port or an ephemeral port if `--port` is not specified.
- Accepts exactly `POST /v1/responses` (no query string). The request body is forwarded to `https://api.openai.com/v1/responses` with `Authorization: Bearer <key>` set. All original request headers (except any incoming `Authorization`) are forwarded upstream. For other requests, it responds with `403`.
- Optionally writes a single-line JSON file with startup info, currently `{ "port": <u16> }`.
- Optional `--http-shutdown` enables `GET /shutdown` to terminate the process with exit code 0. This allows one user (e.g., root) to start the proxy and another unprivileged user on the host to shut it down.

## CLI

```
responses-api-proxy [--port <PORT>] [--startup-info <FILE>] [--http-shutdown]
```

- `--port <PORT>`: Port to bind on `127.0.0.1`. If omitted, an ephemeral port is chosen.
- `--startup-info <FILE>`: If set, the proxy writes a single line of JSON with `{ "port": <PORT> }` once listening.
- `--http-shutdown`: If set, enables `GET /shutdown` to exit the process with code `0`.

## Notes

- Only `POST /v1/responses` is permitted. No query strings are allowed.
- All request headers are forwarded to the upstream call (aside from overriding `Authorization`). Response status and content-type are mirrored from upstream.
