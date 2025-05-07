#!/usr/bin/env bash

# Install native runtime dependencies for codex-cli.
#
# By default the script copies the sandbox binaries that are required at
# runtime.  When called with the flag --rust (or --native) it additionally
# bundles pre-built Rust CLI binaries so that the resulting npm package can run
# the native implementation when users set CODEX_RUST=1.
#
# Usage
#   install_native_deps.sh [RELEASE_ROOT] [--rust]
#
# The optional RELEASE_ROOT is the path that contains package.json.  Omitting
# it installs the binaries into the repository's own bin/ folder to support
# local development.

set -euo pipefail

# ------------------
# Parse arguments
# ------------------

DEST_DIR=""
INCLUDE_RUST=0

for arg in "$@"; do
  case "$arg" in
    --native|--rust)
      INCLUDE_RUST=1
      ;;
    *)
      if [[ -z "$DEST_DIR" ]]; then
        DEST_DIR="$arg"
      else
        echo "Unexpected argument: $arg" >&2
        exit 1
      fi
      ;;
  esac
done

# Where do we copy files to?
if [[ -n "$DEST_DIR" ]]; then
  CODEX_CLI_ROOT="$DEST_DIR"
  BIN_DIR="$CODEX_CLI_ROOT/bin"
else
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  CODEX_CLI_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
  BIN_DIR="$CODEX_CLI_ROOT/bin"
fi

mkdir -p "$BIN_DIR"

# ------------------
# Copy linux-sandbox binaries
# ------------------

# Normally we would fetch these from CI.  In the sandbox we just copy the ones
# already present in the repository.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [[ -f "$REPO_ROOT/codex-cli/bin/codex-linux-sandbox-x64" ]]; then
  cp "$REPO_ROOT/codex-cli/bin/codex-linux-sandbox-x64" "$BIN_DIR/"
fi

if [[ -f "$REPO_ROOT/codex-cli/bin/codex-linux-sandbox-arm64" ]]; then
  cp "$REPO_ROOT/codex-cli/bin/codex-linux-sandbox-arm64" "$BIN_DIR/"
fi

# ------------------
# Optionally bundle Rust CLI binaries
# ------------------

if [[ "$INCLUDE_RUST" -eq 1 ]]; then
  NATIVE_DIR="$CODEX_CLI_ROOT/native"
  mkdir -p "$NATIVE_DIR"

  unpack() {
    local triple="$1"
    local archive="codex-${triple}.zst"
    local source_dir="$REPO_ROOT/${triple}"
    local src_path="$source_dir/$archive"

    if [[ ! -f "$src_path" ]]; then
      echo "Warning: $src_path not found - skipping $triple" >&2
      return
    fi

    local dest="$NATIVE_DIR/codex-${triple}"
    mkdir -p "$dest"
    cp "$src_path" "$dest/"

    if file "$dest/$archive" | grep -q "tar archive"; then
      ( cd "$dest" && tar -I zstd -xf "$archive" && rm "$archive" )
    else
      ( cd "$dest" && zstd -d "$archive" -o codex && chmod +x codex && rm "$archive" )
    fi
  }

  unpack x86_64-unknown-linux-musl
  unpack aarch64-unknown-linux-gnu
fi

echo "Installed native dependencies into $BIN_DIR"
