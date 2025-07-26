use std::path::PathBuf;
use std::sync::mpsc::channel;

use codex_core::protocol::ReviewDecision; // keep this import to ensure the enum is linked
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use codex_tui::AppEvent;
use codex_tui::AppEventSender;
use codex_tui::ApprovalRequest;
use codex_tui::UserApprovalWidget;

/// Regression test: ensure that the exec command line (cwd + command) is always
/// visible when rendering the approval modal, even when the available viewport
/// is very small and the confirmation prompt needs to be truncated.
#[test]
fn exec_command_is_visible_in_small_viewport() {
    // Construct a long reason to inflate the confirmation prompt, so we can
    // verify that it gets truncated rather than pushing the command or the
    // response options out of view.
    let long_reason = "This is a very long explanatory reason that would normally occupy many lines in the confirmation prompt. \
It should not cause the actual command or the response options to be scrolled out of the visible area.".to_string();

    // Minimal AppEventSender – the widget will send an event when a decision is
    // made, but we will not interact with it in this rendering test.
    let (tx, _rx) = channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);

    let cwd = PathBuf::from("/home/alice/project");
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "echo 123 && printf 'hello'".to_string(),
    ];

    let widget = UserApprovalWidget::new(
        ApprovalRequest::Exec {
            id: "test-id".to_string(),
            command: command.clone(),
            cwd: cwd.clone(),
            reason: Some(long_reason),
        },
        app_tx,
    );

    // Render into a deliberately small area to force truncation of the prompt.
    // The outer border consumes one row/column on each side, leaving very
    // little space inside – this is fine, we just want to see that the command
    // line is still present in the buffer.
    let area = Rect::new(0, 0, 50, 12);
    let mut buf = Buffer::empty(area);
    (&widget).render_ref(area, &mut buf);

    // Extract the contents of the buffer into a single string for searching.
    let mut rendered = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = buf.get(x, y);
            rendered.push(cell.symbol().chars().next().unwrap_or('\0'));
        }
        rendered.push('\n');
    }

    // The command is displayed after stripping the leading "bash -lc" wrapper
    // and shell-escaping. For our input this should result in the literal
    // command text below. The cwd is printed as a prefix ending with a '$', but
    // to keep the assertion robust we only verify that the command itself is
    // present in the rendered buffer.
    assert!(
        rendered.contains("echo 123 && printf 'hello'"),
        "rendered buffer did not contain the command.\n--- buffer ---\n{rendered}\n----------------"
    );

    // Additionally assert that at least one of the response options is visible
    // to ensure the interactive section has not been pushed out of view.
    assert!(rendered.contains("Yes (y)"));

    // Silence unused import warning for ReviewDecision (ensures the crate is
    // linked with the protocol module and avoids accidental dead-code removal).
    let _ = ReviewDecision::Approved;
}
