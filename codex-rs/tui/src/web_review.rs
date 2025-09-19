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

impl ReviewCommentSide {
    fn display_name(self) -> &'static str {
        match self {
            ReviewCommentSide::Left => "left",
            ReviewCommentSide::Right => "right",
        }
    }
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
                    let message = format_review_message(&summary, &payload.comments);
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

pub(crate) fn format_review_message(summary: &str, comments: &[ReviewComment]) -> String {
    let mut output = String::new();
    let trimmed_summary = summary.trim();
    let has_summary = !trimmed_summary.is_empty();
    if has_summary {
        output.push_str("Review summary:\n");
        output.push_str(trimmed_summary);
        output.push('\n');
    }

    if comments.is_empty() {
        return output;
    }

    if has_summary {
        output.push('\n');
    }
    output.push_str("File comments:\n");
    let mut grouped: Vec<(&str, Vec<&ReviewComment>)> = Vec::new();
    for comment in comments {
        if let Some((_, entries)) = grouped
            .iter_mut()
            .find(|(path, _)| *path == comment.path.as_str())
        {
            entries.push(comment);
        } else {
            grouped.push((comment.path.as_str(), vec![comment]));
        }
    }

    for (path, entries) in grouped {
        output.push_str(&format!("- {path}\n"));
        for comment in entries {
            let (start, end) = if comment.line_start <= comment.line_end {
                (comment.line_start, comment.line_end)
            } else {
                (comment.line_end, comment.line_start)
            };
            let range_label = if start == end {
                format!("L{start}")
            } else {
                format!("L{start}-{end}")
            };
            output.push_str(&format!(
                "  - {} ({}) {}\n",
                range_label,
                comment.side.display_name(),
                comment.text.trim()
            ));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

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
                line_end: 34,
                side: ReviewCommentSide::Left,
                text: "Consider renaming".to_string(),
            },
        ];
        let formatted = format_review_message("Summary text", &comments);
        assert!(formatted.contains("Summary text"));
        assert!(formatted.contains("src/lib.rs"));
        assert!(formatted.contains("L12 (right)"));
        assert!(formatted.contains("L34 (left)"));
    }
    #[test]
    fn groups_comments_by_file() {
        let comments = vec![
            ReviewComment {
                path: "a.rs".to_string(),
                line_start: 1,
                line_end: 3,
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
            ReviewComment {
                path: "a.rs".to_string(),
                line_start: 3,
                line_end: 5,
                side: ReviewCommentSide::Right,
                text: "Third".to_string(),
            },
        ];
        let formatted = format_review_message("", &comments);
        assert!(!formatted.contains("Review summary"));
        let a_index = formatted.find("- a.rs").expect("a.rs entry");
        let b_index = formatted.find("- b.rs").expect("b.rs entry");
        assert!(a_index < b_index);
        assert_eq!(formatted.matches("- a.rs").count(), 1);
        assert!(formatted.contains("L1-3 (right) First"));
        assert!(formatted.contains("L3-5 (right) Third"));
    }

    #[test]
    fn formats_review_message_without_comments() {
        let formatted = format_review_message("", &[]);
        assert!(formatted.is_empty());
    }
}
