#!/usr/bin/env bash
set -euo pipefail

# Signs the codex binary using the same flow as the CI release workflow.
# Usage: ./sign_codex.sh [path-to-binary]

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
default_p12=""
# Prefer a PKCS#12 bundle that lives next to this script, but fall back to the
# repo root where the helper scripts generate codex-test.p12 by default.
for candidate in "${script_dir}/codex-test.p12" "${script_dir}/../codex-test.p12"; do
  if [[ -f "$candidate" ]]; then
    default_p12="$candidate"
    break
  fi
done

if [[ -z "${APPLE_CERTIFICATE:-}" && -f "$default_p12" ]]; then
  export APPLE_CERTIFICATE="$(base64 -b 0 < "$default_p12")"
  export APPLE_CERTIFICATE_PASSWORD="${APPLE_CERTIFICATE_PASSWORD:-codex-local-password}"
  export APPLE_CODESIGN_IDENTITY="${APPLE_CODESIGN_IDENTITY:-Codex Local Signing}"
  export CODESIGN_TEST="${CODESIGN_TEST:-false}"
else
  export APPLE_CERTIFICATE="${APPLE_CERTIFICATE:-}"
  export APPLE_CERTIFICATE_PASSWORD="${APPLE_CERTIFICATE_PASSWORD:-}"
  export APPLE_CODESIGN_IDENTITY="${APPLE_CODESIGN_IDENTITY:-}"
  export CODESIGN_TEST="${CODESIGN_TEST:-true}"
fi

binary_path="${1:-target/debug/codex}"

if [[ ! -f "$binary_path" ]]; then
  echo "Binary not found: $binary_path" >&2
  exit 1
fi

if [[ "${CODESIGN_TEST:-}" == "true" ]]; then
  codesign --force --sign - "$binary_path"
  codesign --verify --deep --strict "$binary_path"
  exit 0
fi

missing_vars=()
[[ -z "${APPLE_CERTIFICATE:-}" ]] && missing_vars+=(APPLE_CERTIFICATE)
[[ -z "${APPLE_CERTIFICATE_PASSWORD:-}" ]] && missing_vars+=(APPLE_CERTIFICATE_PASSWORD)
[[ -z "${APPLE_CODESIGN_IDENTITY:-}" ]] && missing_vars+=(APPLE_CODESIGN_IDENTITY)
if (( ${#missing_vars[@]} > 0 )); then
  echo "Missing required environment variables: ${missing_vars[*]}" >&2
  exit 1
fi

keychain_password="${KEYCHAIN_PASSWORD:-actions}"

original_keychains=()
while IFS= read -r keychain; do
  keychain=$(echo "$keychain" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//;s/"//g')
  [[ -n "$keychain" ]] && original_keychains+=("$keychain")
done < <(security list-keychains)

tmpdir="$(mktemp -d)"
keychain_path="$tmpdir/codex-signing.keychain-db"
cert_path="$tmpdir/apple_signing_certificate.p12"

cleanup() {
  rm -f "$cert_path"
  if [[ -f "$keychain_path" ]]; then
    security delete-keychain "$keychain_path" >/dev/null 2>&1 || true
  fi
  if (( ${#original_keychains[@]} > 0 )); then
    security list-keychains -s "${original_keychains[@]}" >/dev/null 2>&1 || true
    security default-keychain -s "${original_keychains[0]}" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmpdir"
}
trap cleanup EXIT

printf '%s' "$APPLE_CERTIFICATE" | base64 -d > "$cert_path"

security create-keychain -p "$keychain_password" "$keychain_path"
security set-keychain-settings -lut 21600 "$keychain_path"
security unlock-keychain -p "$keychain_password" "$keychain_path"
if (( ${#original_keychains[@]} > 0 )); then
  security list-keychains -s "$keychain_path" "${original_keychains[@]}"
else
  security list-keychains -s "$keychain_path"
fi
security default-keychain -s "$keychain_path"
# `security import` needs the bundle format explicitly or it may fail when fed
# via stdin/base64.
security import "$cert_path" -f pkcs12 -k "$keychain_path" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security
security set-key-partition-list -S apple-tool:,apple: -s -k "$keychain_password" "$keychain_path"

security find-identity -v -p codesigning "$keychain_path" || true

codesign --force --options runtime --timestamp --keychain "$keychain_path" --sign "$APPLE_CODESIGN_IDENTITY" "$binary_path"
codesign --verify --deep --strict "$binary_path"
