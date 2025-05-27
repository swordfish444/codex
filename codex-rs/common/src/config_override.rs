//! Support for `-c key=value` overrides shared across Codex CLI tools.
//!
//! This module provides a [`CliConfigOverrides`] struct that can be embedded
//! into a `clap`-derived CLI struct using `#[clap(flatten)]`. Each occurrence
//! of `-c key=value` (or `--config key=value`) will be collected as a raw
//! string. Helper methods are provided to convert the raw strings into
//! key/value pairs as well as to apply them onto a mutable
//! `serde_json::Value` representing the configuration tree.

use clap::ArgAction;
use clap::Parser;
use serde_json::Value;

/// CLI option that captures arbitrary configuration overrides specified as
/// `-c key=value`. It intentionally keeps both halves **unparsed** so that the
/// calling code can decide how to interpret the right-hand side.
#[derive(Parser, Debug, Default, Clone)]
pub struct CliConfigOverrides {
    /// Override a configuration value that would otherwise be loaded from
    /// `~/.codex/config.toml`. Use a dotted path (`foo.bar.baz`) to override
    /// nested values. The `value` portion is parsed as JSON. If it fails to
    /// parse as JSON, the raw string is used as a literal.
    ///
    /// Examples:
    ///   - `-c model="o4-mini"`
    ///   - `-c 'sandbox_permissions=["disk-full-read-access"]'`
    ///   - `-c shell_environment_policy.inherit=all`
    #[arg(
        short = 'c',
        long = "config",
        value_name = "key=value",
        action = ArgAction::Append,
        global = true,
    )]
    pub raw_overrides: Vec<String>,
}

impl CliConfigOverrides {
    /// Parse the raw strings captured from the CLI into a list of `(path,
    /// value)` tuples where `value` is a `serde_json::Value`.
    pub fn parse_overrides(&self) -> Result<Vec<(String, Value)>, String> {
        self.raw_overrides
            .iter()
            .map(|s| {
                // Only split on the *first* '=' so values are free to contain
                // the character.
                let mut parts = s.splitn(2, '=');
                let key = match parts.next() {
                    Some(k) => k.trim(),
                    None => return Err("Override missing key".to_string()),
                };
                let value_str = parts
                    .next()
                    .ok_or_else(|| format!("Invalid override (missing '='): {s}"))?
                    .trim();

                if key.is_empty() {
                    return Err(format!("Empty key in override: {s}"));
                }

                // Attempt to parse as JSON. If that fails, treat it as a raw
                // string. This allows convenient usage such as
                // `-c model=o4-mini` without the quotes.
                let value: Value = match serde_json::from_str(value_str) {
                    Ok(v) => v,
                    Err(_) => Value::String(value_str.to_string()),
                };

                Ok((key.to_string(), value))
            })
            .collect()
    }

    /// Apply all parsed overrides onto `target`. Intermediate objects will be
    /// created as necessary. Values located at the destination path will be
    /// replaced.
    pub fn apply_on_value(&self, target: &mut Value) -> Result<(), String> {
        let overrides = self.parse_overrides()?;
        for (path, value) in overrides {
            apply_single_override(target, &path, value);
        }
        Ok(())
    }
}

/// Apply a single override onto `root`, creating intermediate objects as
/// necessary.
fn apply_single_override(root: &mut Value, path: &str, value: Value) {
    use serde_json::Map;

    let parts: Vec<&str> = path.split('.').collect();
    let mut current = root;

    for (i, part) in parts.iter().enumerate() {
        let is_last = i == parts.len() - 1;

        if is_last {
            // Replace value at leaf.
            if let Value::Object(obj) = current {
                obj.insert(part.to_string(), value);
            } else {
                // Replace non-object with object containing the leaf.
                *current = Value::Object({
                    let mut m = Map::new();
                    m.insert(part.to_string(), value);
                    m
                });
            }
            return;
        }

        // Traverse or create intermediate object.
        match current {
            Value::Object(obj) => {
                current = obj
                    .entry(part.to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
            }
            _ => {
                // Non-object encountered, replace with object so we can
                // continue traversal.
                *current = Value::Object(Map::new());
                if let Value::Object(obj) = current {
                    current = obj
                        .entry((*part).to_string())
                        .or_insert_with(|| Value::Object(Map::new()));
                }
            }
        }
    }
}
