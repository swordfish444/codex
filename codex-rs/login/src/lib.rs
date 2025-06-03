use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

const SOURCE_FOR_PYTHON_SERVER: &str = include_str!("./login_with_chatgpt.py");

const JSON_PATH_FOR_API_KEY: &str = "OPENAI_API_KEY";

/// Run `python3 -c {{SOURCE_FOR_PYTHON_SERVER}}` with the CODEX_HOME
/// environment variable set to the provided `codex_home` path. If the
/// subprocess exits 0, read the OPENAI_API_KEY property out of
/// CODEX_HOME/auth.json and return Ok(OPENAI_API_KEY). Otherwise, return Err
/// with any information from the subprocess.
pub async fn login_with_chatgpt(codex_home: &Path) -> std::io::Result<String> {
    let child = Command::new("python3")
        .arg("-c")
        .arg(SOURCE_FOR_PYTHON_SERVER)
        .env("CODEX_HOME", codex_home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let output = child.wait_with_output().await?;
    if output.status.success() {
        let auth_path = codex_home.join("auth.json");
        let mut file = fs::File::open(&auth_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let v: serde_json::Value = serde_json::from_str(&contents)?;
        if let Some(api_key) = v.get(JSON_PATH_FOR_API_KEY).and_then(|t| t.as_str()) {
            Ok(api_key.to_string())
        } else {
            Err(std::io::Error::other(format!(
                "{auth_path:?} missing {JSON_PATH_FOR_API_KEY} field"
            )))
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(std::io::Error::other(format!(
            "login_with_chatgpt subprocess failed: {stderr}"
        )))
    }
}
