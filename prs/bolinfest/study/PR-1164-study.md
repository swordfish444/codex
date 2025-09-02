**DOs**
- **Set the default model to "codex-mini-latest"**: Update constants, docs, and tests to reflect the new default.
```rust
env_flags! {
    pub OPENAI_DEFAULT_MODEL: &str = "codex-mini-latest";
    pub OPENAI_API_BASE: &str = "https://api.openai.com/v1";
}
```

- **Update protocol/serialization tests to expect the new default**: Assert the serialized payload includes the correct model.
```rust
let serialized = serde_json::to_string(&event).unwrap();
assert!(serialized.contains("\"model\":\"codex-mini-latest\""));
```

- **Demonstrate overrides with non-default models**: Use models like "o3" or "o4-mini" in examples to show how to override the default.
```toml
# config.toml
model = "o3"  # overrides default of "codex-mini-latest"
```
```bash
# CLI override (quoted or unquoted both fine)
codex -c model="o4-mini"
codex -c model=o4-mini
```

- **Keep example comments explicit about overrides**: Make it clear examples are overrides, not the default.
```rust
/// Optional override for the model name (e.g., "o3", "o4-mini")
pub model: Option<String>,
```

- **Document precedence with the updated default**: Ensure README and help text say the CLI defaults to "codex-mini-latest".
```md
4. the default value that comes with Codex CLI (i.e., Codex CLI defaults to `codex-mini-latest`)
```

**DON’Ts**
- **Don’t use the default model in override examples**: Using the default as an “override” confuses readers.
```rust
// Wrong (uses the default as an example override)
/// Optional override for the model name (e.g., "codex-mini-latest")
```

- **Don’t leave stale references to the old default**: Remove mentions that imply "o4-mini" is still the default.
```md
- 4. ... defaults to `o4-mini`   # Wrong
+ 4. ... defaults to `codex-mini-latest`
```

- **Don’t forget to align test fixtures and snapshots**: Any test expecting "o4-mini" as the model must be updated.
```rust
// Wrong
assert!(serialized.contains("\"model\":\"o4-mini\""));

// Correct
assert!(serialized.contains("\"model\":\"codex-mini-latest\""));
```

- **Don’t “fix” override examples to mirror the new default**: Keep them as non-default models to clearly illustrate overriding behavior.
```bash
# Wrong: shows default as if it were an override
codex -c model=codex-mini-latest
# Correct: shows a true override
codex -c model=o4-mini
```