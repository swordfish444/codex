**DOs**
- **Prefer `cargo check`: Use for CI validation instead of `cargo build` to reduce time and disk usage.**
```yaml
- name: cargo check individual crates
  run: |
    find . -name Cargo.toml -mindepth 2 -maxdepth 2 -print0 \
      | xargs -0 -n1 -I{} bash -c 'cd "$(dirname "{}")" && cargo check --profile ${{ matrix.profile }}'
```

- **Per-crate checks: Run each crate individually to catch underspecified features hidden by workspace builds.**
```yaml
# Avoids workspace-root union-of-features masking
- if: ${{ matrix.target == 'x86_64-unknown-linux-gnu' && matrix.profile != 'release' }}
  name: cargo check individual crates
  run: |
    find . -name Cargo.toml -mindepth 2 -maxdepth 2 -print0 \
      | xargs -0 -n1 -I{} bash -c 'cd "$(dirname "{}")" && cargo check --profile ${{ matrix.profile }}'
```

- **Keep feature-union note: Retain the comment explaining workspace-root builds use the union of features.**
```yaml
# Running `cargo build` from the workspace root builds the workspace using
# the union of all features from third-party crates.
# To avoid masking underspecified features, check each crate individually.
```

- **Consistent step IDs: Rename step IDs and update all references (e.g., verify step).**
```yaml
- name: cargo check individual crates
  id: cargo_check_all_crates
  continue-on-error: true
  run: …

- name: verify all steps passed
  if: |
    steps.clippy.outcome == 'failure' ||
    steps.cargo_check_all_crates.outcome == 'failure' ||
    steps.test.outcome == 'failure'
  run: |
    echo "One or more checks failed (clippy, cargo_check_all_crates, or test). See logs for details."
    exit 1
```

- **Clean apt lists: Reclaim disk after installs to prevent “No space left on device.”**
```yaml
- if: ${{ matrix.target == 'x86_64-unknown-linux-musl' || matrix.target == 'aarch64-unknown-linux-musl' }}
  name: Install musl build tools
  run: |
    sudo apt install -y musl-tools pkg-config && sudo rm -rf /var/lib/apt/lists/*
```

- **Defer failure aggregation: Use `continue-on-error: true` for per-crate checks and fail once at the end.**
```yaml
- name: cargo check individual crates
  continue-on-error: true
  run: …
# Final step (see above) collects outcomes and exits non-zero if any failed.
```

- **Scope checks: Limit expensive per-crate checks to representative targets/profiles to keep CI fast.**
```yaml
if: ${{ matrix.target == 'x86_64-unknown-linux-gnu' && matrix.profile != 'release' }}
```


**DON’Ts**
- **Don’t use `cargo build` when `cargo check` suffices: Avoid unnecessary object file generation.**
```yaml
# Bad
- name: cargo build individual crates
  run: cargo build --profile ${{ matrix.profile }}

# Good
- name: cargo check individual crates
  run: cargo check --profile ${{ matrix.profile }}
```

- **Don’t rely only on workspace-root builds: They can hide missing feature specs.**
```yaml
# Bad
- name: workspace build only
  run: cargo build --workspace

# Good
- name: per-crate checks
  run: |
    find . -name Cargo.toml -mindepth 2 -maxdepth 2 -print0 \
      | xargs -0 -n1 -I{} bash -c 'cd "$(dirname "{}")" && cargo check --profile ${{ matrix.profile }}'
```

- **Don’t forget to update verify logic when step IDs/names change: Mismatched IDs silently skip failures.**
```yaml
# Bad (stale reference)
if: steps.build.outcome == 'failure'

# Good
if: steps.cargo_check_all_crates.outcome == 'failure'
```

- **Don’t leave apt caches: Skipping cleanup wastes space on CI runners.**
```yaml
# Bad
sudo apt install -y musl-tools pkg-config

# Good
sudo apt install -y musl-tools pkg-config && sudo rm -rf /var/lib/apt/lists/*
```

- **Don’t fail early on per-crate checks if you want a complete error report: Aggregate at the end.**
```yaml
# Bad
continue-on-error: false

# Good
continue-on-error: true
# …then fail in a final gate step if any step failed.
```

- **Don’t drop clippy/tests: Keep all gates (clippy, per-crate checks, tests) contributing to final status.**
```yaml
if: |
  steps.clippy.outcome == 'failure' ||
  steps.cargo_check_all_crates.outcome == 'failure' ||
  steps.test.outcome == 'failure'
```