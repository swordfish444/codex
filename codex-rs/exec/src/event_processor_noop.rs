use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;

use crate::event_processor::CodexStatus;
use crate::event_processor::EventProcessor;

pub(crate) struct EventProcessorNoop {}

impl EventProcessor for EventProcessorNoop {
    fn process_event(&mut self, event: Event) -> CodexStatus {
        let Event { id: _, msg } = event;
        match msg {
            EventMsg::ShutdownComplete => CodexStatus::Shutdown,
            EventMsg::TaskComplete(_) => CodexStatus::InitiateShutdown,
            _ => CodexStatus::Running,
        }
    }
}
