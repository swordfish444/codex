#!/usr/bin/env python3
"""Install Codex native binaries (Rust CLI plus ripgrep helpers)."""

from __future__ import annotations

import argparse
import importlib.util
import os
import shutil
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Iterable

SCRIPT_DIR = Path(__file__).resolve().parent
CODEX_CLI_ROOT = SCRIPT_DIR.parent
DEFAULT_WORKFLOW_URL = "https://github.com/openai/codex/actions/runs/17952349351"  # rust-v0.40.0
VENDOR_DIR_NAME = "vendor"
CODEX_TARGETS = (
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
)


def _load_rg_utils():
    spec = importlib.util.spec_from_file_location("rg_utils", SCRIPT_DIR / "rg_utils.py")
    if spec is None or spec.loader is None:
        raise RuntimeError("Unable to load rg_utils module")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


rg_utils = _load_rg_utils()
fetch_rg = rg_utils.fetch_rg
extract_archive = rg_utils.extract_archive
DEFAULT_RG_TARGETS = rg_utils.DEFAULT_RG_TARGETS


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Install native Codex binaries.")
    parser.add_argument(
        "--workflow-url",
        help=(
            "GitHub Actions workflow URL that produced the artifacts. Defaults to a "
            "known good run when omitted."
        ),
    )
    parser.add_argument(
        "root",
        nargs="?",
        type=Path,
        help=(
            "Directory containing package.json for the staged package. If omitted, the "
            "repository checkout is used."
        ),
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    codex_cli_root = (args.root or CODEX_CLI_ROOT).resolve()
    vendor_dir = codex_cli_root / VENDOR_DIR_NAME
    vendor_dir.mkdir(parents=True, exist_ok=True)

    workflow_url = (args.workflow_url or DEFAULT_WORKFLOW_URL).strip()
    if not workflow_url:
        workflow_url = DEFAULT_WORKFLOW_URL

    _prepare_vendor_layout(vendor_dir, CODEX_TARGETS)

    workflow_id = workflow_url.rstrip("/").split("/")[-1]

    with tempfile.TemporaryDirectory(prefix="codex-native-artifacts-") as artifacts_dir_str:
        artifacts_dir = Path(artifacts_dir_str)
        _download_artifacts(workflow_id, artifacts_dir)
        install_codex_binaries(artifacts_dir, vendor_dir, CODEX_TARGETS)

    fetch_rg(vendor_dir, DEFAULT_RG_TARGETS)

    print(f"Installed native dependencies into {vendor_dir}")
    return 0


def _prepare_vendor_layout(vendor_dir: Path, targets: Iterable[str]) -> None:
    # Clear legacy file layout living directly in bin/.
    bin_dir = vendor_dir.parent / "bin"
    for legacy in bin_dir.glob("codex-*"):
        if legacy.is_file():
            try:
                legacy.unlink()
            except OSError:
                pass
    for legacy in bin_dir.glob("rg-*"):
        if legacy.is_file():
            try:
                legacy.unlink()
            except OSError:
                pass
    for target in targets:
        shutil.rmtree(bin_dir / target, ignore_errors=True)

    shutil.rmtree(vendor_dir, ignore_errors=True)
    vendor_dir.mkdir(parents=True, exist_ok=True)


def _download_artifacts(workflow_id: str, dest_dir: Path) -> None:
    cmd = [
        "gh",
        "run",
        "download",
        "--dir",
        str(dest_dir),
        "--repo",
        "openai/codex",
        workflow_id,
    ]
    subprocess.run(cmd, check=True)


def install_codex_binaries(
    artifacts_dir: Path, vendor_dir: Path, targets: Iterable[str]
) -> list[Path]:
    targets = list(targets)
    if not targets:
        return []

    results: dict[str, Path] = {}
    max_workers = min(len(targets), max(1, (os.cpu_count() or 1)))

    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        future_map = {
            executor.submit(
                _install_single_codex_binary, artifacts_dir, vendor_dir, target
            ): target
            for target in targets
        }

        for future in as_completed(future_map):
            target = future_map[future]
            results[target] = future.result()

    return [results[target] for target in targets]


def _install_single_codex_binary(
    artifacts_dir: Path, vendor_dir: Path, target: str
) -> Path:
    artifact_subdir = artifacts_dir / target
    archive_name = _archive_name_for_target(target)
    archive_path = artifact_subdir / archive_name
    if not archive_path.exists():
        raise FileNotFoundError(f"Expected artifact not found: {archive_path}")

    dest_dir = vendor_dir / target / "codex"
    dest_dir.mkdir(parents=True, exist_ok=True)

    binary_name = "codex.exe" if "windows" in target else "codex"
    dest = dest_dir / binary_name
    dest.unlink(missing_ok=True)
    extract_archive(archive_path, "zst", None, dest)
    if "windows" not in target:
        dest.chmod(0o755)
    return dest


def _archive_name_for_target(target: str) -> str:
    if "windows" in target:
        return f"codex-{target}.exe.zst"
    return f"codex-{target}.zst"


if __name__ == "__main__":
    import sys

    sys.exit(main())
