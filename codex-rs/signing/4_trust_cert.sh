#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
default_cert="${script_dir}/codex-test.crt"
cert_path="${1:-${default_cert}}"
if [[ ! -f "$cert_path" ]]; then
  echo "Certificate not found: $cert_path" >&2
  exit 1
fi

# macOS expects the camelCase "codeSign" policy name here.
security add-trusted-cert \
  -d \
  -r trustRoot \
  -p codeSign \
  -k ~/Library/Keychains/login.keychain-db \
  "$cert_path"

# Confirm macOS sees the entry
# `security find-identity` expects the lowercase policy name.
security find-identity -v -p codesigning ~/Library/Keychains/login.keychain-db
