#!/usr/bin/env python3
"""Stage and optionally package the @openai/codex npm module."""

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Sequence

from install_native_deps import CODEX_TARGETS, VENDOR_DIR_NAME

SCRIPT_DIR = Path(__file__).resolve().parent
CODEX_CLI_ROOT = SCRIPT_DIR.parent
REPO_ROOT = CODEX_CLI_ROOT.parent
GITHUB_REPO = "openai/codex"

TARGET_TO_SLICE_TAG = {
    "x86_64-unknown-linux-musl": "linux-x64",
    "aarch64-unknown-linux-musl": "linux-arm64",
    "x86_64-apple-darwin": "darwin-x64",
    "aarch64-apple-darwin": "darwin-arm64",
    "x86_64-pc-windows-msvc": "win32-x64",
    "aarch64-pc-windows-msvc": "win32-arm64",
}

_SLICE_ACCUMULATOR: dict[str, list[str]] = {}
for target in CODEX_TARGETS:
    slice_tag = TARGET_TO_SLICE_TAG.get(target)
    if slice_tag is None:
        raise RuntimeError(f"Missing slice tag mapping for target '{target}'.")
    _SLICE_ACCUMULATOR.setdefault(slice_tag, []).append(target)

SLICE_TAG_TO_TARGETS = {tag: tuple(targets) for tag, targets in _SLICE_ACCUMULATOR.items()}

DEFAULT_SLICE_TAGS = tuple(SLICE_TAG_TO_TARGETS)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build or stage the Codex CLI npm package.")
    parser.add_argument(
        "--version",
        help="Version number to write to package.json inside the staged package.",
    )
    parser.add_argument(
        "--release-version",
        help=(
            "Version to stage for npm release. When provided, the script also resolves the "
            "matching rust-release workflow unless --workflow-url is supplied."
        ),
    )
    parser.add_argument(
        "--workflow-url",
        help="Optional GitHub Actions workflow run URL used to download native binaries.",
    )
    parser.add_argument(
        "--staging-dir",
        type=Path,
        help=(
            "Directory to stage the package contents. Defaults to a new temporary directory "
            "if omitted. The directory must be empty when provided."
        ),
    )
    parser.add_argument(
        "--tmp",
        dest="staging_dir",
        type=Path,
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--pack-output",
        type=Path,
        help="Path where the generated npm tarball should be written.",
    )
    parser.add_argument(
        "--slice-pack-dir",
        type=Path,
        help=(
            "Directory where per-platform slice npm tarballs should be written. "
            "When provided, all known slices are packed unless --slices is given."
        ),
    )
    parser.add_argument(
        "--slices",
        nargs="+",
        choices=sorted(DEFAULT_SLICE_TAGS),
        help="Optional subset of slice tags to pack.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    if args.slices and args.slice_pack_dir is None:
        raise RuntimeError("--slice-pack-dir is required when specifying --slices.")

    version = args.version
    release_version = args.release_version
    if release_version:
        if version and version != release_version:
            raise RuntimeError("--version and --release-version must match when both are provided.")
        version = release_version

    if not version:
        raise RuntimeError("Must specify --version or --release-version.")

    staging_dir, created_temp = prepare_staging_dir(args.staging_dir)

    try:
        stage_sources(staging_dir, version)

        workflow_url = args.workflow_url
        resolved_head_sha: str | None = None
        if not workflow_url:
            if release_version:
                workflow = resolve_release_workflow(version)
                workflow_url = workflow["url"]
                resolved_head_sha = workflow.get("headSha")
            else:
                workflow_url = resolve_latest_alpha_workflow_url()
        elif release_version:
            try:
                workflow = resolve_release_workflow(version)
                resolved_head_sha = workflow.get("headSha")
            except Exception:
                resolved_head_sha = None

        if release_version and resolved_head_sha:
            print(f"should `git checkout {resolved_head_sha}`")

        if not workflow_url:
            raise RuntimeError("Unable to determine workflow URL for native binaries.")

        install_native_binaries(staging_dir, workflow_url)

        slice_outputs: list[tuple[str, Path]] = []
        if args.slice_pack_dir is not None:
            slice_tags = tuple(args.slices or DEFAULT_SLICE_TAGS)
            slice_outputs = build_slice_packages(
                staging_dir,
                version,
                args.slice_pack_dir,
                slice_tags,
            )

        if release_version:
            staging_dir_str = str(staging_dir)
            print(
                f"Staged version {version} for release in {staging_dir_str}\n\n"
                "Verify the CLI:\n"
                f"    node {staging_dir_str}/bin/codex.js --version\n"
                f"    node {staging_dir_str}/bin/codex.js --help\n\n"
            )
        else:
            print(f"Staged package in {staging_dir}")

        if args.pack_output is not None:
            output_path = run_npm_pack(staging_dir, args.pack_output)
            print(f"npm pack output written to {output_path}")

        for slice_tag, output_path in slice_outputs:
            print(f"built slice {slice_tag} tarball at {output_path}")
    finally:
        if created_temp:
            # Preserve the staging directory for further inspection.
            pass

    return 0


def prepare_staging_dir(staging_dir: Path | None) -> tuple[Path, bool]:
    if staging_dir is not None:
        staging_dir = staging_dir.resolve()
        staging_dir.mkdir(parents=True, exist_ok=True)
        if any(staging_dir.iterdir()):
            raise RuntimeError(f"Staging directory {staging_dir} is not empty.")
        return staging_dir, False

    temp_dir = Path(tempfile.mkdtemp(prefix="codex-npm-stage-"))
    return temp_dir, True


def stage_sources(staging_dir: Path, version: str) -> None:
    bin_dir = staging_dir / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)

    shutil.copy2(CODEX_CLI_ROOT / "bin" / "codex.js", bin_dir / "codex.js")
    rg_manifest = CODEX_CLI_ROOT / "bin" / "rg"
    if rg_manifest.exists():
        shutil.copy2(rg_manifest, bin_dir / "rg")

    readme_src = REPO_ROOT / "README.md"
    if readme_src.exists():
        shutil.copy2(readme_src, staging_dir / "README.md")

    with open(CODEX_CLI_ROOT / "package.json", "r", encoding="utf-8") as fh:
        package_json = json.load(fh)
    package_json["version"] = version

    with open(staging_dir / "package.json", "w", encoding="utf-8") as out:
        json.dump(package_json, out, indent=2)
        out.write("\n")


def install_native_binaries(staging_dir: Path, workflow_url: str | None) -> None:
    cmd = ["./scripts/install_native_deps.py"]
    if workflow_url:
        cmd.extend(["--workflow-url", workflow_url])
    cmd.append(str(staging_dir))
    subprocess.check_call(cmd, cwd=CODEX_CLI_ROOT)


def build_slice_packages(
    base_staging_dir: Path,
    version: str,
    output_dir: Path,
    slice_tags: Sequence[str],
) -> list[tuple[str, Path]]:
    if not slice_tags:
        return []

    base_vendor = base_staging_dir / VENDOR_DIR_NAME
    if not base_vendor.exists():
        raise RuntimeError(
            f"Base staging directory {base_staging_dir} does not include native vendor binaries."
        )

    output_dir = output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    results: list[tuple[str, Path]] = []
    for slice_tag in slice_tags:
        targets = SLICE_TAG_TO_TARGETS.get(slice_tag)
        if not targets:
            raise RuntimeError(f"Unknown slice tag '{slice_tag}'.")

        missing = [target for target in targets if not (base_vendor / target).exists()]
        if missing:
            missing_label = ", ".join(missing)
            raise RuntimeError(
                f"Missing native binaries for slice '{slice_tag}': {missing_label} is absent in vendor."
            )

        with tempfile.TemporaryDirectory(prefix=f"codex-npm-slice-{slice_tag}-") as slice_dir_str:
            slice_dir = Path(slice_dir_str)
            stage_sources(slice_dir, version)
            slice_vendor = slice_dir / VENDOR_DIR_NAME
            copy_vendor_slice(base_vendor, slice_vendor, targets)
            output_path = output_dir / f"codex-npm-{version}-{slice_tag}.tgz"
            run_npm_pack(slice_dir, output_path)
            results.append((slice_tag, output_path))

    return results


def copy_vendor_slice(base_vendor: Path, dest_vendor: Path, targets: Sequence[str]) -> None:
    dest_vendor.parent.mkdir(parents=True, exist_ok=True)
    dest_vendor.mkdir(parents=True, exist_ok=True)

    for entry in base_vendor.iterdir():
        if entry.is_file():
            shutil.copy2(entry, dest_vendor / entry.name)

    for target in targets:
        src = base_vendor / target
        dest = dest_vendor / target
        shutil.copytree(src, dest)


def resolve_latest_alpha_workflow_url() -> str:
    version = determine_latest_alpha_version()
    workflow_url = fetch_workflow_url_for_version(version)
    if not workflow_url:
        raise RuntimeError(f"Unable to locate workflow for version {version}.")
    return workflow_url


def determine_latest_alpha_version() -> str:
    releases = list_releases()
    best_key: tuple[int, int, int, int] | None = None
    best_version: str | None = None
    pattern = re.compile(r"^rust-v(\d+)\.(\d+)\.(\d+)-alpha\.(\d+)$")
    for release in releases:
        tag = release.get("tag_name", "")
        match = pattern.match(tag)
        if not match:
            continue
        key = tuple(int(match.group(i)) for i in range(1, 5))
        if best_key is None or key > best_key:
            best_key = key
            best_version = (
                f"{match.group(1)}.{match.group(2)}.{match.group(3)}-alpha.{match.group(4)}"
            )

    if best_version is None:
        raise RuntimeError("No alpha releases found when resolving workflow URL.")
    return best_version


def list_releases() -> list[dict]:
    stdout = subprocess.check_output(
        ["gh", "api", f"/repos/{GITHUB_REPO}/releases?per_page=100"],
        text=True,
    )
    try:
        releases = json.loads(stdout or "[]")
    except json.JSONDecodeError as exc:
        raise RuntimeError("Unable to parse releases JSON.") from exc
    if not isinstance(releases, list):
        raise RuntimeError("Unexpected response when listing releases.")
    return releases


def fetch_workflow_url_for_version(version: str) -> str | None:
    ref = f"rust-v{version}"
    stdout = subprocess.check_output(
        [
            "gh",
            "run",
            "list",
            "--branch",
            ref,
            "--limit",
            "20",
            "--json",
            "workflowName,url",
        ],
        text=True,
    )

    try:
        runs = json.loads(stdout or "[]")
    except json.JSONDecodeError as exc:
        raise RuntimeError("Unable to parse workflow run listing.") from exc

    for run in runs:
        if run.get("workflowName") == "rust-release":
            url = run.get("url")
            if url:
                return url
    return None


def resolve_release_workflow(version: str) -> dict:
    stdout = subprocess.check_output(
        [
            "gh",
            "run",
            "list",
            "--branch",
            f"rust-v{version}",
            "--json",
            "workflowName,url,headSha",
            # Empirically, we have seen both "rust-release" and
            # ".github/workflows/rust-release.yml" as the workflowName, so we
            # check for both here. The docs are not clear on which is expected:
            # https://cli.github.com/manual/gh_run_list
            "--workflow",
            ".github/workflows/rust-release.yml",
            "--jq",
            "first(.[])",
        ],
        text=True,
    )
    workflow = json.loads(stdout)
    if not workflow:
        raise RuntimeError(f"Unable to find rust-release workflow for version {version}.")
    return workflow


def run_npm_pack(staging_dir: Path, output_path: Path) -> Path:
    output_path = output_path.resolve()
    output_path.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="codex-npm-pack-") as pack_dir_str:
        pack_dir = Path(pack_dir_str)
        stdout = subprocess.check_output(
            ["npm", "pack", "--json", "--pack-destination", str(pack_dir)],
            cwd=staging_dir,
            text=True,
        )
        try:
            pack_output = json.loads(stdout)
        except json.JSONDecodeError as exc:
            raise RuntimeError("Failed to parse npm pack output.") from exc

        if not pack_output:
            raise RuntimeError("npm pack did not produce an output tarball.")

        tarball_name = pack_output[0].get("filename") or pack_output[0].get("name")
        if not tarball_name:
            raise RuntimeError("Unable to determine npm pack output filename.")

        tarball_path = pack_dir / tarball_name
        if not tarball_path.exists():
            raise RuntimeError(f"Expected npm pack output not found: {tarball_path}")

        shutil.move(str(tarball_path), output_path)

    return output_path


if __name__ == "__main__":
    import sys

    sys.exit(main())
