#!/usr/bin/env python3
"""
Automate the release procedure documented in `../README.md → Releasing codex`.

Run this script from the repository *root*:

```bash
python release_codex.py
```

It performs the same steps that the README lists manually:

1. Create and switch to a `bump-version-<timestamp>` branch.
2. Bump the timestamp-based version in `codex-cli/package.json` **and**
   `codex-cli/src/utils/session.ts`.
3. Commit with a DCO sign-off.
4. Copy the top-level `README.md` into `codex-cli/` (npm consumers see it).
5. Run `pnpm release` (copies README again, builds, publishes to npm).
6. Push the branch so you can open a PR that merges the version bump.

The current directory can live anywhere; all paths are resolved relative to
this file so moving it elsewhere (e.g. into `scripts/`) still works.
"""

from __future__ import annotations

import datetime as _dt
import json as _json
import os
import re
import shutil
import subprocess as _sp
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

#   repo-root/
#   ├── codex-cli/
#   ├── scripts/       <-- you are here
#   └── README.md

REPO_ROOT = Path(__file__).resolve().parent.parent
CODEX_CLI = REPO_ROOT / "codex-cli"
PKG_JSON = CODEX_CLI / "package.json"
SESSION_TS = CODEX_CLI / "src" / "utils" / "session.ts"
README_SRC = REPO_ROOT / "README.md"
README_DST = CODEX_CLI / "README.md"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def sh(cmd: list[str] | str, *, cwd: Path | None = None) -> None:
    """Run *cmd* printing it first and exit on non-zero status."""

    if isinstance(cmd, list):
        printable = " ".join(cmd)
    else:
        printable = cmd

    print("+", printable)

    _sp.run(cmd, cwd=cwd, shell=isinstance(cmd, str), check=True)


def _new_version() -> str:
    """Return a new timestamp version string such as `0.1.2504301234`."""

    return "0.1." + _dt.datetime.utcnow().strftime("%y%m%d%H%M")


def bump_version() -> str:
    """Update package.json and session.ts, returning the new version."""

    new_ver = _new_version()

    # ---- package.json
    data = _json.loads(PKG_JSON.read_text())
    old_ver = data.get("version", "<unknown>")
    data["version"] = new_ver
    PKG_JSON.write_text(_json.dumps(data, indent=2) + "\n")

    # ---- session.ts
    pattern = r'CLI_VERSION = "0\\.1\\.\\d{10}"'
    repl = f'CLI_VERSION = "{new_ver}"'
    _text = SESSION_TS.read_text()
    if re.search(pattern, _text):
        SESSION_TS.write_text(re.sub(pattern, repl, _text))
    else:
        print(
            "WARNING: CLI_VERSION constant not found – file format may have changed",
            file=sys.stderr,
        )

    print(f"Version bump: {old_ver} → {new_ver}")
    return new_ver


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:  # noqa: C901 – readable top-level flow is desired
    # Ensure we can locate required files.
    for p in (CODEX_CLI, PKG_JSON, SESSION_TS, README_SRC):
        if not p.exists():
            sys.exit(f"Required path missing: {p.relative_to(REPO_ROOT)}")

    os.chdir(REPO_ROOT)

    # ------------------------------- create release branch
    branch = "bump-version-" + _dt.datetime.utcnow().strftime("%Y%m%d-%H%M")
    sh(["git", "checkout", "-b", branch])

    # ------------------------------- bump version + commit
    new_ver = bump_version()
    sh(
        [
            "git",
            "add",
            str(PKG_JSON.relative_to(REPO_ROOT)),
            str(SESSION_TS.relative_to(REPO_ROOT)),
        ]
    )
    sh(["git", "commit", "-s", "-m", f"chore(release): codex-cli v{new_ver}"])

    # ------------------------------- copy README (shown on npmjs.com)
    shutil.copyfile(README_SRC, README_DST)

    # ------------------------------- build + publish via pnpm script
    sh(["pnpm", "install"], cwd=CODEX_CLI)
    sh(["pnpm", "release"], cwd=CODEX_CLI)

    # ------------------------------- push branch
    sh(["git", "push", "-u", "origin", branch])

    print("\n✅  Release script finished!")
    print(f"   • npm publish run by pnpm script (branch: {branch})")
    print("   • Open a PR to merge the version bump once CI passes.")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit("\nCancelled by user")
