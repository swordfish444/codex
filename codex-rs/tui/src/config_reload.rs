//! Helpers for config reload diff generation.

/// Generate a unified diff between the old and new config contents.
pub fn generate_diff(old: &str, new: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .header("Current", "New")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::generate_diff;

    #[test]
    fn diff_detects_line_change() {
        let old = "a\nb\nc\n";
        let new = "a\nx\nc\n";
        let diff = generate_diff(old, new);
        assert!(diff.contains("-b\n+x\n"), "Unexpected diff output: {}", diff);
    }
}
