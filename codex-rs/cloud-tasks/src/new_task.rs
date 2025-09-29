use crate::attachments::AttachmentAssetPointer;
use crate::attachments::AttachmentId;
use crate::attachments::AttachmentKind;
use crate::attachments::AttachmentUploadError;
use crate::attachments::AttachmentUploadMode;
use crate::attachments::AttachmentUploadProgress;
use crate::attachments::AttachmentUploadState;
use crate::attachments::AttachmentUploadUpdate;
use crate::attachments::AttachmentUploader;
use crate::attachments::ComposerAttachment;
use crate::attachments::MentionPickerState;
use crate::attachments::MentionQueryState;
use crate::attachments::MentionSuggestion;
use crate::attachments::upload::AttachmentPointerKind;
use crate::util::append_error_log;
use codex_tui::ComposerInput;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use throbber_widgets_tui::ThrobberState;
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitPhase {
    Idle,
    WaitingForUploads,
    Sending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmissionAction {
    StartSending {
        prompt: String,
        attachments: Vec<AttachmentSubmission>,
    },
    WaitForUploads,
    Blocked(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionPayload {
    pub prompt: String,
    pub attachments: Vec<AttachmentSubmission>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachmentUploadDisplay {
    Pending,
    Uploaded,
    Failed(String),
}

pub struct NewTaskPage {
    pub composer: ComposerInput,
    pub submitting: bool,
    pub env_id: Option<String>,
    #[allow(dead_code)]
    pub attachments: Vec<ComposerAttachment>,
    attachment_uploader: AttachmentUploader,
    next_attachment_id: u64,
    pub mention_state: MentionQueryState,
    pub mention_picker: MentionPickerState,
    pub mention_search: MentionSearchEngine,
    pub mention_search_pending: bool,
    submit_phase: SubmitPhase,
    pending_submit_body: Option<String>,
    submit_throbber: ThrobberState,
}

impl NewTaskPage {
    pub fn new(env_id: Option<String>, upload_mode: AttachmentUploadMode) -> Self {
        let mut composer = ComposerInput::new();
        composer.set_hint_items(vec![
            ("⏎", "send"),
            ("Shift+⏎", "newline"),
            ("Ctrl+O", "env"),
            ("Ctrl+C", "quit"),
        ]);
        Self {
            composer,
            submitting: false,
            env_id,
            attachments: Vec::new(),
            attachment_uploader: AttachmentUploader::new(upload_mode),
            next_attachment_id: 1,
            mention_state: MentionQueryState::default(),
            mention_picker: MentionPickerState::default(),
            mention_search: MentionSearchEngine::new(),
            mention_search_pending: false,
            submit_phase: SubmitPhase::Idle,
            pending_submit_body: None,
            submit_throbber: ThrobberState::default(),
        }
    }

    /// Sync the mention query + suggestions with the current cursor token.
    /// Returns true when the UI needs to refresh.
    pub fn refresh_mention_state(&mut self) -> MentionRefresh {
        let token = self.composer.mention_token();
        let changed = self.mention_state.update_from(token);
        let mut picker_changed = false;
        let mut search_requested = false;
        if changed {
            match self.mention_state.current.as_ref() {
                Some(tok) if tok.query.is_empty() => {
                    self.mention_search.cancel();
                    picker_changed |= self.mention_picker.clear();
                    self.mention_search_pending = false;
                }
                Some(tok) => {
                    self.mention_search_pending = self.mention_search.request(tok.query.clone());
                    search_requested = self.mention_search_pending;
                }
                None => {
                    self.mention_search.cancel();
                    picker_changed |= self.mention_picker.clear();
                    self.mention_search_pending = false;
                }
            }
        }
        MentionRefresh {
            state_changed: changed || picker_changed,
            search_requested,
        }
    }

    pub fn mention_active(&self) -> bool {
        self.mention_state.current.is_some() || self.mention_search_pending
    }

    pub fn move_mention_selection(&mut self, delta: isize) {
        self.mention_picker.move_selection(delta);
    }

    pub fn cancel_mention(&mut self) {
        let _ = self.mention_picker.clear();
        let _ = self.mention_state.update_from(None);
        self.mention_search.cancel();
        self.mention_search_pending = false;
    }

    /// Apply the currently selected suggestion, updating the composer text and
    /// capturing an attachment entry. Returns true when a suggestion was applied.
    pub fn accept_current_mention(&mut self) -> bool {
        let suggestion = match self.mention_picker.current() {
            Some(s) => s.clone(),
            None => return false,
        };

        let mention_text = format!("[{}]", suggestion.path);
        self.composer.replace_current_token(&mention_text);
        append_error_log(format!(
            "mention.accepted label={} path={}",
            suggestion.label, suggestion.path
        ));
        let attachment = self.build_attachment_from_suggestion(&suggestion);
        self.attachments.push(attachment);
        let _ = self.mention_picker.clear();
        let _ = self
            .mention_state
            .update_from(self.composer.mention_token());
        self.mention_search.cancel();
        self.mention_search_pending = false;
        true
    }

    /// Render attachment metadata into a human-readable header that mirrors the
    /// desktop client context. The returned string includes the original body.
    pub fn compose_prompt_with_attachments(&self, body: &str) -> String {
        let referenced = self.referenced_attachment_indices(body);
        if referenced.is_empty() {
            return body.to_string();
        }
        let mut out = String::new();
        out.push_str("# Files mentioned by the user:\n");
        for idx in referenced {
            let att = &self.attachments[idx];
            out.push_str("- ");
            out.push_str(&att.label);
            out.push_str(": ");
            out.push_str(&att.path);
            if let Some(start) = att.start_line {
                let line_info = match att.end_line {
                    Some(end) if end > start => format!(" (lines {start}-{end})"),
                    _ => format!(" (line {start})"),
                };
                out.push_str(&line_info);
            }
            match &att.upload {
                AttachmentUploadState::Uploaded(success) => {
                    out.push_str(&format!(" (uploaded as {})", success.display_name));
                }
                AttachmentUploadState::Uploading(_) | AttachmentUploadState::NotStarted => {
                    out.push_str(" (upload pending)");
                }
                AttachmentUploadState::Failed(err) => {
                    out.push_str(&format!(" (upload failed: {})", err.message));
                }
            }
            out.push('\n');
        }
        out.push('\n');
        out.push_str(body);
        out
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn clear_attachments(&mut self) {
        self.attachment_uploader.cancel_all();
        self.attachments.clear();
        self.next_attachment_id = 1;
    }

    pub fn prepare_submission(&mut self, body: String) -> SubmissionAction {
        if self.submit_phase != SubmitPhase::Idle {
            return SubmissionAction::Blocked("Submission already in progress".to_string());
        }

        let referenced = self.referenced_attachment_indices(&body);

        if let Some(&failed_idx) = referenced
            .iter()
            .find(|&&idx| self.attachments[idx].upload.is_failed())
        {
            let failed = &self.attachments[failed_idx];
            return SubmissionAction::Blocked(format!(
                "Attachment upload failed for {}. Remove it and retry.",
                failed.label
            ));
        }

        if referenced
            .iter()
            .any(|&idx| self.attachments[idx].upload.is_pending())
        {
            self.pending_submit_body = Some(body);
            self.set_submit_phase(SubmitPhase::WaitingForUploads);
            return SubmissionAction::WaitForUploads;
        }

        self.set_submit_phase(SubmitPhase::Sending);
        let prompt = self.compose_prompt_with_attachments(&body);
        let attachments = self.uploaded_attachments(&body);
        SubmissionAction::StartSending {
            prompt,
            attachments,
        }
    }

    pub fn take_ready_submission(&mut self) -> Option<SubmissionPayload> {
        if self.submit_phase != SubmitPhase::WaitingForUploads {
            return None;
        }

        let body = match self.pending_submit_body.as_ref() {
            Some(body) => body,
            None => return None,
        };

        let referenced = self.referenced_attachment_indices(body);

        if referenced
            .iter()
            .any(|&idx| self.attachments[idx].upload.is_pending())
        {
            return None;
        }

        if let Some(&failed_idx) = referenced
            .iter()
            .find(|&&idx| self.attachments[idx].upload.is_failed())
        {
            let failed_label = self.attachments[failed_idx].label.clone();
            let failed_id = self.attachments[failed_idx].id.raw();
            self.reset_submission_state();
            append_error_log(format!(
                "submission aborted: attachment failed id={failed_id} label={failed_label}"
            ));
            return None;
        }

        let body = self.pending_submit_body.take().unwrap_or_default();
        self.set_submit_phase(SubmitPhase::Sending);
        let prompt = self.compose_prompt_with_attachments(&body);
        let attachments = self.uploaded_attachments(&body);
        Some(SubmissionPayload {
            prompt,
            attachments,
        })
    }

    pub fn reset_submission_state(&mut self) {
        self.pending_submit_body = None;
        self.set_submit_phase(SubmitPhase::Idle);
    }

    pub fn submit_phase(&self) -> SubmitPhase {
        self.submit_phase
    }

    pub fn is_submitting(&self) -> bool {
        !matches!(self.submit_phase, SubmitPhase::Idle)
    }

    pub fn pending_upload_count(&self) -> usize {
        self.attachments
            .iter()
            .filter(|att| att.upload.is_pending())
            .count()
    }

    pub fn has_pending_referenced_uploads(&self) -> bool {
        let Some(body) = self.pending_submit_body.as_ref() else {
            return false;
        };
        self.referenced_attachment_indices(body)
            .iter()
            .any(|&idx| self.attachments[idx].upload.is_pending())
    }

    pub fn attachment_display_items(&self) -> Vec<(String, AttachmentUploadDisplay)> {
        self.attachments
            .iter()
            .map(|att| {
                let label = att.label.clone();
                let state = if att.upload.is_uploaded() {
                    AttachmentUploadDisplay::Uploaded
                } else if let AttachmentUploadState::Failed(err) = &att.upload {
                    AttachmentUploadDisplay::Failed(err.message.clone())
                } else {
                    AttachmentUploadDisplay::Pending
                };
                (label, state)
            })
            .collect()
    }

    pub fn prune_unreferenced_attachments(&mut self) {
        let text = self.composer.text_content();
        let referenced_ids: HashSet<AttachmentId> = self
            .referenced_attachment_indices(&text)
            .into_iter()
            .map(|idx| self.attachments[idx].id)
            .collect();
        if referenced_ids.len() == self.attachments.len() {
            return;
        }
        let before = self.attachments.len();
        if before == referenced_ids.len() {
            return;
        }
        if before > referenced_ids.len() {
            for att in self
                .attachments
                .iter()
                .filter(|att| !referenced_ids.contains(&att.id))
            {
                append_error_log(format!(
                    "mention.attachment.removed id={} label={}",
                    att.id.raw(),
                    att.label
                ));
            }
        }
        self.attachments
            .retain(|att| referenced_ids.contains(&att.id));
    }

    pub fn submit_throbber_mut(&mut self) -> &mut ThrobberState {
        &mut self.submit_throbber
    }

    pub fn poll_attachment_uploads(&mut self) -> AttachmentUploadPoll {
        let mut state_changed = false;
        let mut failure: Option<String> = None;

        for update in self.attachment_uploader.poll() {
            match update {
                AttachmentUploadUpdate::Started { id, total_bytes } => {
                    if let Some(att) = self.find_attachment_mut(id) {
                        att.upload = AttachmentUploadState::Uploading(
                            AttachmentUploadProgress::new(0, total_bytes),
                        );
                        state_changed = true;
                    }
                }
                AttachmentUploadUpdate::Finished { id, result } => {
                    if let Some(att) = self.find_attachment_mut(id) {
                        att.upload = match result {
                            Ok(success) => {
                                append_error_log(format!(
                                    "mention.attachment.upload.completed id={}",
                                    att.id.raw()
                                ));
                                AttachmentUploadState::Uploaded(success)
                            }
                            Err(err) => {
                                let reason = err.message.clone();
                                append_error_log(format!(
                                    "mention.attachment.upload.failed id={} reason={reason}",
                                    att.id.raw()
                                ));
                                if failure.is_none() {
                                    failure = Some(format!(
                                        "Attachment upload failed for {}.",
                                        att.label
                                    ));
                                }
                                AttachmentUploadState::Failed(err)
                            }
                        };
                        state_changed = true;
                    }
                }
            }
        }

        let has_pending = self.attachments.iter().any(|att| att.upload.is_pending());

        if failure.is_some() && matches!(self.submit_phase, SubmitPhase::WaitingForUploads) {
            self.reset_submission_state();
        }

        AttachmentUploadPoll {
            state_changed,
            has_pending,
            failed: failure,
        }
    }

    pub fn poll_mention_search(&mut self) -> bool {
        let mut changed = false;
        let was_pending = self.mention_search_pending;
        while let Some(result) = self.mention_search.poll() {
            if let Some(current) = self.mention_state.current.as_ref() {
                if !current.query.starts_with(&result.query)
                    && !result.query.starts_with(&current.query)
                {
                    continue;
                }
            } else {
                continue;
            }
            changed |= self.mention_picker.set_suggestions(result.matches);
            self.mention_search_pending = false;
        }
        if was_pending && !self.mention_search_pending {
            changed = true;
        }
        changed
    }

    // Additional helpers can be added as usage evolves.

    fn next_attachment_id(&mut self) -> AttachmentId {
        let next = self.next_attachment_id;
        self.next_attachment_id = self.next_attachment_id.wrapping_add(1).max(1);
        AttachmentId::new(next)
    }

    fn build_attachment_from_suggestion(
        &mut self,
        suggestion: &MentionSuggestion,
    ) -> ComposerAttachment {
        let id = self.next_attachment_id();
        let mut attachment = ComposerAttachment::from_suggestion(id, suggestion);
        self.start_upload_for_attachment(&mut attachment);
        attachment
    }

    fn start_upload_for_attachment(&mut self, attachment: &mut ComposerAttachment) {
        let Some(fs_path) = attachment.fs_path.clone() else {
            let message = "file path unavailable".to_string();
            append_error_log(format!(
                "mention.attachment.upload.skip id={} reason={message}",
                attachment.id.raw()
            ));
            attachment.upload = AttachmentUploadState::Failed(AttachmentUploadError::new(message));
            return;
        };
        attachment.upload =
            AttachmentUploadState::Uploading(AttachmentUploadProgress::new(0, None));
        if let Err(err) = self.attachment_uploader.start_upload(
            attachment.id,
            attachment.label.clone(),
            PathBuf::from(fs_path),
        ) {
            let reason = err.message.clone();
            append_error_log(format!(
                "mention.attachment.upload.failed_to_start id={} reason={reason}",
                attachment.id.raw()
            ));
            attachment.upload = AttachmentUploadState::Failed(err);
        }
    }

    fn find_attachment_mut(&mut self, id: AttachmentId) -> Option<&mut ComposerAttachment> {
        self.attachments.iter_mut().find(|att| att.id == id)
    }

    pub fn uploaded_attachments(&self, body: &str) -> Vec<AttachmentSubmission> {
        self.referenced_attachment_indices(body)
            .into_iter()
            .filter_map(|idx| match &self.attachments[idx].upload {
                AttachmentUploadState::Uploaded(success) => {
                    let att = &self.attachments[idx];
                    let submission_kind = match success.asset_pointer.kind {
                        AttachmentPointerKind::Image => AttachmentKind::Image,
                        _ => AttachmentKind::File,
                    };
                    Some(AttachmentSubmission {
                        id: att.id,
                        label: att.label.clone(),
                        path: att.path.clone(),
                        fs_path: att.fs_path.clone(),
                        pointer: success.asset_pointer.clone(),
                        display_name: success.display_name.clone(),
                        kind: submission_kind,
                    })
                }
                _ => None,
            })
            .collect()
    }

    fn referenced_attachment_indices(&self, body: &str) -> Vec<usize> {
        if self.attachments.is_empty() || body.is_empty() {
            return Vec::new();
        }
        let mut indices = Vec::new();
        let mut used_ranges: Vec<(usize, usize)> = Vec::new();
        for (idx, att) in self.attachments.iter().enumerate() {
            if att.path.is_empty() {
                continue;
            }
            if let Some((start, end)) =
                Self::find_attachment_occurrence(body, &att.path, &used_ranges)
            {
                used_ranges.push((start, end));
                indices.push(idx);
            }
        }
        indices
    }

    fn find_attachment_occurrence(
        body: &str,
        needle: &str,
        used_ranges: &[(usize, usize)],
    ) -> Option<(usize, usize)> {
        if needle.is_empty() {
            return None;
        }
        let mut search_start = 0;
        while let Some(rel_idx) = body[search_start..].find(needle) {
            let start = search_start + rel_idx;
            let end = start + needle.len();
            if used_ranges.iter().any(|&(used_start, used_end)| {
                Self::ranges_overlap(used_start, used_end, start, end)
            }) {
                search_start = end;
                continue;
            }
            if !Self::is_path_boundary(body, start, end) {
                search_start = end;
                continue;
            }
            return Some((start, end));
        }
        None
    }

    fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
        a_start < b_end && b_start < a_end
    }

    fn is_path_boundary(body: &str, start: usize, end: usize) -> bool {
        let before = body[..start].chars().rev().next();
        if let Some(c) = before {
            if !Self::is_left_boundary_char(c) {
                return false;
            }
        }
        let after = body[end..].chars().next();
        if let Some(c) = after {
            if !Self::is_right_boundary_char(c) {
                return false;
            }
        }
        true
    }

    fn is_left_boundary_char(c: char) -> bool {
        c.is_whitespace() || matches!(c, '(' | '[' | '{' | '<' | '"' | '\'')
    }

    fn is_right_boundary_char(c: char) -> bool {
        c.is_whitespace()
            || matches!(
                c,
                ')' | ']' | '}' | '>' | ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\''
            )
    }

    fn set_submit_phase(&mut self, phase: SubmitPhase) {
        self.submit_phase = phase;
        self.submitting = !matches!(phase, SubmitPhase::Idle);
    }
}

pub struct AttachmentUploadPoll {
    pub state_changed: bool,
    pub has_pending: bool,
    pub failed: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentSubmission {
    pub id: AttachmentId,
    pub label: String,
    pub path: String,
    pub fs_path: Option<String>,
    pub pointer: AttachmentAssetPointer,
    pub display_name: String,
    pub kind: AttachmentKind,
}

pub struct MentionRefresh {
    pub state_changed: bool,
    pub search_requested: bool,
}

struct SearchResult {
    id: u64,
    query: String,
    matches: Vec<MentionSuggestion>,
}

pub struct MentionSearchEngine {
    root: Option<PathBuf>,
    result_rx: mpsc::UnboundedReceiver<SearchResult>,
    result_tx: mpsc::UnboundedSender<SearchResult>,
    latest_request_id: u64,
    last_applied_id: u64,
    in_flight: Option<Arc<AtomicBool>>,
}

impl MentionSearchEngine {
    fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            root: determine_repo_root(),
            result_rx: rx,
            result_tx: tx,
            latest_request_id: 0,
            last_applied_id: 0,
            in_flight: None,
        }
    }

    #[cfg(test)]
    fn noop() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            root: None,
            result_rx: rx,
            result_tx: tx,
            latest_request_id: 0,
            last_applied_id: 0,
            in_flight: None,
        }
    }

    fn request(&mut self, query: String) -> bool {
        if query.is_empty() {
            self.cancel();
            return false;
        }
        self.latest_request_id = self.latest_request_id.wrapping_add(1);
        self.cancel();
        let Some(root) = self.root.clone() else {
            append_error_log("mention.search.skip: no git root");
            return false;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.in_flight = Some(cancel.clone());
        let tx = self.result_tx.clone();
        let request_id = self.latest_request_id;
        append_error_log(format!(
            "mention.search.start id={} query={} root={}",
            request_id,
            query,
            root.display()
        ));
        tokio::spawn(async move {
            let search_root = root.clone();
            let cancel_clone = cancel.clone();
            let query_clone = query.clone();
            let result = tokio::task::spawn_blocking(move || {
                run_file_search(&query_clone, search_root.as_path(), cancel_clone)
            })
            .await
            .unwrap_or_else(|_| Vec::new());

            if cancel.load(Ordering::Relaxed) {
                append_error_log(format!(
                    "mention.search.cancelled id={request_id} query={query}"
                ));
                return;
            }

            let matches = result
                .into_iter()
                .map(|fm| file_match_to_suggestion(root.as_path(), fm))
                .collect();
            let _ = tx.send(SearchResult {
                id: request_id,
                query,
                matches,
            });
        });
        true
    }

    fn cancel(&mut self) {
        if let Some(token) = &self.in_flight {
            token.store(true, Ordering::Relaxed);
        }
        self.in_flight = None;
    }

    fn poll(&mut self) -> Option<SearchResult> {
        let mut latest: Option<SearchResult> = None;
        while let Ok(res) = self.result_rx.try_recv() {
            if res.id >= self.latest_request_id {
                latest = Some(res);
            }
        }
        if let Some(res) = latest {
            self.last_applied_id = res.id;
            self.in_flight = None;
            append_error_log(format!(
                "mention.search.result id={} query={} matches={}",
                res.id,
                res.query,
                res.matches.len()
            ));
            return Some(res);
        }
        None
    }
}

impl Default for MentionSearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn run_file_search(
    query: &str,
    root: &Path,
    cancel: Arc<AtomicBool>,
) -> Vec<codex_file_search::FileMatch> {
    const FILE_SEARCH_LIMIT: NonZeroUsize = NonZeroUsize::new(8).expect("limit must be non-zero");
    const FILE_SEARCH_THREADS: NonZeroUsize =
        NonZeroUsize::new(2).expect("thread count must be non-zero");
    codex_file_search::run(
        query,
        FILE_SEARCH_LIMIT,
        root,
        Vec::new(),
        FILE_SEARCH_THREADS,
        cancel,
        true,
    )
    .map(|res| res.matches)
    .unwrap_or_default()
}

fn file_match_to_suggestion(root: &Path, fm: codex_file_search::FileMatch) -> MentionSuggestion {
    let label = std::path::Path::new(&fm.path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| fm.path.clone());
    let mut suggestion = MentionSuggestion::new(label, fm.path.clone());
    let fs_path = root.join(&fm.path);
    suggestion.fs_path = Some(fs_path.display().to_string());
    suggestion
}

fn determine_repo_root() -> Option<PathBuf> {
    if let Ok(output) = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(PathBuf::from(text));
        }
    }
    std::env::current_dir().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachments::AttachmentAssetPointer;
    use crate::attachments::AttachmentKind;
    use crate::attachments::AttachmentUploadError;
    use crate::attachments::AttachmentUploadState;
    use crate::attachments::upload::AttachmentPointerKind;
    use crate::attachments::upload::AttachmentUploadSuccess;
    use base64::Engine as _;
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;
    use std::fs;
    use tokio::time::Duration;

    #[test]
    fn compose_prompt_includes_attachment_section() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: Some(5),
            end_line: Some(10),
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::default(),
        });
        let prompt = page.compose_prompt_with_attachments("body [src/main.rs]");
        assert!(prompt.contains("# Files mentioned by the user"));
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("lines 5-10"));
        assert!(prompt.contains("upload pending"));
    }

    #[test]
    fn clear_attachments_resets_list() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "foo.rs".to_string(),
            path: "src/foo.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::default(),
        });
        page.clear_attachments();
        assert!(page.attachments.is_empty());
    }

    #[test]
    fn compose_prompt_marks_uploaded_attachment() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Uploaded(AttachmentUploadSuccess::new(
                AttachmentAssetPointer::new(AttachmentPointerKind::File, "123"),
                "src/main.rs",
            )),
        });
        let prompt = page.compose_prompt_with_attachments("body [src/main.rs]");
        assert!(prompt.contains("uploaded as src/main.rs"));
    }

    #[test]
    fn accept_current_mention_adds_attachment() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::ImmediateSuccess);
        page.mention_search = MentionSearchEngine::noop();
        let _ = page
            .mention_picker
            .set_suggestions(vec![MentionSuggestion::new("src/foo.rs", "src/foo.rs")]);
        let pasted = "@foo".to_string();
        page.composer.handle_paste(pasted);
        assert!(page.refresh_mention_state().state_changed);
        assert!(page.accept_current_mention());
        assert_eq!(page.attachments.len(), 1);
        assert_eq!(page.composer.text_content(), "[src/foo.rs] ");
    }

    #[tokio::test]
    async fn accepting_mention_starts_upload() {
        let tmp = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .expect("tmp");
        let png_bytes = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAFgwJ/lkEc1QAAAABJRU5ErkJggg==")
            .expect("png bytes");
        fs::write(tmp.path(), png_bytes).unwrap();
        let mut suggestion = MentionSuggestion::new("images/pixel.png", "images/pixel.png");
        suggestion.fs_path = Some(tmp.path().display().to_string());

        let mut page = NewTaskPage::new(None, AttachmentUploadMode::ImmediateSuccess);
        page.mention_search = MentionSearchEngine::noop();
        let _ = page.mention_picker.set_suggestions(vec![suggestion]);
        page.composer.handle_paste("@src/main".to_string());
        assert!(page.refresh_mention_state().state_changed);
        assert!(page.accept_current_mention());
        assert_eq!(page.attachments.len(), 1);

        let mut uploaded = false;
        for _ in 0..50 {
            let poll = page.poll_attachment_uploads();
            if matches!(
                page.attachments[0].upload,
                AttachmentUploadState::Uploaded(_)
            ) {
                uploaded = true;
                break;
            }
            if !poll.has_pending {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            uploaded,
            "attachment upload should complete within allotted retries"
        );
        assert!(matches!(
            page.attachments[0].upload,
            AttachmentUploadState::Uploaded(_)
        ));
    }

    #[test]
    fn cancel_mention_hides_picker() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::ImmediateSuccess);
        page.mention_search = MentionSearchEngine::noop();
        let _ = page
            .mention_picker
            .set_suggestions(vec![MentionSuggestion::new(
                "tests/test.rs",
                "tests/test.rs",
            )]);
        page.composer.handle_paste("@test".to_string());
        assert!(page.refresh_mention_state().state_changed);
        page.cancel_mention();
        assert!(!page.mention_active());
    }

    #[test]
    fn compose_prompt_handles_failed_upload() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::ImmediateSuccess);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "missing.rs".to_string(),
            path: "missing.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Failed(AttachmentUploadError::new("boom")),
        });
        let prompt = page.compose_prompt_with_attachments("body [missing.rs]");
        assert!(prompt.contains("upload failed: boom"));
    }

    #[test]
    fn compose_prompt_handles_missing_file() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::ImmediateSuccess);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "missing.rs".to_string(),
            path: "missing.rs".to_string(),
            fs_path: Some("/path/does/not/exist".to_string()),
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::default(),
        });
        // Missing file without upload should still note pending status.
        let prompt = page.compose_prompt_with_attachments("body [missing.rs]");
        assert!(prompt.contains("upload pending"));
    }

    #[test]
    fn non_referenced_attachments_are_filtered() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Uploaded(AttachmentUploadSuccess::new(
                AttachmentAssetPointer::new(AttachmentPointerKind::File, "file-service://1"),
                "src/main.rs",
            )),
        });
        let body = "no files referenced";
        let prompt = page.compose_prompt_with_attachments(body);
        assert_eq!(prompt, body);
        assert!(page.uploaded_attachments(body).is_empty());
    }

    #[test]
    fn attachment_reference_allows_trailing_punctuation() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Uploaded(AttachmentUploadSuccess::new(
                AttachmentAssetPointer::new(AttachmentPointerKind::File, "file-service://1"),
                "src/main.rs",
            )),
        });
        let body = "Check [src/main.rs].";
        let prompt = page.compose_prompt_with_attachments(body);
        assert!(prompt.contains("# Files mentioned"));
        let attachments = page.uploaded_attachments(body);
        assert_eq!(attachments.len(), 1);
    }

    #[test]
    fn attachment_reference_counts_occurrences() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Uploaded(AttachmentUploadSuccess::new(
                AttachmentAssetPointer::new(AttachmentPointerKind::File, "file-service://1"),
                "src/main.rs",
            )),
        });
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(2),
            upload: AttachmentUploadState::Uploaded(AttachmentUploadSuccess::new(
                AttachmentAssetPointer::new(AttachmentPointerKind::File, "file-service://2"),
                "src/main.rs",
            )),
        });

        let single_body = "Include [src/main.rs] once";
        assert_eq!(page.uploaded_attachments(single_body).len(), 1);

        let double_body = "Include [src/main.rs] twice: [src/main.rs]";
        assert_eq!(page.uploaded_attachments(double_body).len(), 2);
    }

    #[test]
    fn prepare_submission_waits_until_uploads_complete() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Uploading(AttachmentUploadProgress::new(0, None)),
        });

        assert!(matches!(
            page.prepare_submission("body [src/main.rs]".to_string()),
            SubmissionAction::WaitForUploads
        ));
        assert_eq!(page.submit_phase(), SubmitPhase::WaitingForUploads);

        page.attachments[0].upload = AttachmentUploadState::Uploaded(AttachmentUploadSuccess::new(
            AttachmentAssetPointer::new(AttachmentPointerKind::File, "file-service://123"),
            "src/main.rs",
        ));

        let payload = page.take_ready_submission().expect("payload");
        assert!(payload.prompt.contains("uploaded as"));
        assert_eq!(page.submit_phase(), SubmitPhase::Sending);
    }

    #[test]
    fn prepare_submission_blocked_on_failed_upload() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Failed(AttachmentUploadError::new("boom")),
        });

        match page.prepare_submission("body [src/main.rs]".to_string()) {
            SubmissionAction::Blocked(msg) => assert!(msg.contains("failed")),
            other => panic!("unexpected action: {other:?}"),
        }
        assert_eq!(page.submit_phase(), SubmitPhase::Idle);
    }

    #[test]
    fn pending_unreferenced_attachments_do_not_block_submission() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Uploading(AttachmentUploadProgress::new(0, None)),
        });

        match page.prepare_submission("body".to_string()) {
            SubmissionAction::StartSending {
                prompt,
                attachments,
            } => {
                assert_eq!(prompt, "body");
                assert!(attachments.is_empty());
            }
            other => panic!("unexpected action: {other:?}"),
        }
        assert_eq!(page.submit_phase(), SubmitPhase::Sending);
    }

    #[test]
    fn failed_unreferenced_attachments_do_not_block_submission() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Failed(AttachmentUploadError::new("boom")),
        });

        match page.prepare_submission("body".to_string()) {
            SubmissionAction::StartSending {
                prompt,
                attachments,
            } => {
                assert_eq!(prompt, "body");
                assert!(attachments.is_empty());
            }
            other => panic!("unexpected action: {other:?}"),
        }
        assert_eq!(page.submit_phase(), SubmitPhase::Sending);
    }

    #[test]
    fn take_ready_submission_ignores_unreferenced_pending_uploads() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::Disabled);
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/main.rs".to_string(),
            path: "src/main.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(1),
            upload: AttachmentUploadState::Uploading(AttachmentUploadProgress::new(0, None)),
        });
        page.attachments.push(ComposerAttachment {
            kind: AttachmentKind::File,
            label: "src/other.rs".to_string(),
            path: "src/other.rs".to_string(),
            fs_path: None,
            start_line: None,
            end_line: None,
            id: AttachmentId::new(2),
            upload: AttachmentUploadState::Uploading(AttachmentUploadProgress::new(0, None)),
        });

        assert!(matches!(
            page.prepare_submission("body [src/main.rs]".to_string()),
            SubmissionAction::WaitForUploads
        ));
        assert!(page.has_pending_referenced_uploads());

        page.attachments[0].upload = AttachmentUploadState::Uploaded(AttachmentUploadSuccess::new(
            AttachmentAssetPointer::new(AttachmentPointerKind::File, "file-service://123"),
            "src/main.rs",
        ));

        assert!(!page.has_pending_referenced_uploads());
        let payload = page.take_ready_submission().expect("payload");
        assert!(payload.prompt.contains("src/main.rs"));
        assert!(
            payload
                .attachments
                .iter()
                .all(|att| att.path == "src/main.rs")
        );
    }

    #[test]
    fn pruning_removed_mentions_drops_attachments_and_logs() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::ImmediateSuccess);
        page.mention_search = MentionSearchEngine::noop();
        let _ = page
            .mention_picker
            .set_suggestions(vec![MentionSuggestion::new("src/foo.rs", "src/foo.rs")]);
        page.composer.handle_paste("@foo".to_string());
        assert!(page.refresh_mention_state().state_changed);
        assert!(page.accept_current_mention());
        assert_eq!(page.attachments.len(), 1);
        assert_eq!(page.composer.text_content(), "[src/foo.rs] ");

        page.composer.set_text_content(String::new());
        page.prune_unreferenced_attachments();
        assert!(page.attachments.is_empty());
    }

    #[test]
    fn backspace_removes_bracketed_mention_atomically() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::ImmediateSuccess);
        page.mention_search = MentionSearchEngine::noop();
        let _ = page
            .mention_picker
            .set_suggestions(vec![MentionSuggestion::new("src/foo.rs", "src/foo.rs")]);
        page.composer.handle_paste("@foo".to_string());
        assert!(page.refresh_mention_state().state_changed);
        assert!(page.accept_current_mention());
        assert_eq!(page.attachments.len(), 1);
        assert_eq!(page.composer.text_content(), "[src/foo.rs] ");

        let _ = page
            .composer
            .input(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        let _ = page
            .composer
            .input(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));

        page.prune_unreferenced_attachments();

        assert!(!page.composer.text_content().contains('['));
        assert!(page.attachments.is_empty());
    }

    #[test]
    fn non_image_attachment_is_rejected() {
        let mut page = NewTaskPage::new(None, AttachmentUploadMode::ImmediateSuccess);
        page.mention_search = MentionSearchEngine::noop();
        let tmp = tempfile::Builder::new()
            .suffix(".txt")
            .tempfile()
            .expect("tmp");
        fs::write(tmp.path(), "hello world").expect("write");
        let mut suggestion = MentionSuggestion::new("docs/readme.txt", "docs/readme.txt");
        suggestion.fs_path = Some(tmp.path().display().to_string());
        let _ = page.mention_picker.set_suggestions(vec![suggestion]);

        page.composer.handle_paste("@docs".to_string());
        assert!(page.refresh_mention_state().state_changed);
        assert!(page.accept_current_mention());
        assert_eq!(page.attachments.len(), 1);
        match &page.attachments[0].upload {
            AttachmentUploadState::Failed(err) => {
                assert!(err.message.contains("only image files"));
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }
}
