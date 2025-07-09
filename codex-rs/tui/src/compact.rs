use anyhow::Result;
use anyhow::anyhow;
use codex_core::config::Config;
use codex_core::openai_api_key::get_openai_api_key;
use serde::Serialize;

#[derive(Clone)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone)]
pub struct TranscriptEntry {
    pub role: Role,
    pub text: String,
}

impl TranscriptEntry {
    fn role_str(&self) -> &'static str {
        match self.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: String,
}

#[derive(Serialize)]
struct Payload<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
}

/// Generate a concise summary of the provided transcript using the OpenAI chat
/// completions API.
pub async fn generate_compact_summary(
    transcript: &[TranscriptEntry],
    model: &str,
    config: &Config,
) -> Result<String> {
    let conversation_text = transcript
        .iter()
        .map(|e| format!("{}: {}", e.role_str(), e.text))
        .collect::<Vec<_>>()
        .join("\n");

    let messages = vec![
        Message {
            role: "assistant",
            content: "You are an expert coding assistant. Your goal is to generate a concise, structured summary of the conversation below that captures all essential information needed to continue development after context replacement. Include tasks performed, code areas modified or reviewed, key decisions or assumptions, test results or errors, and outstanding tasks or next steps.".to_string(),
        },
        Message {
            role: "user",
            content: format!(
                "Here is the conversation so far:\n{conversation_text}\n\nPlease summarize this conversation, covering:\n1. Tasks performed and outcomes\n2. Code files, modules, or functions modified or examined\n3. Important decisions or assumptions made\n4. Errors encountered and test or build results\n5. Remaining tasks, open questions, or next steps\nProvide the summary in a clear, concise format."
            ),
        },
    ];

    let api_key = get_openai_api_key().ok_or_else(|| anyhow!("OpenAI API key not set"))?;
    let client = reqwest::Client::new();
    let base = config.model_provider.base_url.trim_end_matches('/');
    let url = format!("{}/chat/completions", base);

    let payload = Payload { model, messages };
    let res = client
        .post(url)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await?;

    let body: serde_json::Value = res.json().await?;
    if let Some(summary) = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
    {
        Ok(summary.to_string())
    } else {
        Ok("Unable to generate summary.".to_string())
    }
}
