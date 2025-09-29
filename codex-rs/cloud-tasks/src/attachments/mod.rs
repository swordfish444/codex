pub mod upload;

pub use upload::AttachmentAssetPointer;
pub use upload::AttachmentId;
pub use upload::AttachmentUploadError;
pub use upload::AttachmentUploadMode;
pub use upload::AttachmentUploadProgress;
pub use upload::AttachmentUploadState;
pub use upload::AttachmentUploadUpdate;
pub use upload::AttachmentUploader;
pub use upload::HttpConfig as AttachmentUploadHttpConfig;
pub use upload::pointer_id_from_value;

use serde::Deserialize;
use serde::Serialize;

const MAX_SUGGESTIONS: usize = 5;

/// The type of attachment included alongside a composer submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentKind {
    File,
    Image,
}

/// Metadata describing a file or asset attached via an `@` mention.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposerAttachment {
    pub kind: AttachmentKind,
    pub label: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fs_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip, default)]
    pub id: AttachmentId,
    #[serde(skip_serializing, skip_deserializing)]
    pub upload: AttachmentUploadState,
}

impl ComposerAttachment {
    pub fn from_suggestion(id: AttachmentId, suggestion: &MentionSuggestion) -> Self {
        Self {
            kind: AttachmentKind::File,
            label: suggestion.label.clone(),
            path: suggestion.path.clone(),
            fs_path: suggestion.fs_path.clone(),
            start_line: suggestion.start_line,
            end_line: suggestion.end_line,
            id,
            upload: AttachmentUploadState::default(),
        }
    }
}

/// UI state for the active `@` mention query inside the composer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MentionQueryState {
    pub current: Option<MentionToken>,
}

impl MentionQueryState {
    /// Returns true when the stored token changed.
    pub fn update_from(&mut self, token: Option<String>) -> bool {
        let next = token.map(MentionToken::from_query);
        if next != self.current {
            self.current = next;
            return true;
        }
        false
    }
}

/// Represents an `@` mention currently under the user's cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionToken {
    /// Query string without the leading `@`.
    pub query: String,
    /// Raw token including the `@` prefix.
    pub raw: String,
}

impl MentionToken {
    pub(crate) fn from_query(query: String) -> Self {
        let raw = format!("@{query}");
        Self { query, raw }
    }
}

/// A suggested file (or range within a file) that matches the active `@` token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionSuggestion {
    pub label: String,
    pub path: String,
    pub fs_path: Option<String>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
}

impl MentionSuggestion {
    pub fn new(label: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            path: path.into(),
            fs_path: None,
            start_line: None,
            end_line: None,
        }
    }
}

/// Tracks suggestion list + selection for the mention picker overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MentionPickerState {
    suggestions: Vec<MentionSuggestion>,
    selected: usize,
}

impl MentionPickerState {
    pub fn clear(&mut self) -> bool {
        if self.suggestions.is_empty() {
            return false;
        }
        self.suggestions.clear();
        self.selected = 0;
        true
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.suggestions.is_empty() {
            return;
        }
        let len = self.suggestions.len() as isize;
        let mut idx = self.selected as isize + delta;
        if idx < 0 {
            idx = len - 1;
        }
        if idx >= len {
            idx = 0;
        }
        self.selected = idx as usize;
    }

    pub fn selected_index(&self) -> usize {
        self.selected.min(self.suggestions.len().saturating_sub(1))
    }

    pub fn current(&self) -> Option<&MentionSuggestion> {
        self.suggestions.get(self.selected_index())
    }

    pub fn render_height(&self) -> u16 {
        let rows = self.suggestions.len().clamp(1, MAX_SUGGESTIONS) as u16;
        // Add borders + padding space.
        rows.saturating_add(2)
    }

    pub fn items(&self) -> &[MentionSuggestion] {
        &self.suggestions
    }

    pub fn set_suggestions(&mut self, suggestions: Vec<MentionSuggestion>) -> bool {
        let mut trimmed = suggestions;
        if trimmed.len() > MAX_SUGGESTIONS {
            trimmed.truncate(MAX_SUGGESTIONS);
        }
        if trimmed == self.suggestions {
            return false;
        }
        self.suggestions = trimmed;
        self.selected = 0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::AttachmentUploadState;
    use super::*;

    #[test]
    fn compose_attachment_from_suggestion_copies_fields() {
        let mut suggestion = MentionSuggestion::new("src/main.rs", "src/main.rs");
        suggestion.fs_path = Some("/repo/src/main.rs".to_string());
        suggestion.start_line = Some(10);
        suggestion.end_line = Some(20);
        let att = ComposerAttachment::from_suggestion(AttachmentId::new(42), &suggestion);
        assert_eq!(att.label, "src/main.rs");
        assert_eq!(att.path, "src/main.rs");
        assert_eq!(att.fs_path.as_deref(), Some("/repo/src/main.rs"));
        assert_eq!(att.start_line, Some(10));
        assert_eq!(att.end_line, Some(20));
        assert!(matches!(att.upload, AttachmentUploadState::NotStarted));
        assert_eq!(att.id.raw(), 42);
    }
    #[test]
    fn move_selection_wraps() {
        let _token = MentionToken::from_query("foo".to_string());
        let mut picker = MentionPickerState::default();
        assert!(picker.set_suggestions(vec![
            MentionSuggestion::new("src/foo.rs", "src/foo.rs"),
            MentionSuggestion::new("src/main.rs", "src/main.rs"),
        ]));
        picker.move_selection(1);
        assert_eq!(
            picker.selected_index(),
            1.min(picker.items().len().saturating_sub(1))
        );
        picker.move_selection(-1);
        assert_eq!(picker.selected_index(), 0);
    }

    #[test]
    fn refresh_none_clears_suggestions() {
        let _token = MentionToken::from_query("bar".to_string());
        let mut picker = MentionPickerState::default();
        assert!(
            picker.set_suggestions(vec![MentionSuggestion::new("docs/bar.md", "docs/bar.md",)])
        );
        assert!(picker.clear());
        assert!(picker.items().is_empty());
    }
}
