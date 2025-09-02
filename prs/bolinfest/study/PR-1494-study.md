**DOs**
- Verify runtime requirements: Cross-check `package.json` engines before bumping Dockerfile.
```json
// package.json
{
  "engines": { "node": ">=20" }
}
```
```dockerfile
# Dockerfile (aligned with engines)
FROM node:20-slim
```

- Document version bumps: Explain why a newer base image is necessary.
```dockerfile
# Dockerfile
# Requires Node >=22 due to <specific feature/dep>; see package.json "engines".
FROM node:22-slim
```

- Keep versions consistent: Align Node across Dockerfile and CI.
```yaml
# .github/workflows/codex.yml
- uses: actions/setup-node@v4
  with:
    node-version: '20'
```
```dockerfile
# Dockerfile
FROM node:20-slim
```

- Upgrade Rust deps conservatively: Separate breaking changes; justify each bump.
```toml
# Cargo.toml (non-breaking bump)
toml = "0.8.23"   # patch/minor within 0.8.x
# Breaking bumps (e.g., 0.8 -> 0.9) go in a dedicated PR with rationale.
```

- Prove safety with tests: Run targeted and full suites when core/common/protocol change.
```bash
# In codex-rs/
just fmt
just fix -p codex-core
cargo test -p codex-core
cargo test --all-features
# If TUI output changed:
cargo insta pending-snapshots -p codex-tui
```

**DON’Ts**
- Don’t bump Node “because it’s newer”: Require a concrete, documented need.
```dockerfile
# Bad: Unjustified bump
FROM node:22-slim
```

- Don’t let versions drift across environments: Avoid mismatches between CI and Dockerfile.
```yaml
# Bad: CI on 20...
- uses: actions/setup-node@v4
  with:
    node-version: '20'
```
```dockerfile
# ...but Dockerfile on 22
FROM node:22-slim
```

- Don’t sweep-update all crates without validation: Avoid blind `cargo update` across the workspace.
```bash
# Bad: Broad, unreviewed updates
cargo update
```

- Don’t mix breaking and non-breaking Rust upgrades in one PR: Isolate majors.
```toml
# Bad: Multiple majors at once across crates
toml = "0.9"           # 0.8 -> 0.9 (breaking)
tree-sitter-bash = "0.25.0"  # potential breaking API changes
```

- Don’t rely on incomplete test coverage: Add/adjust tests or snapshot updates when upgrading.
```bash
# Bad: Upgrading deps without verifying behavior
# (no `cargo test -p <crate>`, no snapshot review/accept)
```