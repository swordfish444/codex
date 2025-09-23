#!/usr/bin/env python3

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True


def main() -> int:
    parser = argparse.ArgumentParser(
        description="""Stage an npm release for the Codex CLI.

Run this after the corresponding GitHub Release has been created and use
`--release-version` to specify the version to publish.

Optionally pass `--tmp` to control the temporary staging directory that will be
forwarded to the builder.
"""
    )
    parser.add_argument("--release-version", required=True, help="Version to release, e.g., 0.3.0")
    parser.add_argument(
        "--tmp",
        help="Optional path to stage the npm package; forwarded to the builder",
    )
    parser.add_argument(
        "--pack-output",
        help="Optional path to write the generated npm tarball.",
    )
    parser.add_argument(
        "--workflow-url",
        help=(
            "Optional GitHub Actions workflow run URL that produced the native binaries. "
            "When omitted, the script resolves the latest rust-release run for the given version."
        ),
    )
    args = parser.parse_args()
    version = args.release_version

    staging_dir, created_temp = prepare_staging_dir(args.tmp)

    workflow_url = args.workflow_url
    head_sha: str | None = None
    if not workflow_url:
        workflow = resolve_release_workflow(version)
        workflow_url = workflow["url"]
        head_sha = workflow.get("headSha")

    if head_sha:
        print(f"should `git checkout {head_sha}`")

    current_dir = Path(__file__).parent.resolve()
    cmd = [
        sys.executable,
        str(current_dir / "build_npm_package.py"),
        "--version",
        version,
    ]
    if workflow_url:
        cmd.extend(["--workflow-url", workflow_url])
    cmd.extend(["--staging-dir", str(staging_dir)])
    if args.pack_output:
        cmd.extend(["--pack-output", args.pack_output])

    subprocess.run(cmd, check=True)

    staging_dir_str = str(staging_dir)
    print(
        f"Staged version {version} for release in {staging_dir_str}\n\n"
        "Verify the CLI:\n"
        f"    node {staging_dir_str}/bin/codex.js --version\n"
        f"    node {staging_dir_str}/bin/codex.js --help\n\n"
        f'Next:  cd "{staging_dir_str}" && npm publish'
    )

    if created_temp:
        print("(Temporary staging directory preserved for inspection.)")

    return 0


def prepare_staging_dir(tmp_path: str | None) -> tuple[Path, bool]:
    if tmp_path:
        staging_dir = Path(tmp_path).expanduser().resolve()
        staging_dir.mkdir(parents=True, exist_ok=True)
        return staging_dir, False

    temp_dir = Path(tempfile.mkdtemp(prefix="codex-npm-stage-"))
    return temp_dir, True


def resolve_release_workflow(version: str) -> dict:
    gh_run = subprocess.run(
        [
            "gh",
            "run",
            "list",
            "--branch",
            f"rust-v{version}",
            "--json",
            "workflowName,url,headSha",
            "--jq",
            'first(.[] | select(.workflowName == "rust-release"))',
        ],
        stdout=subprocess.PIPE,
        check=True,
    )
    gh_run.check_returncode()
    workflow = json.loads(gh_run.stdout)
    if not workflow:
        raise RuntimeError(f"Unable to find rust-release workflow for version {version}.")
    return workflow


if __name__ == "__main__":
    sys.exit(main())
