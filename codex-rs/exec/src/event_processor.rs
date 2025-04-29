use chrono::Utc;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::FileChange;
use owo_colors::OwoColorize;
use owo_colors::Style;
use shlex::try_join;
use std::collections::HashMap;

pub(crate) struct EventProcessor {
    call_id_to_command: HashMap<String, ExecCommandBegin>,

    // To ensure that --color=never is respected, ANSI escapes _must_ be added
    // using .style() with one of these fields. If you need a new style, add a
    // new field here.
    bold: Style,
    dimmed: Style,

    magenta: Style,
    red: Style,
    green: Style,
}

impl EventProcessor {
    pub(crate) fn create_with_ansi(with_ansi: bool) -> Self {
        let call_id_to_command = HashMap::new();

        if with_ansi {
            Self {
                call_id_to_command,
                bold: Style::new().bold(),
                dimmed: Style::new().dimmed(),
                magenta: Style::new().magenta(),
                red: Style::new().red(),
                green: Style::new().green(),
            }
        } else {
            Self {
                call_id_to_command,
                bold: Style::new(),
                dimmed: Style::new(),
                magenta: Style::new(),
                red: Style::new(),
                green: Style::new(),
            }
        }
    }
}

struct ExecCommandBegin {
    command: Vec<String>,
    start_time: chrono::DateTime<Utc>,
}

macro_rules! ts_println {
    ($($arg:tt)*) => {{
        let now = Utc::now();
        let formatted = now.format("%Y-%m-%dT%H:%M:%S").to_string();
        print!("[{}] ", formatted);
        println!($($arg)*);
    }};
}

impl EventProcessor {
    pub(crate) fn process_event(&mut self, event: Event) {
        let Event { id, msg } = event;
        match msg {
            EventMsg::Error { message } => {
                let prefix = "ERROR:".style(self.red);
                ts_println!("{prefix} {message}");
            }
            EventMsg::BackgroundEvent { message } => {
                ts_println!("{}", message.style(self.dimmed));
            }
            EventMsg::TaskStarted => {
                let msg = format!("Task started: {id}");
                ts_println!("{}", msg.style(self.dimmed));
            }
            EventMsg::TaskComplete => {
                let msg = format!("Task complete: {id}");
                ts_println!("{}", msg.style(self.bold));
            }
            EventMsg::AgentMessage { message } => {
                let prefix = "Agent message:".style(self.bold);
                ts_println!("{prefix} {message}");
            }
            EventMsg::ExecCommandBegin {
                call_id,
                command,
                cwd,
            } => {
                self.call_id_to_command.insert(
                    call_id.clone(),
                    ExecCommandBegin {
                        command: command.clone(),
                        start_time: Utc::now(),
                    },
                );
                ts_println!(
                    "{} {} in {}",
                    "exec".style(self.magenta),
                    escape_command(&command).style(self.bold),
                    cwd,
                );
            }
            EventMsg::ExecCommandEnd {
                call_id,
                stdout,
                stderr,
                exit_code,
            } => {
                let exec_command = self.call_id_to_command.remove(&call_id);
                let (duration, call) = if let Some(ExecCommandBegin {
                    command,
                    start_time,
                }) = exec_command
                {
                    let duration = Utc::now().signed_duration_since(start_time);
                    let millis = duration.num_milliseconds();
                    (
                        if millis < 1000 {
                            format!(" in {}ms", millis)
                        } else {
                            format!(" in {:.2}s", millis as f64 / 1000.0)
                        },
                        format!("{}", escape_command(&command).style(self.bold)),
                    )
                } else {
                    ("".to_string(), format!("exec('{call_id}')"))
                };

                let output = if exit_code == 0 { stdout } else { stderr };
                let truncated_output = output.lines().take(5).collect::<Vec<_>>().join("\n");
                match exit_code {
                    0 => {
                        let title = format!("{call} succeded{duration}:");
                        ts_println!("{}", title.style(self.green));
                    }
                    _ => {
                        let title = format!("{call} exited {exit_code}{duration}:");
                        ts_println!("{}", title.style(self.red));
                    }
                }
                println!("{}", truncated_output.style(self.dimmed));
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
                ts_println!("apply_patch('{call_id}') auto_approved={auto_approved}:\n{changes}");
            }
            EventMsg::PatchApplyEnd {
                call_id,
                stdout,
                stderr,
                success,
            } => {
                let (exit_code, output) = if success { (0, stdout) } else { (1, stderr) };
                let truncated_output = output.lines().take(5).collect::<Vec<_>>().join("\n");
                ts_println!("apply_patch('{call_id}') exited {exit_code}:\n{truncated_output}");
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

fn escape_command(command: &[String]) -> String {
    try_join(command.iter().map(|s| s.as_str())).unwrap_or_else(|_| command.join(" "))
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
