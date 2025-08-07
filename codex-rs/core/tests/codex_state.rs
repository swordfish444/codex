#![cfg(unix)]
#![allow(clippy::expect_used)]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use codex_core::codex_state::CodexState;
use codex_core::codex_state::Project;
use codex_core::codex_state::lookup_project;
use codex_core::codex_state::update_project;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::config::ConfigToml;
use tempfile::TempDir;

/// Build a Config for tests with a temporary `codex_home` and a specific `cwd`.
fn make_config(codex_home: &TempDir, cwd: PathBuf) -> Config {
    Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        ConfigOverrides {
            cwd: Some(cwd),
            ..Default::default()
        },
        codex_home.path().to_path_buf(),
    )
    .expect("config construction for tests should succeed")
}

#[tokio::test]
async fn lookup_returns_default_for_missing_or_empty_state() {
    // Given a fresh codex_home with no state file
    let codex_home = TempDir::new().expect("create temp codex_home");
    let project_dir = TempDir::new().expect("create temp project dir");
    let cfg = make_config(&codex_home, project_dir.path().to_path_buf());

    // When we lookup the project
    let project = lookup_project(&cfg).await.expect("lookup should succeed");

    // Then the default should be returned
    assert!(!project.trusted);

    // And the state file should have been created (empty file acceptable)
    let state_path = cfg.codex_home.join("codex-state.json");
    assert!(state_path.exists());
}

#[tokio::test]
async fn update_then_lookup_roundtrips_and_sets_permissions() {
    let codex_home = TempDir::new().expect("create temp codex_home");
    let project_dir = TempDir::new().expect("create temp project dir");
    let cfg = make_config(&codex_home, project_dir.path().to_path_buf());

    // Update project state to trusted = true
    let p = Project { trusted: true };
    update_project(&cfg, &p)
        .await
        .expect("update should succeed");

    // Verify file exists with correct JSON structure
    let state_path = cfg.codex_home.join("codex-state.json");
    let contents = fs::read_to_string(&state_path).expect("read state file");
    let state: CodexState = serde_json::from_str(&contents).expect("parse state JSON");

    assert_eq!(
        state,
        CodexState {
            projects: HashMap::from([(
                project_dir.path().to_path_buf(),
                Project { trusted: true }
            )])
        }
    );

    // Lookup should now return the updated value
    let looked_up = lookup_project(&cfg).await.expect("lookup should succeed");
    assert!(looked_up.trusted);
}

#[tokio::test]
async fn lookup_handles_invalid_json_gracefully() {
    let codex_home = TempDir::new().expect("create temp codex_home");
    let project_dir = TempDir::new().expect("create temp project dir");
    let cfg = make_config(&codex_home, project_dir.path().to_path_buf());

    // Write invalid JSON into the state file
    let state_path = cfg.codex_home.join("codex-state.json");
    fs::create_dir_all(&cfg.codex_home).expect("create codex_home dir");
    fs::write(&state_path, b"this is not json").expect("write invalid json");

    // Lookup should not error and should return defaults
    let project = lookup_project(&cfg).await.expect("lookup should succeed");
    assert!(!project.trusted);
}
