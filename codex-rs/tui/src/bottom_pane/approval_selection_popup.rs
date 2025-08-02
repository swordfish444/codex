use codex_core::protocol::AskForApproval;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

const MAX_RESULTS: usize = 6;

use super::selection_list::{SelectionItem, SelectionList};
use super::selection_popup_common::{render_rows, GenericDisplayRow};

/// Popup for selecting the approval mode at runtime.
pub(crate) struct ApprovalSelectionPopup {
    list: SelectionList<AskForApproval>,
}

impl ApprovalSelectionPopup {
    pub(crate) fn new(current: AskForApproval, mut options: Vec<AskForApproval>) -> Self {
        options.dedup();
        let items = build_items(current, options);
        Self { list: SelectionList::new(items) }
    }

    pub(crate) fn set_options(&mut self, current: AskForApproval, mut options: Vec<AskForApproval>) {
        options.dedup();
        self.list.set_items(build_items(current, options));
    }

    pub(crate) fn set_query(&mut self, query: &str) { self.list.set_query(query); }

    pub(crate) fn move_up(&mut self) { self.list.move_up(); }

    pub(crate) fn move_down(&mut self) { self.list.move_down(); }

    pub(crate) fn selected_mode(&self) -> Option<AskForApproval> { self.list.selected_value() }

    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.visible_rows().len().clamp(1, MAX_RESULTS) as u16
    }

    fn visible_rows(&self) -> Vec<GenericDisplayRow> {
        self.list.visible_rows().into_iter().map(|(row, _)| row).collect()
    }
}

fn display_name(mode: AskForApproval) -> &'static str {
    match mode {
        AskForApproval::UnlessTrusted => "Prompt on Writes",
        AskForApproval::OnFailure => "Auto",
        AskForApproval::Never => "Deny all",
    }
}

fn description_for(mode: AskForApproval) -> &'static str {
    match mode {
        AskForApproval::UnlessTrusted =>
            "ask for approval for every write in the CWD and every sandbox breach",
        AskForApproval::OnFailure =>
            "only ask for commands that would breach the sandbox",
        AskForApproval::Never =>
            "deny all writes and commands that would breach the sandbox",
    }
}

// Internal rows are produced by the generic SelectionList.

impl WidgetRef for &ApprovalSelectionPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let rows_all: Vec<GenericDisplayRow> = self.visible_rows();
        render_rows(area, buf, &rows_all, &self.list.state, MAX_RESULTS);
    }
}

/// Parse a free‑form string and try to map it to an approval mode.
pub(crate) fn parse_approval_mode_token(s: &str) -> Option<AskForApproval> {
    let t = s.trim().to_ascii_lowercase();
    match t.as_str() {
        "untrusted" | "prompt-on-writes" | "prompt on writes" => Some(AskForApproval::UnlessTrusted),
        "on-failure" | "auto" | "full-auto" | "fullauto" | "full" => Some(AskForApproval::OnFailure),
        "never" | "deny-all" | "deny all" => Some(AskForApproval::Never),
        _ => None,
    }
}

fn aliases_for(mode: AskForApproval) -> &'static [&'static str] {
    match mode {
        AskForApproval::UnlessTrusted => &["untrusted", "prompt-on-writes", "prompt on writes"],
        AskForApproval::OnFailure => &["auto", "full-auto", "on-failure", "fullauto", "full"],
        AskForApproval::Never => &["never", "deny-all", "deny all"],
    }
}

fn build_items(
    current: AskForApproval,
    options: Vec<AskForApproval>,
) -> Vec<SelectionItem<AskForApproval>> {
    let mut items: Vec<SelectionItem<AskForApproval>> = Vec::new();
    let current_item = SelectionItem::new(current, display_name(current).to_string())
        .with_description(Some(description_for(current).to_string()))
        .with_aliases(aliases_for(current).iter().map(|s| s.to_string()).collect())
        .mark_current(true);
    items.push(current_item);
    for m in options.into_iter().filter(|m| *m != current) {
        items.push(
            SelectionItem::new(m, display_name(m).to_string())
                .with_description(Some(description_for(m).to_string()))
                .with_aliases(aliases_for(m).iter().map(|s| s.to_string()).collect()),
        );
    }
    items
}
