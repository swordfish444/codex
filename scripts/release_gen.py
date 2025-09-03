#!/usr/bin/env python3
import argparse
import json
import os
import re
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional, Sequence, Tuple


def header(msg: str) -> None:
    print(f"==> {msg}", file=sys.stderr)


USAGE_TEXT = """
Usage: scripts/release_gen.py [--dump-only] [-q|--quiet] [owner/repo] <from_tag> <to_tag> [version]

Examples:
  scripts/release_gen.py openai/codex v0.23.0 v0.24.0
  scripts/release_gen.py v0.23.0 v0.24.0                # auto-detect repo from git remote
  scripts/release_gen.py v0.23.0 v0.24.0 0.24.0         # auto-detect with explicit version
  scripts/release_gen.py --dump-only v0.23.0 v0.24.0    # only generate releases/release_dump_<ver>.txt
  scripts/release_gen.py -q v0.23.0 v0.24.0             # quiet Codex call with progress dots

Notes:
  - Requires: gh (GitHub CLI) for dump generation; codex CLI for note generation.
  - If release_dump_<ver>.txt does not exist, it will be created automatically.
  - Then runs codex to generate <ver>.txt based on the dump (unless --dump-only).
  - If you omit tags, the script lists the last 20 releases for the repo.
""".strip()


# -------- argument parsing (shell-like) --------


@dataclass
class Args:
    dump_only: bool
    quiet: bool
    rest: List[str]


def parse_args(argv: Sequence[str]) -> Args:
    dump_only = False
    quiet = False
    rest: List[str] = []
    it = iter(argv)
    for a in it:
        if a == "--dump-only":
            dump_only = True
        elif a in ("-q", "--quiet"):
            quiet = True
        elif a in ("-h", "--help"):
            print(USAGE_TEXT)
            sys.exit(0)
        else:
            rest.append(a)
    return Args(dump_only=dump_only, quiet=quiet, rest=rest)


# -------- helpers --------


def run(cmd: Sequence[str], check: bool = True, capture: bool = True, text: bool = True, env: Optional[dict] = None) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, check=check, capture_output=capture, text=text, env=env)


def which(name: str) -> Optional[str]:
    from shutil import which as _which

    return _which(name)


def abspath(p: Path) -> Path:
    return p.resolve()


def detect_repo_from_git() -> Optional[str]:
    # Try origin then upstream
    urls: List[str] = []
    for remote in ("origin", "upstream"):
        try:
            cp = run(["git", "remote", "get-url", remote])
            if cp.stdout.strip():
                urls.append(cp.stdout.strip())
                break
        except subprocess.CalledProcessError:
            continue
    if not urls:
        return None
    remote = urls[0]
    path = remote
    # strip protocols and user@
    for prefix in ("git@", "ssh://", "https://", "http://"):
        if path.startswith(prefix):
            path = path[len(prefix) :]
    if "@" in path:
        path = path.split("@", 1)[1]
    # handle github.com:owner/repo or .../owner/repo
    if ":" in path and path.split(":", 1)[0].endswith("github.com"):
        path = path.split(":", 1)[1]
    if "github.com/" in path:
        path = path.split("github.com/", 1)[1]
    path = path.lstrip("/")
    if path.endswith(".git"):
        path = path[: -len(".git")]
    parts = path.split("/")
    if len(parts) >= 2:
        return f"{parts[0]}/{parts[1]}"
    return None


def show_recent_releases_and_exit(repo: str) -> None:
    print("", file=sys.stderr)
    print("Please pass a source/target release.", file=sys.stderr)
    print("", file=sys.stderr)
    print("e.g.: ./scripts/release_gen.sh rust-v0.23.0 rust-v0.24.0", file=sys.stderr)
    print("", file=sys.stderr)
    header(f"Recent releases for {repo}:")
    print("", file=sys.stderr)
    try:
        cp = run(["gh", "release", "list", "--repo", repo, "--limit", "20"])
        lines = cp.stdout.splitlines()
        for line in lines:
            if not line.strip():
                continue
            first = line.split()[0]
            print(f"- {first}", file=sys.stderr)
    except subprocess.CalledProcessError:
        print(f"Error: unable to fetch releases for {repo}", file=sys.stderr)
        sys.exit(1)
    sys.exit(1)


# -------- dump generation (ported) --------


def gh_json(args: Sequence[str]) -> dict:
    cp = run(["gh", *args])
    return json.loads(cp.stdout)


def get_tag_datetime_iso(repo: str, tag: str) -> str:
    # Try release publish date
    try:
        cp = run([
            "gh",
            "release",
            "view",
            tag,
            "--repo",
            repo,
            "--json",
            "publishedAt",
            "--jq",
            ".publishedAt",
        ])
        ts = cp.stdout.strip()
        if ts and ts != "null":
            return ts
    except subprocess.CalledProcessError:
        pass

    # Fallback via tag -> commit -> committer.date
    ref = gh_json(["api", f"repos/{repo}/git/ref/tags/{tag}"])
    obj_type = ref.get("object", {}).get("type")
    obj_url = ref.get("object", {}).get("url")
    commit_sha = None
    if obj_type == "tag" and obj_url:
        tag_obj = gh_json(["api", obj_url])
        commit_sha = (tag_obj.get("object") or {}).get("sha")
    else:
        commit_sha = (ref.get("object") or {}).get("sha")
    if not commit_sha:
        raise RuntimeError(f"Failed to resolve commit for tag {tag}")
    commit = gh_json(["api", f"repos/{repo}/commits/{commit_sha}"])
    return ((commit.get("commit") or {}).get("committer") or {}).get("date") or ""


def collect_prs_within_range(repo: str, from_iso: str, to_iso: str) -> List[dict]:
    cp = run(
        [
            "gh",
            "pr",
            "list",
            "--repo",
            repo,
            "--state",
            "merged",
            "--limit",
            "1000",
            "--json",
            "number,title,mergedAt,author,body",
        ]
    )
    data = json.loads(cp.stdout)

    def keep(pr: dict) -> bool:
        ma = pr.get("mergedAt")
        return bool(ma and from_iso <= ma <= to_iso)

    out = []
    for pr in data:
        if not keep(pr):
            continue
        out.append(
            {
                "number": pr.get("number"),
                "title": pr.get("title") or "",
                "merged_at": pr.get("mergedAt") or "",
                "author": ((pr.get("author") or {}).get("login")) or "-",
                "body": pr.get("body") or "",
            }
        )
    out.sort(key=lambda x: x.get("merged_at") or "", reverse=True)
    return out


_ISSUE_CLOSING_RE = re.compile(
    r"(?i)(?:close|closed|closes|fix|fixed|fixes|resolve|resolved|resolves)[\s:]+(?:[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)?#(\d+)"
)


def format_related_issues(body: str) -> str:
    body = body.replace("\r", "")
    nums = {_ for _ in _ISSUE_CLOSING_RE.findall(body)}
    if not nums:
        return "-"
    ints = sorted({int(n) for n in nums})
    return ", ".join([f"#{n}" for n in ints])


def generate_dump(repo: str, from_tag: str, to_tag: str, out_file: Path) -> None:
    if not which("gh"):
        print("Error: gh (GitHub CLI) is required", file=sys.stderr)
        sys.exit(1)

    header(f"Resolving tag dates ({from_tag} -> {to_tag})")
    from_iso = get_tag_datetime_iso(repo, from_tag)
    to_iso = get_tag_datetime_iso(repo, to_tag)
    if not (from_iso and to_iso):
        print(
            f"Error: failed to resolve tag dates. from={from_tag} ({from_iso}) to={to_tag} ({to_iso})",
            file=sys.stderr,
        )
        sys.exit(1)

    header("Collecting merged PRs via gh pr list")
    prs = collect_prs_within_range(repo, from_iso, to_iso)
    count = len(prs)

    header(f"Writing {out_file} (Total PRs: {count})")
    out_file.parent.mkdir(parents=True, exist_ok=True)
    with out_file.open("w", encoding="utf-8") as f:
        f.write(f"Repository: {repo}\n")
        f.write(f"Range: {from_tag} ({from_iso}) -> {to_tag} ({to_iso})\n")
        f.write(f"Generated: {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}\n")
        f.write(f"Total PRs: {count}\n\n")

        for pr in prs:
            title = pr["title"]
            number = pr["number"]
            merged_at = pr["merged_at"]
            author = pr["author"]
            body = pr["body"]
            issues = format_related_issues(body)

            f.write(f"PR #{number}: {title}\n")
            f.write(f"Merged: {merged_at} | Author: {author}\n")
            f.write(f"Related issues: {issues}\n\n")

            dep_names = ("app/dependabot", "dependabot[bot]")
            if author not in dep_names and re.search(r"[Dd]ependabot", author) is None:
                f.write("Description:\n")
                max_len = 2000
                snippet = body[:max_len]
                if len(body) > max_len:
                    snippet += "..."
                f.write(snippet + "\n\n")
            f.write("-----\n\n")

    header(f"Done -> {out_file}")


def build_prompt(dump_path: Path, gen_file: Path) -> str:
    dump_content = dump_path.read_text(encoding="utf-8")
    example = (
        "## Highlights:\n\n"
        "### New Features\n"
        "- Queued messages (#2637)\n"
        "- Copy Paste / Drag & Drop image files (#2567)\n"
        "- Add web_search tool (#2371)\n"
        "- Add transcript mode (Ctrl+T) with scrolling ability (#2525)\n"
        "- Edit/resume conversation (esc-esc) from previous messages (#2607)\n\n"
        "### TUI\n"
        "- Hide CoT by default; show headers in status indicator (#2316)\n"
        "- Show diff output in pager (+ with hunk headers) (#2568)\n"
        "- Simplify command approval UI (#2708)\n"
        "- Unify Esc/Ctrl+C interrupt handling (#2661)\n"
        "- Fix windows powershell paste (#2544)\n\n"
        "### Tools and execution\n"
        "- Add support for long-running shell commands `exec_command`/`write_stdin` (#2574)\n"
        "- Improve apply_patch reliability (#2646)\n"
        "- Cap retry counts (#2701)\n"
        "- Sort MCP tools deterministically (#2611)\n\n"
        "### Misc\n"
        "- Add model_verbosity config for GPT-5 (#2108)\n"
        "- Read all AGENTS.md files up to git root (#2532)\n"
        "- Fix git root resolution in worktrees (#2585)\n"
        "- Improve error messages & handling (#2695, #2587, #2640, #2540)\n\n\n"
        "## Full list of merged PRs:\n\n"
        " - #2708 [feat] Simplify command approval UI\n"
        " - #2706 [chore] Tweak..."
    )
    instr = (
        f"{dump_content}\n\n---\n\n"
        f"Please generate a summarized release note based on the list of PRs above. "
        f"Then, write a file called {gen_file} with your suggested release notes. "
        f"It should follow this structure (+ the style/tone/brevity in this example):\n\n"
        f"\"{example}\""
    )
    return instr


def run_codex(prompt: str, quiet: bool) -> int:
    if not which("codex"):
        print(
            "Error: codex CLI is required for generation. Use --dump-only to skip.",
            file=sys.stderr,
        )
        return 127

    cmd = ["codex", "exec", "--sandbox", "workspace-write", prompt]
    if quiet:
        try:
            proc = subprocess.Popen(
                cmd,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                text=True,
            )
        except FileNotFoundError:
            return 127
        while True:
            ret = proc.poll()
            if ret is not None:
                break
            print(".", end="", file=sys.stderr, flush=True)
            time.sleep(1)
        print("", file=sys.stderr)
        return ret or 0
    else:
        # stream to stderr like the shell script
        proc = subprocess.run(cmd, stdout=sys.stderr, stderr=sys.stderr, text=True)
        return proc.returncode


def main(argv: Sequence[str]) -> int:
    pargs = parse_args(argv)

    rest = pargs.rest
    # repo optional first arg
    if rest and "/" in rest[0]:
        repo = rest[0]
        rest = rest[1:]
    else:
        repo = detect_repo_from_git() or ""
        if not repo:
            print(
                "Error: failed to auto-detect repository from git remote. Provide [owner/repo] explicitly.",
                file=sys.stderr,
            )
            return 1

    if len(rest) < 2:
        show_recent_releases_and_exit(repo)
        return 1  # unreachable

    from_tag, to_tag = rest[0], rest[1]
    ver = rest[2] if len(rest) >= 3 else to_tag
    ver = ver.lstrip("v")

    script_dir = Path(__file__).resolve().parent
    releases_dir = script_dir / "releases"
    dump_file = releases_dir / f"release_dump_{ver}.txt"
    gen_file = releases_dir / f"{ver}.txt"

    # Create dump if missing
    if not dump_file.exists():
        header(f"Dump not found: {dump_file}. Generating...")
        generate_dump(repo, from_tag, to_tag, dump_file)
    else:
        header(f"Using existing dump: {dump_file}")

    if pargs.dump_only:
        return 0

    dump_path = abspath(dump_file)
    prompt = build_prompt(dump_path, gen_file)
    header(f"Calling codex to generate {gen_file}")
    status = run_codex(prompt, pargs.quiet)

    if gen_file.exists():
        # Output only the generated release notes to stdout
        sys.stdout.write(gen_file.read_text(encoding="utf-8"))
        return 0
    else:
        print(f"Warning: {gen_file} not created. Check codex output.", file=sys.stderr)
        return 1 if status != 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

