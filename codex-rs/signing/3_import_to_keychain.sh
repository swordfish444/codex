#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
p12_path="${script_dir}/codex-test.p12"

if [[ ! -f "$p12_path" ]]; then
  echo "PKCS#12 bundle not found: $p12_path" >&2
  exit 1
fi

# Explicitly specify PKCS#12 to avoid "Unknown format" errors on import.
security import "$p12_path" \
  -f pkcs12 \
  -k ~/Library/Keychains/login.keychain-db \
  -P codex-local-password \
  -T /usr/bin/codesign \
  -T /usr/bin/security
