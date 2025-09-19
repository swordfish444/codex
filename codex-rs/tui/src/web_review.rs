use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::get_git_diff::DiffFormat;
use crate::get_git_diff::get_git_diff;
use serde::Deserialize;
use serde::Serialize;
use tiny_http::Header;
use tiny_http::Method;
use tiny_http::Request;
use tiny_http::Response;
use tiny_http::Server;
use tokio::process::Command;

pub(crate) struct ReviewServer {
    pub(crate) handle: ReviewServerHandle,
    pub(crate) url: String,
}

pub(crate) struct ReviewServerHandle {
    port: u16,
    server: Arc<Server>,
    shutdown: Arc<AtomicBool>,
    join_handle: Option<thread::JoinHandle<()>>,
}

impl std::fmt::Debug for ReviewServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewServerHandle")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ReviewServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewServer")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl ReviewServerHandle {
    pub(crate) fn shutdown(&mut self) {
        let _ = self.shutdown.swap(true, Ordering::SeqCst);
        self.server.unblock();
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ReviewServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug)]
pub(crate) enum ReviewStartError {
    NotInGitRepo,
    Diff(std::io::Error),
    Server(std::io::Error),
}

impl ReviewStartError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::NotInGitRepo => "`/review` — not inside a git repository".to_string(),
            Self::Diff(err) => format!("Failed to compute diff: {err}"),
            Self::Server(err) => format!("Failed to start review server: {err}"),
        }
    }
}

#[derive(Clone)]
struct ReviewSharedState {
    diff: Arc<str>,
    repo_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReviewCommentSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ReviewComment {
    pub(crate) path: String,
    pub(crate) line_start: u32,
    pub(crate) line_end: u32,
    pub(crate) side: ReviewCommentSide,
    pub(crate) text: String,
}

#[derive(Deserialize)]
struct SubmitPayload {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    comments: Vec<ReviewComment>,
}

#[derive(Serialize)]
struct DiffResponse<'a> {
    repo_path: Option<&'a str>,
    diff: &'a str,
}

const REVIEW_APP_HTML: &str = include_str!("web_review.html");

pub(crate) async fn start_review_server(
    app_event_tx: AppEventSender,
) -> Result<ReviewServer, ReviewStartError> {
    let (is_repo, diff) = get_git_diff(DiffFormat::Plain)
        .await
        .map_err(ReviewStartError::Diff)?;
    if !is_repo {
        return Err(ReviewStartError::NotInGitRepo);
    }

    let server = Server::http(("127.0.0.1", 0))
        .map_err(|err| ReviewStartError::Server(io::Error::other(err)))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or_else(|| {
            ReviewStartError::Server(io::Error::other("failed to determine server port"))
        })?;
    let repo_path = match git_repo_root().await {
        Ok(Some(path)) => Some(path),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!("failed to determine git repo root: {err}");
            None
        }
    };
    let server = Arc::new(server);
    let shutdown = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(ReviewSharedState {
        diff: diff.into(),
        repo_path,
    });

    let server_clone = server.clone();
    let shutdown_clone = shutdown.clone();
    let shared_clone = shared;
    let join_handle = thread::spawn(move || {
        run_server_loop(server_clone, shared_clone, shutdown_clone, app_event_tx)
    });

    let handle = ReviewServerHandle {
        port,
        server,
        shutdown,
        join_handle: Some(join_handle),
    };

    let url = format!("http://127.0.0.1:{port}/");
    Ok(ReviewServer { handle, url })
}

fn run_server_loop(
    server: Arc<Server>,
    data: Arc<ReviewSharedState>,
    shutdown: Arc<AtomicBool>,
    app_event_tx: AppEventSender,
) {
    while !shutdown.load(Ordering::SeqCst) {
        match server.recv() {
            Ok(request) => {
                if handle_request(request, &data, &app_event_tx) {
                    shutdown.store(true, Ordering::SeqCst);
                    break;
                }
            }
            Err(_) => {
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }
}

fn handle_request(
    mut request: Request,
    data: &ReviewSharedState,
    app_event_tx: &AppEventSender,
) -> bool {
    let path = request.url().split('?').next().unwrap_or("/");
    match (request.method(), path) {
        (&Method::Get, "/") => {
            respond_html(request, REVIEW_APP_HTML);
            false
        }
        (&Method::Get, "/favicon.ico") => {
            let _ = request.respond(Response::empty(204));
            false
        }
        (&Method::Get, "/api/diff") => {
            let payload = DiffResponse {
                repo_path: data.repo_path.as_deref(),
                diff: &data.diff,
            };
            if let Ok(body) = serde_json::to_string(&payload) {
                respond_json(request, body);
            } else {
                respond_json(
                    request,
                    "{\"error\":\"failed to serialize diff\"}".to_string(),
                );
            }
            false
        }
        (&Method::Post, "/api/submit") => {
            let mut body = String::new();
            {
                let reader = request.as_reader();
                if reader.read_to_string(&mut body).is_err() {
                    respond_status(request, 400, "Invalid body");
                    return false;
                }
            }
            match serde_json::from_str::<SubmitPayload>(&body) {
                Ok(payload) => {
                    let summary = payload.summary.unwrap_or_default();
                    let message = format_review_message(&summary, &payload.comments, &data.diff);
                    app_event_tx.send(AppEvent::ReviewSubmitted {
                        composer_text: message,
                    });
                    respond_status(request, 200, "OK");
                    true
                }
                Err(err) => {
                    respond_status(request, 400, &format!("Invalid payload: {err}"));
                    false
                }
            }
        }
        (&Method::Post, "/api/cancel") => {
            app_event_tx.send(AppEvent::ReviewCancelled);
            respond_status(request, 200, "OK");
            true
        }
        _ => {
            respond_status(request, 404, "Not found");
            false
        }
    }
}

fn respond_html(request: Request, body: &str) {
    let mut response = Response::from_string(body.to_string());
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]) {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

fn respond_json(request: Request, body: String) {
    let mut response = Response::from_string(body);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

fn respond_status(request: Request, status: u16, message: &str) {
    let status = tiny_http::StatusCode(status);
    let mut response = Response::from_string(message.to_string()).with_status_code(status);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..])
    {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

async fn git_repo_root() -> io::Result<Option<String>> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    } else {
        Ok(None)
    }
}

pub(crate) fn format_review_message(
    summary: &str,
    comments: &[ReviewComment],
    diff: &str,
) -> String {
    let mut output = String::new();
    let trimmed_summary = summary.trim();
    let has_summary = !trimmed_summary.is_empty();
    let has_comments = !comments.is_empty();

    if has_comments {
        output.push_str("Address the following review comments.\n\n");
    }

    if has_summary {
        output.push_str("Review summary:\n");
        output.push_str(trimmed_summary);
        output.push('\n');
        if has_comments {
            output.push('\n');
        }
    }

    if !has_comments {
        return output;
    }

    let diff_index = DiffIndex::from_diff(diff);
    for (idx, comment) in comments.iter().enumerate() {
        let (start, end) = normalize_range(comment.line_start, comment.line_end);
        let range_label = if start == end {
            format!("L{start}")
        } else {
            format!("L{start}-{end}")
        };
        output.push_str(&format!("## {} {}\n", comment.path, range_label));
        let snippet = diff_index
            .snippet_for(comment)
            .unwrap_or_else(|| "[code snippet unavailable]".to_string());
        output.push_str("```\n");
        output.push_str(&snippet);
        output.push_str("\n```\n\n");
        output.push_str(comment.text.trim());
        output.push('\n');
        if idx + 1 < comments.len() {
            output.push('\n');
        }
    }

    output
}

fn normalize_range(a: u32, b: u32) -> (u32, u32) {
    if a <= b { (a, b) } else { (b, a) }
}

#[derive(Clone)]
struct DiffLine {
    prefix: char,
    text: String,
}

impl DiffLine {
    fn new(prefix: char, text: &str) -> Self {
        Self {
            prefix,
            text: text.to_string(),
        }
    }

    fn render(&self) -> String {
        format!("{}{}", self.prefix, self.text)
    }
}

struct FileDiffLines {
    left: BTreeMap<u32, DiffLine>,
    right: BTreeMap<u32, DiffLine>,
}

impl FileDiffLines {
    fn new() -> Self {
        Self {
            left: BTreeMap::new(),
            right: BTreeMap::new(),
        }
    }
}

struct DiffIndex {
    files: HashMap<String, FileDiffLines>,
}

impl DiffIndex {
    fn from_diff(diff: &str) -> Self {
        let mut files: HashMap<String, FileDiffLines> = HashMap::new();
        let mut current_old: Option<String> = None;
        let mut current_new: Option<String> = None;
        let mut old_line: u32 = 0;
        let mut new_line: u32 = 0;

        for line in diff.lines() {
            if line.starts_with("diff --git ") {
                current_old = None;
                current_new = None;
                old_line = 0;
                new_line = 0;
                continue;
            }
            if let Some(path) = parse_file_path_line(line, "--- ") {
                current_old = path;
                ensure_entry(&mut files, current_old.as_deref());
                continue;
            }
            if let Some(path) = parse_file_path_line(line, "+++ ") {
                current_new = path;
                ensure_entry(&mut files, current_new.as_deref());
                continue;
            }
            if let Some((old_start, new_start)) = parse_hunk_header(line) {
                old_line = old_start;
                new_line = new_start;
                continue;
            }

            if line.starts_with('+') && !line.starts_with("+++") {
                let text = line.get(1..).unwrap_or("");
                insert_line(
                    &mut files,
                    current_new.as_ref(),
                    ReviewCommentSide::Right,
                    new_line,
                    '+',
                    text,
                );
                if current_new != current_old {
                    insert_line(
                        &mut files,
                        current_old.as_ref(),
                        ReviewCommentSide::Right,
                        new_line,
                        '+',
                        text,
                    );
                }
                new_line = new_line.saturating_add(1);
                continue;
            }

            if line.starts_with('-') && !line.starts_with("---") {
                let text = line.get(1..).unwrap_or("");
                insert_line(
                    &mut files,
                    current_old.as_ref(),
                    ReviewCommentSide::Left,
                    old_line,
                    '-',
                    text,
                );
                if current_new != current_old {
                    insert_line(
                        &mut files,
                        current_new.as_ref(),
                        ReviewCommentSide::Left,
                        old_line,
                        '-',
                        text,
                    );
                }
                old_line = old_line.saturating_add(1);
                continue;
            }

            if line.starts_with(' ') {
                let text = line.get(1..).unwrap_or("");
                insert_line(
                    &mut files,
                    current_old.as_ref(),
                    ReviewCommentSide::Left,
                    old_line,
                    ' ',
                    text,
                );
                insert_line(
                    &mut files,
                    current_new.as_ref(),
                    ReviewCommentSide::Right,
                    new_line,
                    ' ',
                    text,
                );
                if current_new != current_old {
                    insert_line(
                        &mut files,
                        current_new.as_ref(),
                        ReviewCommentSide::Left,
                        old_line,
                        ' ',
                        text,
                    );
                    insert_line(
                        &mut files,
                        current_old.as_ref(),
                        ReviewCommentSide::Right,
                        new_line,
                        ' ',
                        text,
                    );
                }
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
                continue;
            }
        }

        Self { files }
    }

    fn snippet_for(&self, comment: &ReviewComment) -> Option<String> {
        let file = self.files.get(&comment.path)?;
        let (start, end) = normalize_range(comment.line_start, comment.line_end);
        let lines = match comment.side {
            ReviewCommentSide::Left => &file.left,
            ReviewCommentSide::Right => &file.right,
        };
        let collected: Vec<_> = lines
            .range(start..=end)
            .map(|(_, line)| line.render())
            .collect();
        if collected.is_empty() {
            None
        } else {
            Some(collected.join("\n"))
        }
    }
}

fn insert_line(
    files: &mut HashMap<String, FileDiffLines>,
    path: Option<&String>,
    side: ReviewCommentSide,
    line_no: u32,
    prefix: char,
    text: &str,
) {
    if let Some(path) = path {
        if let Some(file) = files.get_mut(path) {
            let target = match side {
                ReviewCommentSide::Left => &mut file.left,
                ReviewCommentSide::Right => &mut file.right,
            };
            target.insert(line_no, DiffLine::new(prefix, text));
        }
    }
}

fn ensure_entry(map: &mut HashMap<String, FileDiffLines>, path: Option<&str>) {
    if let Some(path) = path {
        map.entry(path.to_string())
            .or_insert_with(FileDiffLines::new);
    }
}

fn parse_file_path_line(line: &str, prefix: &str) -> Option<Option<String>> {
    if !line.starts_with(prefix) {
        return None;
    }
    let path_part = line[prefix.len()..].trim();
    let mut segment = path_part.split('\t').next().unwrap_or("");
    if segment.starts_with('"') && segment.ends_with('"') && segment.len() >= 2 {
        segment = &segment[1..segment.len() - 1];
    }
    let normalized = segment.trim();
    if normalized == "/dev/null" || normalized.is_empty() {
        return Some(None);
    }
    let normalized = normalized
        .strip_prefix("a/")
        .or_else(|| normalized.strip_prefix("b/"))
        .unwrap_or(normalized);
    Some(Some(normalized.to_string()))
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    if !line.starts_with("@@") {
        return None;
    }
    let mut parts = line.split_whitespace();
    let _ = parts.next()?; // @@
    let old_part = parts.next()?;
    let new_part = parts.next()?;
    let old_start = parse_range(old_part, '-');
    let new_start = parse_range(new_part, '+');
    match (old_start, new_start) {
        (Some(o), Some(n)) => Some((o, n)),
        _ => None,
    }
}

fn parse_range(part: &str, marker: char) -> Option<u32> {
    part.strip_prefix(marker)?
        .split(',')
        .next()
        .and_then(|s| s.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const SAMPLE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -12,1 +12,1 @@ fn sample() {\n-old_call();\n+new_call();\n }\n@@ -34,2 +33,0 @@ fn old_section() {\n-old_line_one();\n-old_line_two();\n }\n";

    #[test]
    fn formats_review_message_with_comments() {
        let comments = vec![
            ReviewComment {
                path: "src/lib.rs".to_string(),
                line_start: 12,
                line_end: 12,
                side: ReviewCommentSide::Right,
                text: "Looks good".to_string(),
            },
            ReviewComment {
                path: "src/lib.rs".to_string(),
                line_start: 34,
                line_end: 35,
                side: ReviewCommentSide::Left,
                text: "Consider renaming".to_string(),
            },
        ];
        let formatted = format_review_message("Summary text", &comments, SAMPLE_DIFF);
        let expected = "Address the following review comments.\n\nReview summary:\nSummary text\n\n## src/lib.rs L12\n```\n+new_call();\n```\n\nLooks good\n\n## src/lib.rs L34-35\n```\n-old_line_one();\n-old_line_two();\n```\n\nConsider renaming\n";
        assert_eq!(formatted, expected);
    }

    const MULTI_DIFF: &str = "diff --git a/a.rs b/a.rs\nindex 3333333..4444444 100644\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,3 @@\n-line one\n+line one updated\n line two\n line three\ndiff --git a/b.rs b/b.rs\nindex 5555555..6666666 100644\n--- a/b.rs\n+++ b/b.rs\n@@ -2,2 +2,2 @@\n-line alpha\n+line alpha new\n line beta\n";

    #[test]
    fn preserves_comment_order() {
        let comments = vec![
            ReviewComment {
                path: "a.rs".to_string(),
                line_start: 1,
                line_end: 1,
                side: ReviewCommentSide::Right,
                text: "First".to_string(),
            },
            ReviewComment {
                path: "b.rs".to_string(),
                line_start: 2,
                line_end: 2,
                side: ReviewCommentSide::Left,
                text: "Second".to_string(),
            },
        ];
        let formatted = format_review_message("", &comments, MULTI_DIFF);
        let expected_headings: Vec<_> = formatted
            .lines()
            .filter(|line| line.starts_with("## "))
            .collect();
        assert_eq!(expected_headings, vec!["## a.rs L1", "## b.rs L2"]);
        assert!(!formatted.contains("Review summary"));
    }

    #[test]
    fn formats_review_message_without_comments() {
        let formatted = format_review_message("Only summary", &[], SAMPLE_DIFF);
        assert_eq!(formatted, "Review summary:\nOnly summary\n");
    }
}
