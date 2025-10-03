use std::path::Path;
use std::time::Duration;

pub(crate) fn test_delay_for_path(path: &Path) -> Option<Duration> {
    let Ok(config) = std::env::var("CODEX_TEST_READ_FILE_DELAYS") else {
        return None;
    };

    if config.is_empty() {
        return None;
    }

    let target = path.to_string_lossy();
    for entry in config.split(';') {
        if entry.is_empty() {
            continue;
        }
        let Some((candidate, delay_ms)) = entry.split_once('=') else {
            continue;
        };
        if candidate != target {
            continue;
        }
        if let Ok(ms) = delay_ms.parse::<u64>()
            && ms > 0
        {
            return Some(Duration::from_millis(ms));
        }
    }

    None
}
