#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cert_path="${script_dir}/codex-test.crt"
key_path="${script_dir}/codex-test.key"
p12_path="${script_dir}/codex-test.p12"

# macOS's `security import` still expects a SHA1 MAC on PKCS#12 bundles, so
# explicitly request it to avoid "MAC verification failed" errors.
openssl pkcs12 \
  -export \
  -in "$cert_path" \
  -inkey "$key_path" \
  -out "$p12_path" \
  -name "Codex Local Signing" \
  -macalg sha1 \
  -passout pass:codex-local-password
