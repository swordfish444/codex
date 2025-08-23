use super::definition::SubagentDefinition;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
pub struct SubagentRegistry {
    /// Directory under the project (cwd/.codex/agents).
    project_dir: Option<PathBuf>,
    /// Directory under CODEX_HOME (~/.codex/agents).
    user_dir: Option<PathBuf>,
    /// Merged map: project definitions override user ones.
    map: HashMap<String, SubagentDefinition>,
}

impl SubagentRegistry {
    pub fn new(project_dir: Option<PathBuf>, user_dir: Option<PathBuf>) -> Self {
        Self {
            project_dir,
            user_dir,
            map: HashMap::new(),
        }
    }

    /// Loads JSON files from user_dir then project_dir (project wins on conflict).
    pub fn load(&mut self) {
        let mut map: HashMap<String, SubagentDefinition> = HashMap::new();

        // Load user definitions first
        if let Some(dir) = &self.user_dir {
            Self::load_from_dir_into(dir, &mut map);
        }
        // Then load project definitions which override on conflicts
        if let Some(dir) = &self.project_dir {
            Self::load_from_dir_into(dir, &mut map);
        }

        // Ensure a simple built‑in test subagent exists to validate wiring end‑to‑end.
        // Users can override this by providing their own definition named "hello".
        if !map.contains_key("hello") {
            map.insert(
                "hello".to_string(),
                SubagentDefinition {
                    name: "hello".to_string(),
                    description: "Built‑in test subagent that replies with a greeting".to_string(),
                    // Keep instructions narrow so models reliably output the intended text.
                    instructions:
                        "Reply with exactly this text and nothing else: Hello from subagent"
                            .to_string(),
                    // Disallow tool usage for the hello subagent.
                    tools: Some(Vec::new()),
                },
            );
        }

        self.map = map;
    }

    pub fn get(&self, name: &str) -> Option<&SubagentDefinition> {
        self.map.get(name)
    }

    pub fn all_names(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }

    fn load_from_dir_into(dir: &Path, out: &mut HashMap<String, SubagentDefinition>) {
        let Ok(iter) = fs::read_dir(dir) else {
            return;
        };
        for entry in iter.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("json"))
                    .unwrap_or(false)
            {
                match SubagentDefinition::from_file(&path) {
                    Ok(def) => {
                        out.insert(def.name.clone(), def);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load subagent from {}: {}", path.display(), e);
                    }
                }
            }
        }
    }
}
