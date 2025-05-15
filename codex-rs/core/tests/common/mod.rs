use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::config::ConfigToml;
use tempfile::TempDir;

/// Note TempDir is required to ensure tests create a unique config directory
/// for each test run so they do not interfere with each other.
pub fn load_default_config_for_test(codex_home: &TempDir) -> Config {
    #[expect(clippy::expect_used)]
    Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        ConfigOverrides::default(),
        codex_home.path().to_path_buf(),
    )
    .expect("defaults for test should always succeed")
}
