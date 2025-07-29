use std::path::Path;
use std::path::PathBuf;

use codex_common::summarize_sandbox_policy;
use codex_core::WireApi;
use codex_core::config::Config;
use codex_core::model_supports_reasoning_summaries;
use codex_core::protocol::Event;

pub(crate) enum CodexStatus {
    Running,
    InitiateShutdown,
    Shutdown,
}

pub(crate) trait EventProcessor {
    /// Print summary of effective configuration and user prompt.
    fn print_config_summary(&mut self, config: &Config, prompt: &str);

    /// Handle a single event emitted by the agent.
    fn process_event(&mut self, event: Event) -> CodexStatus;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExperimentalInstructionsOrigin {
    File(PathBuf),
    Literal,
}

pub(crate) fn create_config_summary_entries(
    config: &Config,
    experimental_origin: Option<&ExperimentalInstructionsOrigin>,
) -> Vec<(&'static str, String)> {
    let mut entries = vec![
        ("workdir", config.cwd.display().to_string()),
        ("model", config.model.clone()),
        ("provider", config.model_provider_id.clone()),
        ("approval", config.approval_policy.to_string()),
        ("sandbox", summarize_sandbox_policy(&config.sandbox_policy)),
    ];
    if let Some(origin) = experimental_origin {
        let prompt_val = match origin {
            ExperimentalInstructionsOrigin::Literal => "experimental".to_string(),
            ExperimentalInstructionsOrigin::File(path) => path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
        };
        entries.push(("prompt", prompt_val));
    }
    if config.model_provider.wire_api == WireApi::Responses
        && model_supports_reasoning_summaries(config)
    {
        entries.push((
            "reasoning effort",
            config.model_reasoning_effort.to_string(),
        ));
        entries.push((
            "reasoning summaries",
            config.model_reasoning_summary.to_string(),
        ));
    }

    entries
}

pub(crate) fn handle_last_message(
    last_agent_message: Option<&str>,
    last_message_path: Option<&Path>,
) {
    match (last_message_path, last_agent_message) {
        (Some(path), Some(msg)) => write_last_message_file(msg, Some(path)),
        (Some(path), None) => {
            write_last_message_file("", Some(path));
            eprintln!(
                "Warning: no last agent message; wrote empty content to {}",
                path.display()
            );
        }
        (None, _) => eprintln!("Warning: no file to write last message to."),
    }
}

fn write_last_message_file(contents: &str, last_message_path: Option<&Path>) {
    if let Some(path) = last_message_path {
        if let Err(e) = std::fs::write(path, contents) {
            eprintln!("Failed to write last message file {path:?}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_core::config::Config;
    use codex_core::config::ConfigOverrides;
    use codex_core::config::ConfigToml;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn minimal_config() -> Config {
        let cwd = match TempDir::new() {
            Ok(t) => t,
            Err(e) => panic!("tempdir error: {e}"),
        };
        let codex_home = match TempDir::new() {
            Ok(t) => t,
            Err(e) => panic!("tempdir error: {e}"),
        };
        let cfg = ConfigToml {
            ..Default::default()
        };
        let overrides = ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        };
        match Config::load_from_base_config_with_overrides(
            cfg,
            overrides,
            codex_home.path().to_path_buf(),
        ) {
            Ok(c) => c,
            Err(e) => panic!("config error: {e}"),
        }
    }

    #[test]
    fn entries_include_prompt_experimental_for_literal_origin() {
        let mut cfg = minimal_config();
        cfg.base_instructions = Some("hello".to_string());
        let entries =
            create_config_summary_entries(&cfg, Some(&ExperimentalInstructionsOrigin::Literal));
        let map: HashMap<_, _> = entries.into_iter().collect();
        assert_eq!(map.get("prompt").cloned(), Some("experimental".to_string()));
    }

    #[test]
    fn entries_include_prompt_filename_for_file_origin() {
        let mut cfg = minimal_config();
        cfg.base_instructions = Some("hello".to_string());
        let path = PathBuf::from("/tmp/custom_instructions.txt");
        let entries = create_config_summary_entries(
            &cfg,
            Some(&ExperimentalInstructionsOrigin::File(path.clone())),
        );
        let map: HashMap<_, _> = entries.into_iter().collect();
        assert_eq!(
            map.get("prompt").cloned(),
            Some("custom_instructions.txt".to_string())
        );
    }
}
