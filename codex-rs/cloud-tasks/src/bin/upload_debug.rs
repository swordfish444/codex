use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use codex_cloud_tasks::AttachmentAssetPointer;
use codex_cloud_tasks::AttachmentId;
use codex_cloud_tasks::AttachmentUploadHttpConfig;
use codex_cloud_tasks::AttachmentUploadMode;
use codex_cloud_tasks::AttachmentUploadUpdate;
use codex_cloud_tasks::AttachmentUploader;
use codex_cloud_tasks::env_detect;
use codex_cloud_tasks::pointer_id_from_value;
use codex_cloud_tasks::util::append_error_log;
use codex_cloud_tasks::util::extract_chatgpt_account_id;
use codex_cloud_tasks::util::normalize_base_url;
use codex_cloud_tasks::util::set_user_agent_suffix;
use codex_core::config::find_codex_home;
use codex_core::default_client::get_codex_user_agent;
use codex_login::AuthManager;
use image::image_dimensions;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderName;
use reqwest::header::HeaderValue;
use reqwest::header::USER_AGENT;
use serde_json::json;
use tokio::time::sleep;

#[derive(Debug, Parser)]
#[command(
    name = "upload-debug",
    version,
    about = "Debug Codex cloud image uploads"
)]
struct Args {
    /// Explicit environment id to submit to. Uses auto-detect when omitted.
    #[arg(long = "env-id")]
    environment_id: Option<String>,
    /// Optional environment label hint when auto-detecting (case-insensitive).
    #[arg(long = "env-label")]
    environment_label: Option<String>,
    /// Git ref/branch to include in the submission payload.
    #[arg(long = "ref", default_value = "main")]
    git_ref: String,
    /// Enable QA mode for task creation.
    #[arg(long = "qa-mode", default_value_t = false)]
    qa_mode: bool,
    /// Files to upload as images. Use paths relative to the current workspace for easier repros.
    #[arg(long = "image", value_name = "PATH")]
    images: Vec<String>,
    /// Optional override prompt text. Defaults to a canned debug prompt.
    #[arg(long)]
    prompt: Option<String>,
    /// Skip the final POST /wham/tasks call, only perform uploads.
    #[arg(long = "skip-submit", default_value_t = false)]
    skip_submit: bool,
}

struct ImageAttachment {
    id: AttachmentId,
    fs_path: PathBuf,
    submit_path: String,
    display_name: String,
    size_bytes: u64,
}

#[derive(Clone)]
struct UploadOutcome {
    pointer: AttachmentAssetPointer,
    submit_path: String,
    size_bytes: u64,
    width: u32,
    height: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.images.is_empty() {
        anyhow::bail!("Provide at least one --image <PATH> to upload");
    }

    set_user_agent_suffix("codex_cloud_tasks_upload_debug");
    append_error_log("upload-debug: starting run");

    let base_url = normalize_base_url(
        &std::env::var("CODEX_CLOUD_TASKS_BASE_URL")
            .unwrap_or_else(|_| "https://chatgpt.com/backend-api".to_string()),
    );
    println!("base_url: {base_url}");

    let user_agent = get_codex_user_agent();
    println!("user_agent: {user_agent}");

    let (auth_token, account_id) = load_chatgpt_auth().await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&user_agent).unwrap_or(HeaderValue::from_static("codex-cli")),
    );
    if let Some(token) = &auth_token {
        let hv = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("failed to encode bearer token header")?;
        headers.insert(AUTHORIZATION, hv);
    }
    if let Some(acc) = &account_id {
        let name = HeaderName::from_static("ChatGPT-Account-Id");
        let hv = HeaderValue::from_str(acc).context("invalid ChatGPT-Account-Id header")?;
        headers.insert(name, hv);
    }

    let env_id = match args.environment_id.clone() {
        Some(id) => {
            println!("env_id (provided): {id}");
            id
        }
        None => {
            let selection = env_detect::autodetect_environment_id(
                &base_url,
                &headers,
                args.environment_label.clone(),
            )
            .await
            .context("failed to auto-detect environment")?;
            if let Some(label) = selection.label.as_deref() {
                println!("env_id (auto): {} — label: {}", selection.id, label);
            } else {
                println!("env_id (auto): {}", selection.id);
            }
            selection.id
        }
    };

    let attachments = build_attachments(&args.images)?;
    println!("attachments: {}", attachments.len());

    let auth_token = auth_token.context("ChatGPT auth token is required. Run `codex login`.")?;

    let upload_cfg = AttachmentUploadHttpConfig {
        base_url: base_url.clone(),
        bearer_token: Some(auth_token.clone()),
        chatgpt_account_id: account_id.clone(),
        user_agent: Some(user_agent.clone()),
    };
    let mut uploader = AttachmentUploader::new(AttachmentUploadMode::Http(upload_cfg));

    let upload_results = perform_uploads(&mut uploader, &attachments).await?;

    println!("uploads complete:");
    for (idx, outcome) in upload_results.iter().enumerate() {
        let file_id = pointer_id_from_value(&outcome.pointer.value)
            .unwrap_or_else(|| "<unknown>".to_string());
        println!(
            "  [{}] {} -> {} ({} bytes, {}x{})",
            idx + 1,
            outcome.submit_path,
            file_id,
            outcome.size_bytes,
            outcome.width,
            outcome.height
        );
    }

    let prompt = args
        .prompt
        .clone()
        .unwrap_or_else(|| "Debug upload via codex cloud upload-debug".to_string());

    let mut input_items = vec![json!({
        "type": "message",
        "role": "user",
        "content": [{ "content_type": "text", "text": prompt }]
    })];

    if let Ok(diff) = std::env::var("CODEX_STARTING_DIFF")
        && !diff.is_empty()
    {
        input_items.push(json!({
            "type": "pre_apply_patch",
            "output_diff": { "diff": diff }
        }));
    }

    for outcome in &upload_results {
        input_items.push(json!({
            "type": "image_asset_pointer",
            "asset_pointer": outcome.pointer.value,
            "width": outcome.width,
            "height": outcome.height,
            "size_bytes": outcome.size_bytes,
        }));
    }

    let request_body = json!({
        "new_task": {
            "environment_id": env_id,
            "branch": args.git_ref,
            "run_environment_in_qa_mode": args.qa_mode,
        },
        "input_items": input_items,
    });

    let pretty = serde_json::to_string_pretty(&request_body)?;
    println!("request payload:\n{pretty}");
    append_error_log(format!(
        "upload-debug: request body {}",
        truncate(&pretty, 6000)
    ));

    if args.skip_submit {
        println!("--skip-submit set; skipping POST /wham/tasks");
        return Ok(());
    }

    let client = reqwest::Client::builder().build()?;
    let url = if base_url.contains("/backend-api") {
        format!("{base_url}/wham/tasks")
    } else {
        format!("{base_url}/api/codex/tasks")
    };

    let mut req = client.post(&url).header(USER_AGENT, user_agent.clone());
    req = req.header(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    req = req.bearer_auth(auth_token.clone());
    if let Some(acc) = &account_id {
        req = req.header("ChatGPT-Account-Id", acc);
    }
    let resp = req.json(&request_body).send().await?;

    let status = resp.status();
    let ct = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = resp.text().await.unwrap_or_default();

    println!("status: {status}");
    println!("content-type: {ct}");
    let parsed = serde_json::from_str::<serde_json::Value>(&body);
    let parsed_value = match parsed {
        Ok(ref v) => {
            println!(
                "response (pretty JSON):\n{}",
                serde_json::to_string_pretty(v).unwrap_or(body.clone())
            );
            Some(v.clone())
        }
        Err(_) => {
            println!("response (raw):\n{body}");
            None
        }
    };

    append_error_log(format!(
        "upload-debug: POST {} status={} body={}",
        url,
        status,
        truncate(&body, 4000)
    ));

    if !status.is_success() {
        std::process::exit(1);
    }

    let task_id = parsed_value
        .as_ref()
        .and_then(|v| v.get("task"))
        .and_then(|task| task.get("id"))
        .and_then(|id| id.as_str())
        .map(str::to_string);

    let task_id = match task_id {
        Some(id) => {
            println!("created task id: {id}");
            id
        }
        None => {
            eprintln!("error: response missing task.id field");
            std::process::exit(1);
        }
    };

    verify_task_creation(
        &client,
        &base_url,
        &auth_token,
        account_id.as_deref(),
        &task_id,
    )
    .await?;

    Ok(())
}

async fn verify_task_creation(
    client: &reqwest::Client,
    base_url: &str,
    auth_token: &str,
    account_id: Option<&str>,
    task_id: &str,
) -> anyhow::Result<()> {
    let mut url = base_url.to_string();
    if url.ends_with('/') {
        url.pop();
    }
    url = format!("{url}/wham/tasks/{task_id}");

    const MAX_POLLS: usize = 30;
    const POLL_DELAY: Duration = Duration::from_secs(2);

    let mut saw_image_pointer = false;
    let mut final_status = String::from("unknown");
    let mut final_error: Option<serde_json::Value> = None;
    let mut last_value: Option<serde_json::Value> = None;
    let mut last_body = String::new();

    for attempt in 0..=MAX_POLLS {
        let mut req = client.get(&url).bearer_auth(auth_token);
        if let Some(acc) = account_id {
            req = req.header("ChatGPT-Account-Id", acc);
        }

        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            eprintln!(
                "error: GET {} returned {} body={}",
                url,
                status,
                truncate(&body, 2000)
            );
            std::process::exit(1);
        }

        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| anyhow!("failed to parse task GET response: {e}"))?;
        last_body = body.clone();
        last_value = Some(value.clone());
        let found_id = value
            .get("task")
            .and_then(|task| task.get("id"))
            .and_then(|id| id.as_str())
            .unwrap_or("");
        if found_id != task_id {
            eprintln!("error: GET {url} returned mismatched task id {found_id}");
            std::process::exit(1);
        }

        let turn_items = value
            .get("user_turn")
            .or_else(|| value.get("current_user_turn"))
            .and_then(|turn| turn.get("input_items"))
            .and_then(|items| items.as_array())
            .cloned()
            .unwrap_or_default();
        if !saw_image_pointer {
            for item in &turn_items {
                if item.get("type").and_then(|t| t.as_str()) == Some("image_asset_pointer") {
                    saw_image_pointer = true;
                    break;
                }
            }
        }

        let turn = value
            .get("turn")
            .or_else(|| value.get("current_assistant_turn"))
            .or_else(|| value.get("current_turn"));
        final_status = turn
            .and_then(|t| t.get("turn_status"))
            .and_then(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string();
        final_error = turn
            .and_then(|t| t.get("error"))
            .filter(|e| !e.is_null())
            .cloned();

        let finished = matches!(
            final_status.as_str(),
            "completed" | "success" | "ready" | "failed" | "error" | "cancelled"
        );

        if finished {
            break;
        }

        if attempt == MAX_POLLS {
            eprintln!(
                "error: task {} did not complete within {} polls (last status={})",
                task_id,
                MAX_POLLS + 1,
                final_status
            );
            if let Some(val) = &last_value {
                eprintln!(
                    "task detail: {}",
                    truncate(&serde_json::to_string_pretty(val).unwrap_or_default(), 2000)
                );
            } else if !last_body.is_empty() {
                eprintln!("task detail: {}", truncate(&last_body, 2000));
            }
            std::process::exit(1);
        }

        sleep(POLL_DELAY).await;
    }

    if !saw_image_pointer {
        eprintln!("error: created task missing image_asset_pointer input item after polling");
        if let Some(val) = &last_value {
            eprintln!(
                "task detail: {}",
                truncate(&serde_json::to_string_pretty(val).unwrap_or_default(), 2000)
            );
        } else if !last_body.is_empty() {
            eprintln!("task detail: {}", truncate(&last_body, 2000));
        }
        std::process::exit(1);
    }

    if final_status.eq_ignore_ascii_case("failed")
        || final_status.eq_ignore_ascii_case("error")
        || final_error.is_some()
    {
        eprintln!(
            "error: task {task_id} completed with status={final_status} error={final_error:?}"
        );
        if let Some(val) = &last_value {
            eprintln!(
                "task detail: {}",
                truncate(&serde_json::to_string_pretty(val).unwrap_or_default(), 2000)
            );
        } else if !last_body.is_empty() {
            eprintln!("task detail: {}", truncate(&last_body, 2000));
        }
        std::process::exit(1);
    }

    println!(
        "verified task {task_id} completed with status={final_status} and contains image_asset_pointer"
    );
    Ok(())
}

fn build_attachments(raw_paths: &[String]) -> anyhow::Result<Vec<ImageAttachment>> {
    let mut out = Vec::new();
    for (idx, raw) in raw_paths.iter().enumerate() {
        let fs_path = PathBuf::from(raw);
        if !fs_path.exists() {
            anyhow::bail!("Attachment {} does not exist: {}", idx + 1, raw);
        }
        let submit_path = raw.clone();
        let display_name = fs_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(raw)
            .to_string();
        let size_bytes = std::fs::metadata(&fs_path)
            .with_context(|| format!("metadata failed for {raw}"))?
            .len();
        out.push(ImageAttachment {
            id: AttachmentId::new(idx as u64 + 1),
            fs_path,
            submit_path,
            display_name,
            size_bytes,
        });
    }
    Ok(out)
}

async fn perform_uploads(
    uploader: &mut AttachmentUploader,
    attachments: &[ImageAttachment],
) -> anyhow::Result<Vec<UploadOutcome>> {
    let mut id_to_index = HashMap::new();
    for (idx, att) in attachments.iter().enumerate() {
        uploader
            .start_upload(att.id, att.display_name.clone(), att.fs_path.clone())
            .map_err(|err| {
                anyhow!(
                    "failed to queue upload for {}: {}",
                    att.submit_path,
                    err.message
                )
            })?;
        id_to_index.insert(att.id, idx);
    }

    let mut results: Vec<Option<UploadOutcome>> = vec![None; attachments.len()];

    while results.iter().any(std::option::Option::is_none) {
        let updates = uploader.poll();
        if updates.is_empty() {
            sleep(Duration::from_millis(50)).await;
            continue;
        }
        for update in updates {
            match update {
                AttachmentUploadUpdate::Started { id, total_bytes } => {
                    append_error_log(format!(
                        "upload-debug: id={} started total_bytes={:?}",
                        id.raw(),
                        total_bytes
                    ));
                }
                AttachmentUploadUpdate::Finished { id, result } => match result {
                    Ok(success) => {
                        let idx = *id_to_index
                            .get(&id)
                            .ok_or_else(|| anyhow!("unknown attachment id"))?;
                        let att = &attachments[idx];
                        let (width, height) =
                            image_dimensions(&att.fs_path).with_context(|| {
                                format!("failed to decode image dimensions for {}", att.submit_path)
                            })?;
                        append_error_log(format!(
                            "upload-debug: id={} completed pointer={}",
                            id.raw(),
                            success.asset_pointer.value
                        ));
                        results[idx] = Some(UploadOutcome {
                            pointer: success.asset_pointer,
                            submit_path: att.submit_path.clone(),
                            size_bytes: att.size_bytes,
                            width,
                            height,
                        });
                    }
                    Err(err) => {
                        append_error_log(format!(
                            "upload-debug: id={} failed: {}",
                            id.raw(),
                            err.message
                        ));
                        return Err(anyhow!("upload {} failed: {}", id.raw(), err.message));
                    }
                },
            }
        }
    }

    results
        .into_iter()
        .collect::<Option<Vec<UploadOutcome>>>()
        .ok_or_else(|| anyhow!("upload result missing"))
}

async fn load_chatgpt_auth() -> anyhow::Result<(Option<String>, Option<String>)> {
    if let Ok(home) = find_codex_home() {
        let authm = AuthManager::new(home);
        if let Some(auth) = authm.auth() {
            match auth.get_token().await {
                Ok(token) if !token.is_empty() => {
                    let account_id = auth
                        .get_account_id()
                        .or_else(|| extract_chatgpt_account_id(&token));
                    println!("auth: ChatGPT token loaded ({} chars)", token.len());
                    if let Some(acc) = &account_id {
                        println!("auth: ChatGPT-Account-Id={acc}");
                    }
                    return Ok((Some(token), account_id));
                }
                Ok(_) => {
                    println!("auth: ChatGPT token empty");
                }
                Err(e) => {
                    println!("auth: failed to load token: {e}");
                    append_error_log(format!("upload-debug: auth token load failed: {e}"));
                }
            }
        } else {
            println!("auth: no ChatGPT auth.json");
        }
    } else {
        println!("auth: could not resolve CODEX_HOME");
    }
    Ok((None, None))
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}…", &text[..max])
    }
}
