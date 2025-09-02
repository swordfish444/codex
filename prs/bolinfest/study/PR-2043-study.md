**DOs**
- Checkout early: ensure repository is available to scripts.
```yaml
- name: Checkout repository
  uses: actions/checkout@v4
```

- Derive version from tag: strip the rust-v prefix for release name.
```yaml
- name: Release name
  id: release_name
  shell: bash
  run: |
    version="${GITHUB_REF_NAME#rust-v}"
    echo "name=${version}" >> "$GITHUB_OUTPUT"
```

- Set up Node + pnpm (pinned): mirror ci.yml for parity.
```yaml
- name: Setup Node.js
  uses: actions/setup-node@v4
  with:
    node-version: 22

- name: Setup pnpm
  uses: pnpm/action-setup@v4
  with:
    version: 10.8.1
    run_install: false
```

- Cache pnpm store: compute path and cache by pnpm-lock.yaml.
```yaml
- name: Get pnpm store directory
  id: pnpm-cache
  shell: bash
  run: echo "store_path=$(pnpm store path --silent)" >> "$GITHUB_OUTPUT"

- name: Setup pnpm cache
  uses: actions/cache@v4
  with:
    path: ${{ steps.pnpm-cache.outputs.store_path }}
    key: ${{ runner.os }}-pnpm-store-${{ hashFiles('**/pnpm-lock.yaml') }}
    restore-keys: |
      ${{ runner.os }}-pnpm-store-
```

- Stage npm package in CI: use helper script, GH token, and zip artifact.
```yaml
- name: Stage npm package
  env:
    GH_TOKEN: ${{ github.token }}
  run: |
    set -euo pipefail
    TMP_DIR="${RUNNER_TEMP}/npm-stage"
    python3 codex-cli/scripts/stage_rust_release.py \
      --release-version "${{ steps.release_name.outputs.name }}" \
      --tmp "${TMP_DIR}"
    mkdir -p dist/npm
    (cd "$TMP_DIR" && zip -r \
      "${GITHUB_WORKSPACE}/dist/npm/codex-npm-${{ steps.release_name.outputs.name }}.zip" .)
```

- Thread --tmp through Python helper: pass to stage_release.sh and verify success.
```python
parser.add_argument("--tmp", help="Optional path to stage the npm package")
cmd = [
    str(current_dir / "stage_release.sh"),
    "--version", version,
    "--workflow-url", workflow["url"],
]
if args.tmp:
    cmd.extend(["--tmp", args.tmp])

stage_release = subprocess.run(cmd)
stage_release.check_returncode()
```

- Use official v4 actions: keep versions consistent with ci.yml.
```yaml
uses: actions/checkout@v4
uses: actions/setup-node@v4
uses: pnpm/action-setup@v4
uses: actions/cache@v4
# Release upload:
uses: softprops/action-gh-release@v2
```

**DON’Ts**
- Don’t publish to npm from CI (2FA required); only stage the zip.
```yaml
# Bad: publishing in CI
- run: npm publish

# Good: stage artifact; publish manually later
- run: echo "Staged npm zip in dist/npm/"
```

- Don’t skip checkout before using repo scripts.
```yaml
# Bad
- uses: actions/download-artifact@v4

# Good
- uses: actions/checkout@v4
- uses: actions/download-artifact@v4
```

- Don’t run pnpm install unnecessarily during staging.
```yaml
# Bad
- uses: pnpm/action-setup@v4
  with:
    version: 10.8.1
    run_install: true

# Good
- uses: pnpm/action-setup@v4
  with:
    version: 10.8.1
    run_install: false
```

- Don’t build subprocess commands as shell strings (quoting pitfalls).
```python
# Bad
subprocess.run(f"{stage} --version {version} --workflow-url {url}", shell=True)

# Good
subprocess.run([stage, "--version", version, "--workflow-url", url])
```

- Don’t omit GH_TOKEN when calling GitHub APIs.
```yaml
# Bad
- name: Stage npm package
  run: python3 codex-cli/scripts/stage_rust_release.py --release-version "$VER"

# Good
- name: Stage npm package
  env:
    GH_TOKEN: ${{ github.token }}
  run: python3 codex-cli/scripts/stage_rust_release.py --release-version "$VER"
```

- Don’t ignore return codes from subprocesses.
```python
# Bad
subprocess.run(cmd)

# Good
result = subprocess.run(cmd)
result.check_returncode()
```

- Don’t leave paths/vars unquoted in bash (spaces break builds).
```bash
# Bad
(cd $TMP_DIR && zip -r $GITHUB_WORKSPACE/dist/npm/codex-npm-$VER.zip .)

# Good
(cd "$TMP_DIR" && zip -r "${GITHUB_WORKSPACE}/dist/npm/codex-npm-${VER}.zip" .)
```

- Don’t assume Node/pnpm steps are permanent; keep them for now, but expect removal when package.json dependencies are eliminated.
```yaml
# Today: keep Node/pnpm setup to build the npm package
- uses: actions/setup-node@v4
- uses: pnpm/action-setup@v4

# Future: remove when package.json has no "dependencies"
```