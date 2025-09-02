**DOs**
- Build at least one release per OS: add one macOS and one Linux release job to catch conditional code (e.g., Landlock) and document why they exist.
```yaml
# One macOS release to mirror dev behavior
- runner: macos-14
  target: aarch64-apple-darwin  # Fastest macOS runner; keeps CI quick
  profile: release

# One Linux release to catch Landlock/target-specific code
- runner: ubuntu-24.04
  target: x86_64-unknown-linux-musl
  profile: release
```
- Make release jobs obvious: reflect `profile` in the job name so release builds are easy to spot in CI.
```yaml
name: ${{ matrix.runner }} - ${{ matrix.target }}${{ matrix.profile == 'release' && ' (release)' || '' }}
```
- Pass `--profile` everywhere: ensure build and test steps respect the matrix profile.
```yaml
- run: find . -name Cargo.toml -mindepth 2 -maxdepth 2 -print0 \
       | xargs -0 -n1 -I{} bash -c 'cd "$(dirname "{}")" && cargo build --profile ${{ matrix.profile }}'

- run: cargo test --all-features --target ${{ matrix.target }} --profile ${{ matrix.profile }}
```
- Align cache keys across workflows: include `profile` in CI and match it in release to maximize cache hits.
```yaml
# rust-ci.yml
with:
  key: cargo-${{ matrix.runner }}-${{ matrix.target }}-${{ matrix.profile }}-${{ hashFiles('**/Cargo.lock') }}

# rust-release.yml
with:
  key: cargo-release-${{ matrix.runner }}-${{ matrix.target }}-release-${{ hashFiles('**/Cargo.lock') }}
```
- Rebase after upstream merges: drop changes that already landed on `main` (e.g., imports) to keep diffs clean and avoid build breaks.
```bash
git fetch origin main
git rebase origin/main
# Resolve conflicts, ensure it builds locally, then:
git push --force-with-lease
```

**DON’Ts**
- Don’t hide release coverage: avoid sneaking a single release job into the middle of the matrix without a comment explaining why it’s there.
```yaml
# Bad: release entry with no context
- runner: macos-14
  target: aarch64-apple-darwin
  profile: release  # ← unexplained
```
- Don’t mismatch cache keys: using different key formats between CI and release wastes caches.
```yaml
# Bad: missing profile token → poor cache reuse
key: cargo-${{ matrix.runner }}-${{ matrix.target }}-${{ hashFiles('**/Cargo.lock') }}
```
- Don’t rely only on dev builds: release-only or target-conditional code (e.g., Landlock on Linux) can break if not built in release.
```yaml
# Bad: tests/builds always run in dev profile
- run: cargo test --all-features --target ${{ matrix.target }}
```
- Don’t keep stale changes post-merge: if `main` already includes an import or refactor, remove it from your PR via rebase instead of re-adding it.
```rust
// Bad (duplicate after rebase): already in main
use codex_core::user_agent::get_codex_user_agent;
```