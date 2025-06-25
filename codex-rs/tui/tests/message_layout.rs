use tempfile::TempDir;
use codex_core::config::{Config, ConfigToml, ConfigOverrides};
use codex_tui::history_cell::HistoryCell;

/// Extract plain string content of each line for comparison.
fn lines_from_userprompt(view: &codex_tui::text_block::TextBlock) -> Vec<String> {
    view.lines
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.clone()).collect())
        .collect()
}

#[test]
fn test_user_message_layout_combinations() {
    let tmp = TempDir::new().unwrap();
    let mut config = Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        ConfigOverrides::default(),
        tmp.path().to_path_buf(),
    )
    .unwrap();
    let message = "first line\nsecond line".to_string();
    for &sender_break in &[false, true] {
        for &message_spacing in &[false, true] {
            config.tui.sender_break_line = sender_break;
            config.tui.message_spacing = message_spacing;
            let cell = HistoryCell::new_user_prompt(&config, message.clone());
            let view = match cell {
                HistoryCell::UserPrompt { view } => view,
                _ => panic!("expected UserPrompt variant"),
            };
            let got = lines_from_userprompt(&view);
            let mut expected = Vec::new();
            if sender_break {
                expected.push("user".to_string());
                expected.push("first line".to_string());
                expected.push("second line".to_string());
            } else {
                expected.push("user first line".to_string());
                expected.push(" second line".to_string());
            }
            if message_spacing {
                expected.push(String::new());
            }
            assert_eq!(got, expected,
                "Layout mismatch for sender_break_line={}, message_spacing={}",
                sender_break, message_spacing);
        }
    }
}

#[test]
fn test_agent_message_layout_combinations() {
    let tmp = TempDir::new().unwrap();
    let mut config = Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        ConfigOverrides::default(),
        tmp.path().to_path_buf(),
    )
    .unwrap();
    let message = "first line\nsecond line".to_string();
    for &sender_break in &[false, true] {
        for &message_spacing in &[false, true] {
            config.tui.sender_break_line = sender_break;
            config.tui.message_spacing = message_spacing;
            let cell = HistoryCell::new_agent_message(&config, message.clone());
            let view = match cell {
                HistoryCell::AgentMessage { view } => view,
                _ => panic!("expected AgentMessage variant"),
            };
            let got = lines_from_userprompt(&view);
            let mut expected = Vec::new();
            if sender_break {
                expected.push("codex".to_string());
                expected.push("first line".to_string());
                expected.push("second line".to_string());
            } else {
                expected.push("codex first line".to_string());
                expected.push(" second line".to_string());
            }
            if message_spacing {
                expected.push(String::new());
            }
            assert_eq!(got, expected,
                "Agent layout mismatch for sender_break_line={}, message_spacing={}",
                sender_break, message_spacing);
        }
    }
}

#[test]
fn test_agent_reasoning_layout_combinations() {
    let tmp = TempDir::new().unwrap();
    let mut config = Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        ConfigOverrides::default(),
        tmp.path().to_path_buf(),
    )
    .unwrap();
    let message = "first line\nsecond line".to_string();
    for &sender_break in &[false, true] {
        for &message_spacing in &[false, true] {
            config.tui.sender_break_line = sender_break;
            config.tui.message_spacing = message_spacing;
            let cell = HistoryCell::new_agent_reasoning(&config, message.clone());
            let view = match cell {
                HistoryCell::AgentReasoning { view } => view,
                _ => panic!("expected AgentReasoning variant"),
            };
            let got = lines_from_userprompt(&view);
            let mut expected = Vec::new();
            if sender_break {
                expected.push("thinking".to_string());
                expected.push("first line".to_string());
                expected.push("second line".to_string());
            } else {
                expected.push("thinking first line".to_string());
                expected.push(" second line".to_string());
            }
            if message_spacing {
                expected.push(String::new());
            }
            assert_eq!(got, expected,
                "Reasoning layout mismatch for sender_break_line={}, message_spacing={}",
                sender_break, message_spacing);
        }
    }
}
