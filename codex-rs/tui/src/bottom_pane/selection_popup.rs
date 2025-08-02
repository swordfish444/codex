use codex_core::protocol::AskForApproval;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use super::selection_list::SelectionItem;
use super::selection_list::SelectionList;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionKind {
    Model,
    Approval,
}

#[derive(Clone)]
pub(crate) enum SelectionValue {
    Model(String),
    Approval(AskForApproval),
}

pub(crate) struct SelectionPopup {
    kind: SelectionKind,
    list: SelectionList<SelectionValue>,
}

const MAX_RESULTS: usize = 8;

impl SelectionPopup {
    pub(crate) fn new_model(current_model: &str, options: Vec<String>) -> Self {
        let mut items: Vec<SelectionItem<SelectionValue>> = Vec::new();
        items.push(
            SelectionItem::new(
                SelectionValue::Model(current_model.to_string()),
                current_model.to_string(),
            )
            .mark_current(true),
        );
        for m in options.into_iter().filter(|m| m != current_model) {
            items.push(SelectionItem::new(SelectionValue::Model(m.clone()), m));
        }
        Self {
            kind: SelectionKind::Model,
            list: SelectionList::new(items),
        }
    }

    pub(crate) fn new_approvals(current: AskForApproval, options: Vec<AskForApproval>) -> Self {
        fn display_name(mode: AskForApproval) -> &'static str {
            match mode {
                AskForApproval::UnlessTrusted => "Prompt on Writes",
                AskForApproval::OnFailure => "Auto",
                AskForApproval::Never => "Deny all",
            }
        }
        fn description_for(mode: AskForApproval) -> &'static str {
            match mode {
                AskForApproval::UnlessTrusted => {
                    "ask for approval for every write in the CWD and every sandbox breach"
                }
                AskForApproval::OnFailure => "only ask for commands that would breach the sandbox",
                AskForApproval::Never => {
                    "deny all writes and commands that would breach the sandbox"
                }
            }
        }
        fn aliases_for(mode: AskForApproval) -> &'static [&'static str] {
            match mode {
                AskForApproval::UnlessTrusted => {
                    &["untrusted", "prompt-on-writes", "prompt on writes"]
                }
                AskForApproval::OnFailure => {
                    &["auto", "full-auto", "on-failure", "fullauto", "full"]
                }
                AskForApproval::Never => &["never", "deny-all", "deny all"],
            }
        }

        let mut items: Vec<SelectionItem<SelectionValue>> = Vec::new();
        items.push(
            SelectionItem::new(
                SelectionValue::Approval(current),
                display_name(current).to_string(),
            )
            .with_description(Some(description_for(current).to_string()))
            .with_aliases(aliases_for(current).iter().map(|s| s.to_string()).collect())
            .mark_current(true),
        );
        for m in options.into_iter().filter(|m| *m != current) {
            items.push(
                SelectionItem::new(SelectionValue::Approval(m), display_name(m).to_string())
                    .with_description(Some(description_for(m).to_string()))
                    .with_aliases(aliases_for(m).iter().map(|s| s.to_string()).collect()),
            );
        }
        Self {
            kind: SelectionKind::Approval,
            list: SelectionList::new(items),
        }
    }

    pub(crate) fn kind(&self) -> SelectionKind {
        self.kind
    }

    pub(crate) fn set_query(&mut self, query: &str) {
        self.list.set_query(query);
    }
    pub(crate) fn move_up(&mut self) {
        self.list.move_up();
    }
    pub(crate) fn move_down(&mut self) {
        self.list.move_down();
    }
    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.list.visible_rows().len().clamp(1, MAX_RESULTS) as u16
    }
    pub(crate) fn selected_value(&self) -> Option<SelectionValue> {
        self.list.selected_value()
    }

    fn visible_rows(&self) -> Vec<GenericDisplayRow> {
        self.list
            .visible_rows()
            .into_iter()
            .map(|(row, _)| row)
            .collect()
    }
}

impl WidgetRef for &SelectionPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let rows_all = self.visible_rows();
        render_rows(area, buf, &rows_all, &self.list.state, MAX_RESULTS);
    }
}

/// Parse a free-form token to an approval mode. Used by typed /approvals.
pub(crate) fn parse_approval_mode_token(s: &str) -> Option<AskForApproval> {
    let t = s.trim().to_ascii_lowercase();
    match t.as_str() {
        "untrusted" | "prompt-on-writes" | "prompt on writes" => {
            Some(AskForApproval::UnlessTrusted)
        }
        "on-failure" | "auto" | "full-auto" | "fullauto" | "full" => {
            Some(AskForApproval::OnFailure)
        }
        "never" | "deny-all" | "deny all" => Some(AskForApproval::Never),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_approval_mode_token as parse;
    use codex_core::protocol::AskForApproval;

    #[test]
    fn parse_approval_mode_aliases() {
        // OnFailure
        for t in ["auto", "full-auto", "on-failure", "fullauto", "full"] {
            assert_eq!(parse(t), Some(AskForApproval::OnFailure), "{t}");
        }
        // UnlessTrusted
        for t in ["untrusted", "prompt-on-writes", "prompt on writes"] {
            assert_eq!(parse(t), Some(AskForApproval::UnlessTrusted), "{t}");
        }
        // Never
        for t in ["never", "deny-all", "deny all"] {
            assert_eq!(parse(t), Some(AskForApproval::Never), "{t}");
        }
        // Unknown
        assert_eq!(parse("unknown"), None);
        // Whitespace and case-insensitivity
        assert_eq!(parse("  FULL-AUTO  "), Some(AskForApproval::OnFailure));
    }
}
