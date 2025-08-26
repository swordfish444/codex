use codex_protocol::custom_prompts::CustomPrompt;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

/// Return the default prompts directory: ~/.codex/prompts based on $HOME.
pub fn default_prompts_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(format!("{home}/.codex/prompts"))
}

/// Discover prompt files in the given directory, returning entries sorted by name.
/// Non-files are ignored. If the directory does not exist or cannot be read, returns empty.
pub fn discover_prompts_in(dir: &Path) -> Vec<CustomPrompt> {
    discover_prompts_in_excluding(dir, &HashSet::new())
}

/// Discover prompt files in the given directory, excluding any with names in `exclude`.
/// Returns entries sorted by name. Non-files are ignored. Missing/unreadable dir yields empty.
pub fn discover_prompts_in_excluding(dir: &Path, exclude: &HashSet<String>) -> Vec<CustomPrompt> {
    let mut out: Vec<CustomPrompt> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
            else {
                continue;
            };
            if exclude.contains(&name) {
                continue;
            }
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            out.push(CustomPrompt { name, content });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
    }
    out
}

/// Discover prompt files in the default prompts directory, excluding any with names in `exclude`.
pub fn discover_prompts_excluding(exclude: &HashSet<String>) -> Vec<CustomPrompt> {
    let dir = default_prompts_dir();
    discover_prompts_in_excluding(&dir, exclude)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn empty_when_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nope");
        let found = discover_prompts_in(&missing);
        assert!(found.is_empty());
    }

    #[test]
    fn discovers_and_sorts_files() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("b"), b"b").unwrap();
        fs::write(dir.join("a"), b"a").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();
        let found = discover_prompts_in(dir);
        let names: Vec<String> = found.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn excludes_builtins() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("init"), b"ignored").unwrap();
        fs::write(dir.join("foo"), b"ok").unwrap();
        let mut exclude = HashSet::new();
        exclude.insert("init".to_string());
        let found = discover_prompts_in_excluding(dir, &exclude);
        let names: Vec<String> = found.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["foo"]);
    }
}
