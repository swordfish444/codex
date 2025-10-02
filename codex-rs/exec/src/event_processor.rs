use std::path::PathBuf;

use codex_core::config::Config;
use codex_core::protocol::Event;
use codex_core::protocol::SessionConfiguredEvent;

pub(crate) enum CodexStatus {
    Running,
    InitiateShutdown,
    Shutdown,
}

pub(crate) trait EventProcessor {
    /// Print summary of effective configuration and user prompt.
    fn print_config_summary(
        &mut self,
        _config: &Config,
        _prompt: &str,
        _session_configured: &SessionConfiguredEvent,
    ) {
    }

    /// Handle a single event emitted by the agent.
    fn process_event(&mut self, event: Event) -> CodexStatus;

    fn print_final_output(&mut self) {}
}

pub(crate) fn handle_last_message(
    last_agent_message: Option<String>,
    output_file: Option<Option<PathBuf>>,
) {
    let message = last_agent_message.unwrap_or_default();

    if message.is_empty() && output_file.is_some() {
        eprintln!("Warning: no last agent message; wrote empty content to {output_file:?}");
    }
    match output_file {
        Some(Some(path)) => write_last_message_file(&message, &path),
        Some(None) => println!("{message}"),
        _ => (),
    }
}

fn write_last_message_file(contents: &str, last_message_path: &PathBuf) {
    if let Err(e) = std::fs::write(last_message_path, contents) {
        eprintln!("Failed to write last message file {last_message_path:?}: {e}");
    }
}
