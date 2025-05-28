use base64::Engine;
use codex_ansi_escape::ansi_escape_line;
use codex_common::elapsed::format_duration;
use codex_core::config::Config;
use codex_core::protocol::FileChange;
use codex_core::protocol::SessionConfiguredEvent;
use image::DynamicImage;
use image::GenericImageView;
use image::ImageReader;
use ratatui::prelude::*;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line as RtLine;
use ratatui::text::Span as RtSpan;
use ratatui_image::Image as TuiImage;

use crate::cell_widget::CellWidget;
use crate::text_block::TextBlock;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use crate::exec_command::escape_command;
use crate::markdown::append_markdown;

pub(crate) struct CommandOutput {
    pub(crate) exit_code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) duration: Duration,
}

pub(crate) enum PatchEventType {
    ApprovalRequest,
    ApplyBegin { auto_approved: bool },
}

/// Represents an event to display in the conversation history. Returns its
/// `Vec<Line<'static>>` representation to make it easier to display in a
/// scrollable list.
pub(crate) enum HistoryCell {
    /// Welcome message.
    WelcomeMessage { view: TextBlock },

    /// Message from the user.
    UserPrompt { view: TextBlock },

    /// Message from the agent.
    AgentMessage { view: TextBlock },

    /// Reasoning event from the agent.
    AgentReasoning { view: TextBlock },

    /// An exec tool call that has not finished yet.
    ActiveExecCommand {
        call_id: String,
        /// The shell command, escaped and formatted.
        command: String,
        start: Instant,
        view: TextBlock,
    },

    /// Completed exec tool call.
    CompletedExecCommand { view: TextBlock },

    /// An MCP tool call that has not finished yet.
    ActiveMcpToolCall {
        call_id: String,
        /// `server.tool` fully-qualified name so we can show a concise label
        fq_tool_name: String,
        /// Formatted invocation that mirrors the `$ cmd ...` style of exec
        /// commands. We keep this around so the completed state can reuse the
        /// exact same text without re-formatting.
        invocation: String,
        start: Instant,
        view: TextBlock,
    },

    /// Completed MCP tool call where we show the result serialized as JSON.
    CompletedMcpToolCallWithTextOutput { view: TextBlock },

    /// Completed MCP tool call where the result is an image.
    /// Admittedly, [mcp_types::CallToolResult] can have multiple content types,
    /// which could be a mix of text and images, so we need to tighten this up.
    CompletedMcpToolCallWithImageOutput { image: DynamicImage },

    /// Background event.
    BackgroundEvent { view: TextBlock },

    /// Error event from the backend.
    ErrorEvent { view: TextBlock },

    /// Info describing the newly-initialized session.
    SessionInfo { view: TextBlock },

    /// A pending code patch that is awaiting user approval. Mirrors the
    /// behaviour of `ActiveExecCommand` so the user sees *what* patch the
    /// model wants to apply before being prompted to approve or deny it.
    PendingPatch { view: TextBlock },
}

const TOOL_CALL_MAX_LINES: usize = 5;

impl HistoryCell {
    pub(crate) fn new_session_info(
        config: &Config,
        event: SessionConfiguredEvent,
        is_first_event: bool,
    ) -> Self {
        let SessionConfiguredEvent {
            model,
            session_id,
            history_log_id: _,
            history_entry_count: _,
        } = event;
        if is_first_event {
            let mut lines: Vec<Line<'static>> = vec![
                Line::from(vec![
                    "OpenAI ".into(),
                    "Codex".bold(),
                    " (research preview)".dim(),
                ]),
                Line::from(""),
                Line::from(vec![
                    "codex session".magenta().bold(),
                    " ".into(),
                    session_id.to_string().dim(),
                ]),
            ];

            let entries = vec![
                ("workdir", config.cwd.display().to_string()),
                ("model", config.model.clone()),
                ("provider", config.model_provider_id.clone()),
                ("approval", format!("{:?}", config.approval_policy)),
                ("sandbox", format!("{:?}", config.sandbox_policy)),
            ];
            for (key, value) in entries {
                lines.push(Line::from(vec![format!("{key}: ").bold(), value.into()]));
            }
            lines.push(Line::from(""));
            HistoryCell::WelcomeMessage {
                view: TextBlock::new(lines),
            }
        } else if config.model == model {
            HistoryCell::SessionInfo {
                view: TextBlock::new(Vec::new()),
            }
        } else {
            let lines = vec![
                Line::from("model changed:".magenta().bold()),
                Line::from(format!("requested: {}", config.model)),
                Line::from(format!("used: {}", model)),
                Line::from(""),
            ];
            HistoryCell::SessionInfo {
                view: TextBlock::new(lines),
            }
        }
    }

    pub(crate) fn new_user_prompt(message: String) -> Self {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("user".cyan().bold()));
        lines.extend(message.lines().map(|l| Line::from(l.to_string())));
        lines.push(Line::from(""));

        HistoryCell::UserPrompt {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_agent_message(config: &Config, message: String) -> Self {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("codex".magenta().bold()));
        append_markdown(&message, &mut lines, config);
        lines.push(Line::from(""));

        HistoryCell::AgentMessage {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_agent_reasoning(config: &Config, text: String) -> Self {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("thinking".magenta().italic()));
        append_markdown(&text, &mut lines, config);
        lines.push(Line::from(""));

        HistoryCell::AgentReasoning {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_active_exec_command(call_id: String, command: Vec<String>) -> Self {
        let command_escaped = escape_command(&command);
        let start = Instant::now();

        let lines: Vec<Line<'static>> = vec![
            Line::from(vec!["command".magenta(), " running...".dim()]),
            Line::from(format!("$ {command_escaped}")),
            Line::from(""),
        ];

        HistoryCell::ActiveExecCommand {
            call_id,
            command: command_escaped,
            start,
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_completed_exec_command(command: String, output: CommandOutput) -> Self {
        let CommandOutput {
            exit_code,
            stdout,
            stderr,
            duration,
        } = output;

        let mut lines: Vec<Line<'static>> = Vec::new();

        // Title depends on whether we have output yet.
        let title_line = Line::from(vec![
            "command".magenta(),
            format!(
                " (code: {}, duration: {})",
                exit_code,
                format_duration(duration)
            )
            .dim(),
        ]);
        lines.push(title_line);

        let src = if exit_code == 0 { stdout } else { stderr };

        lines.push(Line::from(format!("$ {command}")));
        let mut lines_iter = src.lines();
        for raw in lines_iter.by_ref().take(TOOL_CALL_MAX_LINES) {
            lines.push(ansi_escape_line(raw).dim());
        }
        let remaining = lines_iter.count();
        if remaining > 0 {
            lines.push(Line::from(format!("... {} additional lines", remaining)).dim());
        }
        lines.push(Line::from(""));

        HistoryCell::CompletedExecCommand {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_active_mcp_tool_call(
        call_id: String,
        server: String,
        tool: String,
        arguments: Option<serde_json::Value>,
    ) -> Self {
        let fq_tool_name = format!("{server}.{tool}");

        // Format the arguments as compact JSON so they roughly fit on one
        // line. If there are no arguments we keep it empty so the invocation
        // mirrors a function-style call.
        let args_str = arguments
            .as_ref()
            .map(|v| {
                // Use compact form to keep things short but readable.
                serde_json::to_string(v).unwrap_or_else(|_| v.to_string())
            })
            .unwrap_or_default();

        let invocation = if args_str.is_empty() {
            format!("{fq_tool_name}()")
        } else {
            format!("{fq_tool_name}({args_str})")
        };

        let start = Instant::now();
        let title_line = Line::from(vec!["tool".magenta(), " running...".dim()]);
        let lines: Vec<Line<'static>> = vec![
            title_line,
            Line::from(format!("$ {invocation}")),
            Line::from(""),
        ];

        HistoryCell::ActiveMcpToolCall {
            call_id,
            fq_tool_name,
            invocation,
            start,
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_completed_mcp_tool_call(
        fq_tool_name: String,
        invocation: String,
        start: Instant,
        success: bool,
        result: Result<mcp_types::CallToolResult, String>,
    ) -> Self {
        // Let's do a quick check to see if the result corresponds to a single
        // image output.
        match &result {
            Ok(mcp_types::CallToolResult { content, .. }) => {
                if let Some(first) = content.first() {
                    if let mcp_types::CallToolResultContent::ImageContent(image) = first {
                        let raw_data =
                            match base64::engine::general_purpose::STANDARD.decode(&image.data) {
                                Ok(data) => data,
                                Err(_) => Vec::new(),
                            };
                        let reader = ImageReader::new(Cursor::new(raw_data))
                            .with_guessed_format()
                            .expect("Cursor io never fails");
                        let image = reader.decode().expect("Image decoding should succeed");

                        return HistoryCell::CompletedMcpToolCallWithImageOutput {
                            image: image.clone(),
                        };
                    }
                }
            }
            _ => { /* continue */ }
        }

        let duration = format_duration(start.elapsed());
        let status_str = if success { "success" } else { "failed" };
        let title_line = Line::from(vec![
            "tool".magenta(),
            format!(" {fq_tool_name} ({status_str}, duration: {})", duration).dim(),
        ]);

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(title_line);
        lines.push(Line::from(format!("$ {invocation}")));

        // Convert result into serde_json::Value early so we don't have to
        // worry about lifetimes inside the match arm.
        let result_val = result.map(|r| {
            serde_json::to_value(r)
                .unwrap_or_else(|_| serde_json::Value::String("<serialization error>".into()))
        });

        if let Ok(res_val) = result_val {
            let json_pretty =
                serde_json::to_string_pretty(&res_val).unwrap_or_else(|_| res_val.to_string());
            let mut iter = json_pretty.lines();
            for raw in iter.by_ref().take(TOOL_CALL_MAX_LINES) {
                lines.push(Line::from(raw.to_string()).dim());
            }
            let remaining = iter.count();
            if remaining > 0 {
                lines.push(Line::from(format!("... {} additional lines", remaining)).dim());
            }
        }

        lines.push(Line::from(""));

        HistoryCell::CompletedMcpToolCallWithTextOutput {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_background_event(message: String) -> Self {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("event".dim()));
        lines.extend(message.lines().map(|l| Line::from(l.to_string()).dim()));
        lines.push(Line::from(""));
        HistoryCell::BackgroundEvent {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_error_event(message: String) -> Self {
        let lines: Vec<Line<'static>> = vec![
            vec!["ERROR: ".red().bold(), message.into()].into(),
            "".into(),
        ];
        HistoryCell::ErrorEvent {
            view: TextBlock::new(lines),
        }
    }

    /// Create a new `PendingPatch` cell that lists the file‑level summary of
    /// a proposed patch. The summary lines should already be formatted (e.g.
    /// "A path/to/file.rs").
    pub(crate) fn new_patch_event(
        event_type: PatchEventType,
        changes: HashMap<PathBuf, FileChange>,
    ) -> Self {
        let title = match event_type {
            PatchEventType::ApprovalRequest => "proposed patch",
            PatchEventType::ApplyBegin {
                auto_approved: true,
            } => "applying patch",
            PatchEventType::ApplyBegin {
                auto_approved: false,
            } => {
                let lines = vec![Line::from("patch applied".magenta().bold())];
                return Self::PendingPatch {
                    view: TextBlock::new(lines),
                };
            }
        };

        let summary_lines = create_diff_summary(changes);

        let mut lines: Vec<Line<'static>> = Vec::new();

        // Header similar to the command formatter so patches are visually
        // distinct while still fitting the overall colour scheme.
        lines.push(Line::from(title.magenta().bold()));

        for line in summary_lines {
            if line.starts_with('+') {
                lines.push(line.green().into());
            } else if line.starts_with('-') {
                lines.push(line.red().into());
            } else if let Some(space_idx) = line.find(' ') {
                let kind_owned = line[..space_idx].to_string();
                let rest_owned = line[space_idx + 1..].to_string();

                let style_for = |fg: Color| Style::default().fg(fg).add_modifier(Modifier::BOLD);

                let styled_kind = match kind_owned.as_str() {
                    "A" => RtSpan::styled(kind_owned.clone(), style_for(Color::Green)),
                    "D" => RtSpan::styled(kind_owned.clone(), style_for(Color::Red)),
                    "M" => RtSpan::styled(kind_owned.clone(), style_for(Color::Yellow)),
                    "R" | "C" => RtSpan::styled(kind_owned.clone(), style_for(Color::Cyan)),
                    _ => RtSpan::raw(kind_owned.clone()),
                };

                let styled_line =
                    RtLine::from(vec![styled_kind, RtSpan::raw(" "), RtSpan::raw(rest_owned)]);
                lines.push(styled_line);
            } else {
                lines.push(Line::from(line));
            }
        }

        lines.push(Line::from(""));

        HistoryCell::PendingPatch {
            view: TextBlock::new(lines),
        }
    }
}

// ---------------------------------------------------------------------------
// `CellWidget` implementation – most variants delegate to their internal
// `TextBlock`.  Variants that need custom painting can add their own logic in
// the match arms.
// ---------------------------------------------------------------------------

impl CellWidget for HistoryCell {
    fn height(&self, width: u16) -> usize {
        match self {
            HistoryCell::WelcomeMessage { view }
            | HistoryCell::UserPrompt { view }
            | HistoryCell::AgentMessage { view }
            | HistoryCell::AgentReasoning { view }
            | HistoryCell::BackgroundEvent { view }
            | HistoryCell::ErrorEvent { view }
            | HistoryCell::SessionInfo { view }
            | HistoryCell::CompletedExecCommand { view }
            | HistoryCell::CompletedMcpToolCallWithTextOutput { view }
            | HistoryCell::PendingPatch { view }
            | HistoryCell::ActiveExecCommand { view, .. }
            | HistoryCell::ActiveMcpToolCall { view, .. } => view.height(width),
            HistoryCell::CompletedMcpToolCallWithImageOutput { image } => {
                // For images, we use a fixed height based on the image size.
                // This is a simplification; ideally, we should calculate the
                // height based on the image's aspect ratio and the given width.
                let (_width, height) = image.dimensions();
                (height as f64 * 0.5).ceil() as usize // Scale down for better fit
            }
        }
    }

    fn render_window(&self, first_visible_line: usize, area: Rect, buf: &mut Buffer) {
        match self {
            HistoryCell::WelcomeMessage { view }
            | HistoryCell::UserPrompt { view }
            | HistoryCell::AgentMessage { view }
            | HistoryCell::AgentReasoning { view }
            | HistoryCell::BackgroundEvent { view }
            | HistoryCell::ErrorEvent { view }
            | HistoryCell::SessionInfo { view }
            | HistoryCell::CompletedExecCommand { view }
            | HistoryCell::CompletedMcpToolCallWithTextOutput { view }
            | HistoryCell::PendingPatch { view }
            | HistoryCell::ActiveExecCommand { view, .. }
            | HistoryCell::ActiveMcpToolCall { view, .. } => {
                view.render_window(first_visible_line, area, buf)
            }
            HistoryCell::CompletedMcpToolCallWithImageOutput { image } => {
                // For images, we render the image directly into the buffer.
                // This is a simplification; ideally, we should handle scaling
                // and centering based on the area size.
                // NOTE: The `ratatui_image` crate went through a few API iterations and the
                // currently-pinned version (v8) does not provide the
                // `Image::from_dynamic_image` convenience helper that older code relied on.
                //
                // To render the picture we now need to:
                // 1. Resize the raw `DynamicImage` so that it fits into the `area` that ratatui
                //    assigned to this cell.
                // 2. Build an appropriate `ratatui_image::protocol::Protocol` instance for the
                //    *current* terminal – the `picker` helper simplifies this.
                // 3. Create a stateless `ratatui_image::Image` widget from the protocol and let
                //    it write to the buffer.
                use ratatui_image::{picker::Picker, Resize as ImgResize};

                // Resize the image to the target width while keeping the aspect ratio.  We clamp
                // the target height to the area height to avoid overspill.
                let (orig_w, orig_h) = image.dimensions();
                if orig_w == 0 || orig_h == 0 || area.width == 0 || area.height == 0 {
                    return;
                }

                let target_w = area.width as u32;
                let scale = target_w as f64 / orig_w as f64;
                let mut target_h = (orig_h as f64 * scale).round() as u32;
                let max_h = area.height as u32;
                if target_h > max_h {
                    // Re-scale so the height fits.
                    let scale = max_h as f64 / orig_h as f64;
                    target_h = max_h;
                    // Keep width in sync with the new scale.
                    let _ = (scale * orig_w as f64).round() as u32;
                }

                let resized = image.resize(target_w, target_h, image::imageops::FilterType::Lanczos3);

                // Build a protocol suited for the active terminal.  We do not have font size info
                // here, but `Picker::from_fontsize` needs *some* value – a reasonable default is
                // fine for now because the widget will clip anything that exceeds `area`.
                let picker = Picker::from_fontsize((8, 16));

                if let Ok(protocol) = picker.new_protocol(resized, area, ImgResize::Fit(None)) {
                    let img_widget = TuiImage::new(&protocol);
                    img_widget.render(area, buf);
                }
            }
        }
    }
}

fn create_diff_summary(changes: HashMap<PathBuf, FileChange>) -> Vec<String> {
    // Build a concise, human‑readable summary list similar to the
    // `git status` short format so the user can reason about the
    // patch without scrolling.
    let mut summaries: Vec<String> = Vec::new();
    for (path, change) in &changes {
        use codex_core::protocol::FileChange::*;
        match change {
            Add { content } => {
                let added = content.lines().count();
                summaries.push(format!("A {} (+{added})", path.display()));
            }
            Delete => {
                summaries.push(format!("D {}", path.display()));
            }
            Update {
                unified_diff,
                move_path,
            } => {
                if let Some(new_path) = move_path {
                    summaries.push(format!("R {} → {}", path.display(), new_path.display(),));
                } else {
                    summaries.push(format!("M {}", path.display(),));
                }
                summaries.extend(unified_diff.lines().map(|s| s.to_string()));
            }
        }
    }

    summaries
}
