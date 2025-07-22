/// Load env vars from ~/.codex/.env and `$(pwd)/.env`.
pub fn load_dotenv() {
    if let Ok(codex_home) = codex_core::config::find_codex_home() {
        dotenvy::from_path(codex_home.join(".env")).ok();
    }
    dotenvy::dotenv().ok();
}
