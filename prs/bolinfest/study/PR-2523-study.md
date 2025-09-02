**DOs**
- Explicit tables: Render per-project entries as explicit TOML tables under `[projects]`.
```toml
# Desired
[projects]
[projects."/path/to/project"]
trust_level = "trusted"
```

- Enforce explicit with toml_edit: Create tables and mark the project table non-implicit; avoid structures that serialize as inline.
```rust
let project_key = project_path.to_string_lossy().to_string();

let root = doc.as_table_mut();
if !root.contains_key("projects") || root.get("projects").and_then(|i| i.as_table()).is_none() {
    root.insert("projects", toml_edit::table());
}
let projects_tbl = doc["projects"].as_table_mut().expect("projects is a table");

if !projects_tbl.contains_key(project_key.as_str())
    || projects_tbl.get(project_key.as_str()).and_then(|i| i.as_table()).is_none()
{
    projects_tbl.insert(project_key.as_str(), toml_edit::table());
}

let proj_tbl = projects_tbl
    .get_mut(project_key.as_str())
    .and_then(|i| i.as_table_mut())
    .expect(&format!("project table missing for {}", project_key));
proj_tbl.set_implicit(false);
proj_tbl["trust_level"] = toml_edit::value("trusted");
```

- Convert legacy inline entries: Replace any existing inline project entries with explicit tables before updating values.
```rust
// If an inline entry exists, overwrite it with an explicit table first
projects_tbl.insert(project_key.as_str(), toml_edit::table());
projects_tbl[project_key.as_str()]["trust_level"] = toml_edit::value("trusted");
projects_tbl[project_key.as_str()].as_table_mut().unwrap().set_implicit(false);
```

- Prefer exact assertions when deterministic: If the output is stable, assert on the full string.
```rust
let expected = format!(
    "[projects]\n[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
    project_dir.path().to_string_lossy()
);
assert_eq!(contents, expected);
```

- Be robust when variability is legitimate: When libraries may choose different quoting (e.g., Windows paths), assert alternatives and explain why in the test.
```rust
let path_str = project_dir.path().to_string_lossy();
let header_double = format!("[projects.\"{}\"]", path_str);
let header_single = format!("[projects.'{}']", path_str);
// toml_edit may choose basic or literal key quoting depending on backslashes
assert!(contents.contains(&header_double) || contents.contains(&header_single));
```

- Assert intent, not formatting trivia: Check for the presence of the trusted value and absence of inline tables.
```rust
assert!(contents.contains("trust_level = \"trusted\""));
assert!(!contents.contains("{ trust_level"));
```

**DON’Ts**
- Inline tables: Don’t serialize per-project config as inline tables.
```toml
# Avoid
[projects]
"/path/to/project" = { trust_level = "trusted" }
```

- Direct nested assignment that yields inline: Don’t rely on direct nested indexing that can produce inline tables.
```rust
// Avoid
doc["projects"][project_key.as_str()]["trust_level"] = toml_edit::value("trusted");
```

- Overly brittle tests: Don’t assert on a single quoting style when the library can validly choose another.
```rust
// Avoid: brittle if toml_edit uses literal quotes
assert!(contents.contains(&format!("[projects.\"{}\"]", path_str)));
```

- Over-constraining headers: Don’t require a standalone `[projects]` header in tests if it isn’t semantically necessary.
```rust
// Avoid: over-specific formatting requirement
assert!(contents.contains("[projects]\n")); // too strict; formatting may differ
```

- Unexplained fuzziness: Don’t use loose `contains` checks without a clear rationale; if exact output isn’t asserted, add a brief comment describing the legitimate variability.
```rust
// Avoid: unexplained fuzzy assertion
assert!(contents.contains("projects"));
```