use tempfile::tempdir;

#[tokio::test]
async fn writes_api_key_and_loads_auth() {
    let dir = tempdir().unwrap();
    codex_login::login_with_api_key(dir.path(), "sk-test-key").unwrap();
    let auth = codex_login::CodexAuth::from_codex_home(dir.path())
        .unwrap()
        .unwrap();
    assert_eq!(auth.mode, codex_login::AuthMode::ApiKey);
    assert_eq!(auth.get_token().await.unwrap().as_str(), "sk-test-key");
}
