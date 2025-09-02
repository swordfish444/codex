**DOs**
- Bold the keyword: Assert exact output via one equality check for clarity and determinism.
```rust
let expected = format!(
    r#"[projects.{path_str}]
trust_level = "trusted"
"#
);
assert_eq!(contents, expected);
```

- Bold the keyword: Inline variables in `format!` raw strings to avoid juggling placeholders.
```rust
let initial = format!(
    r#"[projects]
{path_str} = {{ trust_level = "untrusted" }}
"#
);
let expected = format!(
    r#"[projects.{path_str}]
trust_level = "trusted"
"#
);
```

- Bold the keyword: Quote keys portably based on the path, so tests pass on Windows and Unix.
```rust
let raw_path = project_dir.path().to_string_lossy();
let path_str = if raw_path.contains('\\') {
    format!("'{}'", raw_path) // literal-quoted for backslashes
} else {
    format!("\"{}\"", raw_path) // basic-quoted otherwise
};
```

- Bold the keyword: Prefer explicit child tables over inline tables for project entries.
```rust
// Good: explicit child table
let expected = format!(
    r#"[projects.{path_str}]
trust_level = "trusted"
"#
);
```

- Bold the keyword: Keep `[projects]` implicit when you created it; only write it if it already existed.
```rust
// New file you created the table in: no standalone [projects]
let expected_new = format!(
    r#"[projects.{path_str}]
trust_level = "trusted"
"#
);

// Upgrading a file that already had [projects]: keep it and add the child table
let expected_upgrade = format!(
    r#"[projects]

[projects.{path_str}]
trust_level = "trusted"
"#
);
```

**DON’Ts**
- Bold the keyword: Don’t rely on scattered contains-based assertions; they’re brittle and unclear.
```rust
// Bad
assert!(!contents.contains("{ trust_level"));
assert!(contents.contains(&format!("[projects.\"{}\"]", raw_path))
     || contents.contains(&format!("[projects.'{}']", raw_path)));
```

- Bold the keyword: Don’t hardcode a single quoting style or check for two styles manually.
```rust
// Bad
let project_key_double = format!("[projects.\"{}\"]", raw_path);
let project_key_single = format!("[projects.'{}']", raw_path);
assert!(contents.contains(&project_key_double) || contents.contains(&project_key_single));
```

- Bold the keyword: Don’t emit inline tables for project trust; convert to an explicit table.
```rust
// Bad
let initial = format!(
    r#"[projects]
{path_str} = {{ trust_level = "trusted" }}
"#
);
```

- Bold the keyword: Don’t write a standalone `[projects]` header when you created the table.
```rust
// Bad
let expected = format!(
    r#"[projects]

[projects.{path_str}]
trust_level = "trusted"
"#
);
```

- Bold the keyword: Don’t leave `{}` placeholders when you can inline variables directly.
```rust
// Bad
let initial = format!(
    "[projects]\n'{}' = {{ trust_level = \"untrusted\" }}\n",
    raw_path
);
```