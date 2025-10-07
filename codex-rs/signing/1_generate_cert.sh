#!/usr/bin/env bash
set -euo pipefail

# Create a 2048-bit RSA key + self-signed certificate valid 10 years.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
key_path="${script_dir}/codex-test.key"
cert_path="${script_dir}/codex-test.crt"

openssl req \
  -x509 \
  -newkey rsa:2048 \
  -keyout "$key_path" \
  -out "$cert_path" \
  -days 3650 \
  -nodes \
  -subj "/CN=Codex Local Signing" \
  -addext "basicConstraints = critical,CA:false" \
  -addext "keyUsage = critical,digitalSignature" \
  -addext "extendedKeyUsage = codeSigning"
