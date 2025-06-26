/// Integration test for the `codex config` subcommand.
/// This uses `CARGO_BIN_EXE_codex` to locate the compiled binary.
#[cfg(test)]
mod cli_config {
    use std::fs;
    use std::process::Command;
    use tempfile;
    use toml;

    #[test]
    fn config_subcommand_help() {
        let exe = env!("CARGO_BIN_EXE_codex");
        let output = Command::new(exe)
            .arg("config")
            .arg("--help")
            .output()
            .expect("failed to run codex config --help");
        assert!(output.status.success(), "Exited with {:?}", output.status);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should show config subcommands help
        assert!(stdout.contains("edit"), "help missing 'edit': {}", stdout);
        assert!(stdout.contains("set"), "help missing 'set': {}", stdout);
    }

    #[test]
    fn config_set_and_read() {
        let exe = env!("CARGO_BIN_EXE_codex");
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg_path = tmp.path().join("config.toml");
        let status = Command::new(exe)
            .env("CODEX_HOME", tmp.path())
            .arg("config")
            .arg("set")
            .arg("tui.auto_mount_repo")
            .arg("true")
            .status()
            .expect("failed to run codex config set");
        assert!(status.success());
        let contents = fs::read_to_string(&cfg_path).expect("read config");
        let doc: toml::Value = toml::from_str(&contents).expect("parse config.toml");
        assert_eq!(doc["tui"]["auto_mount_repo"].as_bool(), Some(true));
    }
}
