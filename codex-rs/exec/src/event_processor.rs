use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::FileChange;

use crate::console_writer::ConsoleWriter;

pub(crate) struct EventProcessor {
    writer: Box<dyn ConsoleWriter>,
}

impl EventProcessor {
    pub(crate) fn new(writer: Box<dyn ConsoleWriter>) -> Self {
        EventProcessor { writer }
    }

    pub(crate) fn process_event(&mut self, event: &Event) {
        let Event { id, msg } = event;
        match msg {
            EventMsg::Error { message } => {
                println!("Error: {message}");
            }
            EventMsg::BackgroundEvent { .. } => {
                // Ignore these for now.
            }
            EventMsg::TaskStarted => {
                println!("Task started: {id}");
            }
            EventMsg::TaskComplete => {
                println!("Task complete: {id}");
            }
            EventMsg::AgentMessage { message } => {
                println!("Agent message: {message}");
            }
            EventMsg::ExecCommandBegin {
                call_id,
                command,
                cwd,
            } => {
                println!("exec('{call_id}'): {:?} in {cwd}", command);
            }
            EventMsg::ExecCommandEnd {
                call_id,
                stdout,
                stderr,
                exit_code,
            } => {
                let output = if *exit_code == 0 { stdout } else { stderr };
                let truncated_output = output.lines().take(5).collect::<Vec<_>>().join("\n");
                match exit_code {
                    0 => {
                        self.writer.exec_command_succeed(call_id, &truncated_output);
                    }
                    _ => {
                        self.writer
                            .exec_command_fail(call_id, *exit_code, &truncated_output);
                    }
                }
            }
            EventMsg::PatchApplyBegin {
                call_id,
                auto_approved,
                changes,
            } => {
                let changes = changes
                    .iter()
                    .map(|(path, change)| {
                        format!("{} {}", format_file_change(change), path.to_string_lossy())
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                println!("apply_patch('{call_id}') auto_approved={auto_approved}:\n{changes}");
            }
            EventMsg::PatchApplyEnd {
                call_id,
                stdout,
                stderr,
                success,
            } => {
                let (exit_code, output) = if *success { (0, stdout) } else { (1, stderr) };
                let truncated_output = output.lines().take(5).collect::<Vec<_>>().join("\n");
                println!("apply_patch('{call_id}') exited {exit_code}:\n{truncated_output}");
            }
            EventMsg::ExecApprovalRequest { .. } => {
                // Should we exit?
            }
            EventMsg::ApplyPatchApprovalRequest { .. } => {
                // Should we exit?
            }
            _ => {
                // Ignore event.
            }
        }
    }
}

fn format_file_change(change: &FileChange) -> &'static str {
    match change {
        FileChange::Add { .. } => "A",
        FileChange::Delete => "D",
        FileChange::Update {
            move_path: Some(_), ..
        } => "R",
        FileChange::Update {
            move_path: None, ..
        } => "M",
    }
}
