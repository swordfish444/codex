//! Simple round-trip test that serialises a freshly constructed `SessionMeta`
//! and deserialises it back to ensure the schema is self-consistent.

use codex_session::meta::AgentCli;
use codex_session::meta::SessionMeta;
use codex_session::store::SessionKind;

#[test]
fn meta_round_trip() {
    let exec_cli = codex_exec::Cli {
        images: vec![],
        model: Some("gpt-4o-mini".into()),
        skip_git_repo_check: true,
        disable_response_storage: false,
        prompt: Some("hello world".into()),
    };

    let meta = SessionMeta::new(
        "test-session".into(),
        42,
        SessionKind::Exec,
        AgentCli::Exec(exec_cli.clone()),
        exec_cli.prompt.clone(),
    );

    // Serialise with pretty printer so humans can read the file as well.
    let json = serde_json::to_string_pretty(&meta).expect("serialise");

    // … and parse it back.
    let de: SessionMeta = serde_json::from_str(&json).expect("deserialise");

    assert_eq!(de.version, SessionMeta::CURRENT_VERSION);
    assert_eq!(de.id, "test-session");
    assert_eq!(de.pid, 42);
    assert!(matches!(de.cli, AgentCli::Exec(_)));
}
