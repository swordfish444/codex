use tempfile::tempdir;

#[tokio::test]
async fn writes_api_key_and_loads_auth() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    codex_login::login_with_api_key(dir.path(), "sk-test-key")?;
    let auth = codex_login::CodexAuth::from_codex_home(dir.path())?
        .ok_or_else(|| std::io::Error::other("expected Some(auth)"))?;
    assert_eq!(auth.mode, codex_login::AuthMode::ApiKey);
    assert_eq!(auth.get_token().await?.as_str(), "sk-test-key");
    Ok(())
}
