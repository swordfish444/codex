**DOs**
- Bold the keyword, then colon + concise description.
- Merge related points when possible; avoid a bullet for every trivial detail.
- Keep bullets to one line unless breaking for clarity is unavoidable.

- Bold the keyword, then colon + concise description.

- **Isolate config editing:** Move config-toml mutation into a dedicated `config_edit.rs` module and re-export minimal APIs.
```rust
// lib.rs
pub mod config_edit;

// config.rs
pub use crate::config_edit::set_default_model_and_effort_for_profile;
```

- **Use idiomatic Option chaining:** Compute the effective profile with concise, readable combinators.
```rust
let effective_profile = profile
    .as_deref()
    .or_else(|| {
        load_config_as_toml(codex_home)
            .ok()
            .and_then(|v| v.get("profile").and_then(|i| i.as_str()))
    })
    .map(ToString::to_string);
```

- **Edit TOML by segments:** Address nested keys via explicit path segments so dots/spaces in names are handled safely.
```rust
let segments = ["profiles", "my.team name", "model"];
apply_toml_edit_override_segments(&mut doc, &segments, toml_edit::value("o3"));
```

- **Preserve formatting/comments:** When creating tables, mark them implicit to avoid gratuitous formatting changes.
```rust
current[*seg] = toml_edit::Item::Table(toml_edit::Table::new());
if let Some(t) = current[*seg].as_table_mut() {
    t.set_implicit(true);
}
```

- **Test full file output:** Assert against the entire resulting `config.toml` so table structure and comments are verified.
```rust
let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE))?;
let expected = r#"profile = "o3"

[profiles.o3]
model = "o3"
model_reasoning_effort = "high"
"#;
assert_eq!(contents, expected);
```

- **Use string fixtures:** Seed input TOML and expected TOML as strings to make intent explicit and reviewable.
```rust
let seed = r#"# Global comment
profile = "o3"

[profiles.o3]
existing = "keep"
"#;
std::fs::write(config_path, seed)?;
let expected = r#"# Global comment
profile = "o3"

[profiles.o3]
existing = "keep"
model = "o3"
model_reasoning_effort = "high"
"#;
// ... write via API, then:
assert_eq!(std::fs::read_to_string(config_path)?, expected);
```

- **Offer focused APIs:** Provide both top-level and profile-scoped helpers for clarity and reuse.
```rust
set_default_model_and_effort(codex_home, "gpt-5", ReasoningEffort::Minimal)?;
set_default_model_and_effort_for_profile(codex_home, Some("o3"), "o3", ReasoningEffort::High)?;
```

**DON’Ts**
- **Don’t bloat `config.rs`:** Avoid embedding large edit routines directly in `config.rs`; keep it thin.
```rust
// Bad (monolithic in config.rs):
fn set_default_model_and_effort(...) {
    // hundreds of lines of edit logic here
}
```

- **Don’t build dotted key strings:** Dotted keys break when names contain dots/spaces; use segments instead.
```rust
// Bad:
doc["profiles.my.team name"]["model"] = toml_edit::value("o3");
```

- **Don’t drop comments/formatting:** Avoid serializing from structs that rewrite the whole file and lose trivia.
```rust
// Bad:
let s = toml::to_string(&my_struct)?; // loses comments/order
std::fs::write(config_path, s)?;
```

- **Don’t assert partial changes:** Contains/regex checks miss structural and formatting regressions.
```rust
// Bad:
assert!(contents.contains("model = \"o3\"")); // ignores tables/comments
```

- **Don’t paper over parse errors:** If the TOML can’t be parsed, surface the error and don’t clobber the file.
```rust
// Bad:
let doc = std::fs::read_to_string(config_path)
    .ok()
    .and_then(|s| s.parse::<toml_edit::DocumentMut>().ok())
    .unwrap_or_default(); // silently discards invalid file
```