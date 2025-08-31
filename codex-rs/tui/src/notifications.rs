use std::process::Command;

/// Send a simple OS notification with a fixed app title.
/// Best-effort and silently ignores errors if the platform/tooling is unavailable.
pub fn send_os_notification(message: &str) {
    #[cfg(target_os = "macos")]
    {
        fn detect_bundle_id() -> Option<&'static str> {
            use std::env;
            // Common terminal mappings.
            let term_program = env::var("TERM_PROGRAM").unwrap_or_default();
            match term_program.as_str() {
                "Apple_Terminal" => Some("com.apple.Terminal"),
                "iTerm.app" | "iTerm2" | "iTerm2.app" => Some("com.googlecode.iterm2"),
                "WezTerm" => Some("com.github.wez.wezterm"),
                "Alacritty" => Some("io.alacritty"),
                other => {
                    // Fallback heuristics.
                    let term = env::var("TERM").unwrap_or_default();
                    if other.to_lowercase().contains("kitty") || term.contains("xterm-kitty") {
                        Some("net.kovidgoyal.kitty")
                    } else {
                        None
                    }
                }
            }
        }

        // Prefer terminal-notifier on macOS and attempt to activate the current terminal on click.
        let mut cmd = Command::new("terminal-notifier");
        cmd.arg("-title").arg("Codex").arg("-message").arg(message);
        if let Some(bundle) = detect_bundle_id() {
            cmd.arg("-activate").arg(bundle);
        }
        let _ = cmd.spawn();
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Use notify-send if available (Linux/BSD). Title first, then body.
        let _ = Command::new("notify-send")
            .arg("Codex")
            .arg(message)
            .spawn();
    }

    #[cfg(target_os = "windows")]
    {
        // Best-effort: try a lightweight Toast via PowerShell if available.
        // Fall back to no-op if this fails.
        let ps = r#"
Add-Type -AssemblyName System.Windows.Forms | Out-Null
[System.Windows.Forms.MessageBox]::Show($args[0], 'Codex') | Out-Null
"#;
        let _ = Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(ps)
            .arg(message)
            .spawn();
    }
}
