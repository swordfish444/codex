use codex_core::config::Config;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;

use crate::event_processor::EventProcessor;

pub(crate) struct EventProcessorWithJsonOutput;

impl EventProcessorWithJsonOutput {
    pub fn new() -> Self {
        Self {}
    }
}

impl EventProcessor for EventProcessorWithJsonOutput {
    fn print_config_summary(&mut self, _config: &Config, _prompt: &str) {
        let _ = _config;
        // Intentionally left blank – human summary not needed in JSON mode.
    }

    fn process_event(&mut self, event: Event) {
        match event.msg {
            EventMsg::AgentMessageDelta(_) | EventMsg::AgentReasoningDelta(_) => {
                // Suppress streaming events in JSON mode.
            }
            _ => {
                if let Ok(line) = serde_json::to_string(&event) {
                    println!("{line}");
                }
            }
        }
    }
}
