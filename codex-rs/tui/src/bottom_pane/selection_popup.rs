use codex_core::protocol::AskForApproval;
use codex_core::protocol::SandboxPolicy;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use super::selection_list::SelectionItem;
use super::selection_list::SelectionList;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionKind { Model, Execution }

#[derive(Clone)]
pub(crate) enum SelectionValue {
    Model(String),
    Execution { approval: AskForApproval, sandbox: SandboxPolicy },
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

    pub(crate) fn new_execution_modes(
        current_approval: AskForApproval,
        current_sandbox: &SandboxPolicy,
    ) -> Self {
        fn display_name(approval: AskForApproval, sandbox: &SandboxPolicy) -> &'static str {
            match (approval, sandbox) {
                (AskForApproval::Never, SandboxPolicy::ReadOnly) => "Read only",
                (AskForApproval::OnFailure, SandboxPolicy::ReadOnly) => "Untrusted",
                (AskForApproval::OnFailure, SandboxPolicy::WorkspaceWrite { .. }) => "Auto",
                _ => "Custom",
            }
        }
        fn description_for(approval: AskForApproval, sandbox: &SandboxPolicy) -> &'static str {
            match (approval, sandbox) {
                (AskForApproval::Never, SandboxPolicy::ReadOnly) =>
                    "never prompt; read-only filesystem (flags: --ask-for-approval never --sandbox read-only)",
                (AskForApproval::OnFailure, SandboxPolicy::ReadOnly) =>
                    "ask to retry outside sandbox only on sandbox breach; read-only (flags: --ask-for-approval on-failure --sandbox read-only)",
                (AskForApproval::OnFailure, SandboxPolicy::WorkspaceWrite { .. }) =>
                    "auto in workspace sandbox; ask to retry outside sandbox on breach (flags: --ask-for-approval on-failure --sandbox workspace-write)",
                _ => "custom combination",
            }
        }

        let presets: Vec<(AskForApproval, SandboxPolicy)> = vec![
            (AskForApproval::Never, SandboxPolicy::ReadOnly),
            (AskForApproval::OnFailure, SandboxPolicy::ReadOnly),
            (
                AskForApproval::OnFailure,
                SandboxPolicy::WorkspaceWrite {
                    writable_roots: vec![],
                    network_access: false,
                    include_default_writable_roots: true,
                },
            ),
        ];

        let mut items: Vec<SelectionItem<SelectionValue>> = Vec::new();
        for (a, s) in presets.into_iter() {
            let name = display_name(a, &s).to_string();
            let desc = Some(description_for(a, &s).to_string());
            let mut item = SelectionItem::new(
                SelectionValue::Execution {
                    approval: a,
                    sandbox: s.clone(),
                },
                name,
            )
            .with_description(desc);
            if a == current_approval
                && matches!(
                    (&s, current_sandbox),
                    (SandboxPolicy::ReadOnly, SandboxPolicy::ReadOnly)
                        | (SandboxPolicy::WorkspaceWrite { .. }, SandboxPolicy::WorkspaceWrite { .. })
                )
            {
                item = item.mark_current(true);
            }
            items.push(item);
        }
        Self { kind: SelectionKind::Execution, list: SelectionList::new(items) }
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

/// Parse a free-form token to an execution preset (approval+sandbox).
pub(crate) fn parse_execution_mode_token(
    s: &str,
) -> Option<(AskForApproval, SandboxPolicy)> {
    let t = s.trim().to_ascii_lowercase();
    match t.as_str() {
        "read-only" => Some((AskForApproval::Never, SandboxPolicy::ReadOnly)),
        "untrusted" => Some((AskForApproval::OnFailure, SandboxPolicy::ReadOnly)),
        "auto" => Some((
            AskForApproval::OnFailure,
            SandboxPolicy::WorkspaceWrite {
                writable_roots: vec![],
                network_access: false,
                include_default_writable_roots: true,
            },
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_execution_mode_token as parse;
    use codex_core::protocol::AskForApproval;
    use codex_core::protocol::SandboxPolicy;

    #[test]
    fn parse_approval_mode_aliases() {
        // Only accept the three canonical tokens
        assert!(matches!(parse("auto").unwrap(), (AskForApproval::OnFailure, SandboxPolicy::WorkspaceWrite { .. })));
        assert_eq!(parse("untrusted"), Some((AskForApproval::OnFailure, SandboxPolicy::ReadOnly)));
        assert_eq!(parse("read-only"), Some((AskForApproval::Never, SandboxPolicy::ReadOnly)));
        // Unknown and case/whitespace handling
        assert_eq!(parse("unknown"), None);
        assert_eq!(parse("  AUTO  ").is_some(), true);
    }
}
