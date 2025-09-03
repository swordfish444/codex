use crate::ApplyOutcome;
use crate::ApplyStatus;
use crate::CloudBackend;
use crate::Error;
use crate::Result;
use crate::TaskId;
use crate::TaskStatus;
use crate::TaskSummary;
use chrono::DateTime;
use chrono::Utc;
use codex_cloud_tasks_api::DiffSummary;

use serde_json::Value;
use std::collections::HashMap;

use codex_backend_client as backend;
use codex_backend_client::CodeTaskDetailsResponseExt;
use codex_backend_client::types::extract_file_paths_list;

#[derive(Clone)]
pub struct HttpClient {
    pub base_url: String,
    backend: backend::Client,
}

impl HttpClient {
    pub fn new(base_url: impl Into<String>) -> anyhow::Result<Self> {
        let base_url = base_url.into();
        let backend = backend::Client::new(base_url.clone())?;
        Ok(Self { base_url, backend })
    }

    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.backend = self.backend.clone().with_bearer_token(token);
        self
    }

    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.backend = self.backend.clone().with_user_agent(ua);
        self
    }

    pub fn with_chatgpt_account_id(mut self, account_id: impl Into<String>) -> Self {
        self.backend = self.backend.clone().with_chatgpt_account_id(account_id);
        self
    }
}

#[async_trait::async_trait]
impl CloudBackend for HttpClient {
    async fn list_tasks(&self, env: Option<&str>) -> Result<Vec<TaskSummary>> {
        let resp = self
            .backend
            .list_tasks(Some(20), Some("current"), env)
            .await
            .map_err(|e| Error::Http(format!("list_tasks failed: {e}")))?;

        let tasks: Vec<TaskSummary> = resp
            .items
            .into_iter()
            .map(map_task_list_item_to_summary)
            .collect();
        // Debug log for env filtering visibility
        append_error_log(&format!(
            "http.list_tasks: env={} items={}",
            env.unwrap_or("<all>"),
            tasks.len()
        ));
        Ok(tasks)
    }

    async fn get_task_diff(&self, _id: TaskId) -> Result<String> {
        let id = _id.0;
        let (details, body, ct) = self
            .backend
            .get_task_details_with_body(&id)
            .await
            .map_err(|e| Error::Http(format!("get_task_details failed: {e}")))?;
        if let Some(diff) = details.unified_diff() {
            return Ok(diff);
        }
        // No diff yet (pending or non-diff task). Return a structured error so UI can render cleanly.
        // Keep a concise body tail in logs if needed by callers.
        let _ = (body, ct); // silence unused if logging is disabled at callsite
        Err(Error::NoDiffYet)
    }

    async fn get_task_messages(&self, _id: TaskId) -> Result<Vec<String>> {
        let id = _id.0;
        let (details, body, ct) = self
            .backend
            .get_task_details_with_body(&id)
            .await
            .map_err(|e| Error::Http(format!("get_task_details failed: {e}")))?;
        let mut msgs = details.assistant_text_messages();
        if msgs.is_empty() {
            // Fallback: some pending tasks expose only worklog messages; parse from raw body.
            if let Ok(full) = serde_json::from_str::<serde_json::Value>(&body) {
                // worklog.messages[*] where author.role == "assistant" → content.parts[*].text
                if let Some(arr) = full
                    .get("current_assistant_turn")
                    .and_then(|v| v.get("worklog"))
                    .and_then(|v| v.get("messages"))
                    .and_then(|v| v.as_array())
                {
                    for m in arr {
                        let is_assistant = m
                            .get("author")
                            .and_then(|a| a.get("role"))
                            .and_then(|r| r.as_str())
                            == Some("assistant");
                        if !is_assistant {
                            continue;
                        }
                        if let Some(parts) = m
                            .get("content")
                            .and_then(|c| c.get("parts"))
                            .and_then(|p| p.as_array())
                        {
                            for p in parts {
                                if let Some(s) = p.as_str() {
                                    // Shape: content { content_type: "text", parts: ["..."] }
                                    if !s.is_empty() {
                                        msgs.push(s.to_string());
                                    }
                                    continue;
                                }
                                if let Some(obj) = p.as_object() {
                                    if obj.get("content_type").and_then(|t| t.as_str())
                                        == Some("text")
                                    {
                                        if let Some(txt) = obj.get("text").and_then(|t| t.as_str())
                                        {
                                            msgs.push(txt.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if !msgs.is_empty() {
            return Ok(msgs);
        }
        if let Some(err) = details.assistant_error_message() {
            return Ok(vec![format!("Task failed: {err}")]);
        }
        // No assistant messages found; return a debuggable error with context for logging.
        let url = if self.base_url.contains("/backend-api") {
            format!("{}/wham/tasks/{}", self.base_url, id)
        } else {
            format!("{}/api/codex/tasks/{}", self.base_url, id)
        };
        Err(Error::Http(format!(
            "No assistant text messages in response. GET {url}; content-type={ct}; body={body}"
        )))
    }

    async fn apply_task(&self, _id: TaskId) -> Result<ApplyOutcome> {
        let id = _id.0;
        // Fetch diff fresh and apply locally via git (unified diffs).
        let details = self
            .backend
            .get_task_details(&id)
            .await
            .map_err(|e| Error::Http(format!("get_task_details failed: {e}")))?;
        let diff = details
            .unified_diff()
            .ok_or_else(|| Error::Msg(format!("No diff available for task {id}")))?;
        let diff = match crate::patch_apply::classify_patch(&diff) {
            crate::patch_apply::PatchKind::HunkOnly => {
                let files = extract_file_paths_list(&details);
                if files.len() > 1 {
                    let parts = crate::patch_apply::split_hunk_body_into_files(&diff);
                    if parts.len() == files.len() {
                        let mut acc = String::new();
                        for (i, (oldp, newp)) in files.iter().enumerate() {
                            let u = crate::patch_apply::synthesize_unified_single_file(
                                &parts[i], oldp, newp,
                            );
                            acc.push_str(&u);
                            if !acc.ends_with("\n") {
                                acc.push('\n');
                            }
                        }
                        acc
                    } else if let Some((oldp, newp)) = details.single_file_paths() {
                        crate::patch_apply::synthesize_unified_single_file(&diff, &oldp, &newp)
                    } else {
                        diff
                    }
                } else if let Some((oldp, newp)) = details.single_file_paths() {
                    crate::patch_apply::synthesize_unified_single_file(&diff, &oldp, &newp)
                } else {
                    diff
                }
            }
            _ => diff,
        };

        // Run the centralized Git apply path (supports unified diffs and Codex conversion)
        let ctx = crate::patch_apply::context_from_env(
            std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()),
        );
        let res = crate::patch_apply::apply_patch(&diff, &ctx);
        let status = match res.status {
            crate::patch_apply::ApplyStatus::Success => ApplyStatus::Success,
            crate::patch_apply::ApplyStatus::Partial => ApplyStatus::Partial,
            crate::patch_apply::ApplyStatus::Error => ApplyStatus::Error,
        };
        let applied = matches!(status, ApplyStatus::Success);
        let message = match status {
            ApplyStatus::Success => format!(
                "Applied task {id} locally ({} changed)",
                res.changed_paths.len()
            ),
            ApplyStatus::Partial => format!(
                "Apply partially succeeded for task {id} (changed={}, skipped={}, conflicts={})",
                res.changed_paths.len(),
                res.skipped_paths.len(),
                res.conflict_paths.len()
            ),
            ApplyStatus::Error => {
                let is_check = res.diagnostics.contains("apply --check failed");
                if is_check {
                    format!(
                        "Apply check failed for task {id}: patch does not apply to your working tree. No changes were made. See error.log for details.",
                    )
                } else {
                    // Compact, single-line fallback; avoid embedding multiline stderr directly.
                    let mut diag = res.diagnostics.replace('\n', " ");
                    if diag.len() > 600 {
                        diag.truncate(600);
                        diag.push_str("…");
                    }
                    format!(
                        "Apply failed for task {id} (changed={}, skipped={}, conflicts={}); {}",
                        res.changed_paths.len(),
                        res.skipped_paths.len(),
                        res.conflict_paths.len(),
                        diag
                    )
                }
            }
        };

        // On apply failure, log a detailed record including the diff we attempted.
        if matches!(status, ApplyStatus::Error) {
            let mut log = String::new();
            let summary = summarize_patch_for_logging(&diff);
            use std::fmt::Write as _;
            let _ = writeln!(
                &mut log,
                "apply_error: id={} changed={} skipped={} conflicts={}; {}",
                id,
                res.changed_paths.len(),
                res.skipped_paths.len(),
                res.conflict_paths.len(),
                res.diagnostics
            );
            let _ = writeln!(&mut log, "{summary}");
            let _ = writeln!(&mut log, "----- PATCH BEGIN -----");
            let _ = writeln!(&mut log, "{diff}");
            let _ = writeln!(&mut log, "----- PATCH END -----");
            append_error_log(&log);
        }

        Ok(ApplyOutcome {
            applied,
            status,
            message,
            skipped_paths: res.skipped_paths,
            conflict_paths: res.conflict_paths,
        })
    }

    async fn create_task(
        &self,
        env_id: &str,
        prompt: &str,
        git_ref: &str,
        qa_mode: bool,
    ) -> Result<codex_cloud_tasks_api::CreatedTask> {
        // Build request payload patterned after VSCode/newtask.rs
        let mut input_items: Vec<serde_json::Value> = Vec::new();
        input_items.push(serde_json::json!({
            "type": "message",
            "role": "user",
            "content": [{ "content_type": "text", "text": prompt }]
        }));

        if let Ok(diff) = std::env::var("CODEX_STARTING_DIFF") {
            if !diff.is_empty() {
                input_items.push(serde_json::json!({
                    "type": "pre_apply_patch",
                    "output_diff": { "diff": diff }
                }));
            }
        }

        let request_body = serde_json::json!({
            "new_task": {
                "environment_id": env_id,
                "branch": git_ref,
                "run_environment_in_qa_mode": qa_mode,
            },
            "input_items": input_items,
        });

        // Use the underlying backend client to post with proper headers
        match self.backend.create_task(request_body).await {
            Ok(id) => {
                append_error_log(&format!(
                    "new_task: created id={id} env={} prompt_chars={}",
                    env_id,
                    prompt.chars().count()
                ));
                Ok(codex_cloud_tasks_api::CreatedTask { id: TaskId(id) })
            }
            Err(e) => {
                append_error_log(&format!(
                    "new_task: create failed env={} prompt_chars={}: {}",
                    env_id,
                    prompt.chars().count(),
                    e
                ));
                Err(Error::Http(format!("create_task failed: {e}")))
            }
        }
    }
}

fn map_task_list_item_to_summary(src: backend::TaskListItem) -> TaskSummary {
    fn env_label_from_status_display(v: Option<&HashMap<String, Value>>) -> Option<String> {
        let obj = v?;
        let raw = obj.get("environment_label")?;
        if let Some(s) = raw.as_str() {
            if s.trim().is_empty() {
                return None;
            }
            return Some(s.to_string());
        }
        if let Some(o) = raw.as_object() {
            // Best-effort support for rich shapes: { text: "..." } or { plain_text: "..." }
            if let Some(s) = o.get("text").and_then(Value::as_str) {
                if !s.trim().is_empty() {
                    return Some(s.to_string());
                }
            }
            if let Some(s) = o.get("plain_text").and_then(Value::as_str) {
                if !s.trim().is_empty() {
                    return Some(s.to_string());
                }
            }
            // Fallback: compact JSON for debugging
            if let Ok(s) = serde_json::to_string(o) {
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
    }

    // Best-effort parse of diff_stats (when present in latest_turn_status_display)
    fn diff_summary_from_status_display(v: Option<&HashMap<String, Value>>) -> DiffSummary {
        let mut out = DiffSummary::default();
        let Some(map) = v else { return out };
        let latest = map
            .get("latest_turn_status_display")
            .and_then(Value::as_object);
        let Some(latest) = latest else { return out };
        if let Some(ds) = latest.get("diff_stats").and_then(Value::as_object) {
            if let Some(n) = ds.get("files_modified").and_then(Value::as_i64) {
                out.files_changed = n.max(0) as usize;
            }
            if let Some(n) = ds.get("lines_added").and_then(Value::as_i64) {
                out.lines_added = n.max(0) as usize;
            }
            if let Some(n) = ds.get("lines_removed").and_then(Value::as_i64) {
                out.lines_removed = n.max(0) as usize;
            }
        }
        out
    }

    TaskSummary {
        id: TaskId(src.id),
        title: src.title,
        status: map_status(src.task_status_display.as_ref()),
        updated_at: parse_updated_at(src.updated_at.as_ref()),
        environment_id: None,
        environment_label: env_label_from_status_display(src.task_status_display.as_ref()),
        summary: diff_summary_from_status_display(src.task_status_display.as_ref()),
    }
}

fn map_status(v: Option<&HashMap<String, Value>>) -> TaskStatus {
    if let Some(val) = v {
        // Prefer nested latest_turn_status_display.turn_status when present.
        if let Some(turn) = val
            .get("latest_turn_status_display")
            .and_then(Value::as_object)
        {
            if let Some(s) = turn.get("turn_status").and_then(Value::as_str) {
                return match s {
                    "failed" => TaskStatus::Error,
                    "completed" => TaskStatus::Ready,
                    "in_progress" => TaskStatus::Pending,
                    "pending" => TaskStatus::Pending,
                    "cancelled" => TaskStatus::Error,
                    _ => TaskStatus::Pending,
                };
            }
        }
        // Legacy or alternative flat state.
        if let Some(state) = val.get("state").and_then(Value::as_str) {
            return match state {
                "pending" => TaskStatus::Pending,
                "ready" => TaskStatus::Ready,
                "applied" => TaskStatus::Applied,
                "error" => TaskStatus::Error,
                _ => TaskStatus::Pending,
            };
        }
    }
    TaskStatus::Pending
}

fn parse_updated_at(ts: Option<&f64>) -> DateTime<Utc> {
    if let Some(v) = ts {
        // Value is seconds since epoch with fractional part.
        let secs = *v as i64;
        let nanos = ((*v - secs as f64) * 1_000_000_000.0) as u32;
        return DateTime::<Utc>::from(
            std::time::UNIX_EPOCH + std::time::Duration::new(secs.max(0) as u64, nanos),
        );
    }
    Utc::now()
}

/// Return a compact one-line classification of the patch plus a short head snippet
/// to aid debugging when apply fails.
fn summarize_patch_for_logging(patch: &str) -> String {
    let trimmed = patch.trim_start();
    let kind = if trimmed.starts_with("*** Begin Patch") {
        "codex-patch"
    } else if trimmed.starts_with("diff --git ") || trimmed.contains("\n*** End Patch\n") {
        // In some cases providers nest a codex patch inside another format; detect both.
        "git-diff"
    } else if trimmed.starts_with("@@ ") || trimmed.contains("\n@@ ") {
        "unified-diff"
    } else {
        "unknown"
    };
    let lines = patch.lines().count();
    let chars = patch.len();
    let cwd = std::env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    // Grab the first up-to-20 non-empty lines for context.
    let head: String = patch.lines().take(20).collect::<Vec<&str>>().join("\n");
    // Make sure we don't explode logs with huge content.
    let head_trunc = if head.len() > 800 {
        format!("{}…", &head[..800])
    } else {
        head
    };
    format!(
        "patch_summary: kind={kind} lines={lines} chars={chars} cwd={cwd} ; head=\n{head_trunc}"
    )
}

fn append_error_log(message: &str) {
    let ts = Utc::now().to_rfc3339();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("error.log")
    {
        use std::io::Write as _;
        let _ = writeln!(f, "[{ts}] {message}");
    }
}
