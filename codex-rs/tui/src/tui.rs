use std::io::Result;
use std::io::Stdout;
use std::io::stdout;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use codex_core::config::Config;
use crossterm::event::DisableBracketedPaste;
use crossterm::event::EnableBracketedPaste;
use crossterm::event::KeyboardEnhancementFlags;
use crossterm::event::PopKeyboardEnhancementFlags;
use crossterm::event::PushKeyboardEnhancementFlags;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::disable_raw_mode;
use ratatui::crossterm::terminal::enable_raw_mode;

use crate::custom_terminal::Terminal;

/// A type alias for the terminal type used in this application
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

// Global flag indicating whether Kitty Keyboard Protocol (KKP) appears enabled.
static KKP_ENABLED: AtomicBool = AtomicBool::new(false);

/// Return whether KKP (alternate key reporting) appears enabled.
pub(crate) fn is_kkp_enabled() -> bool {
    KKP_ENABLED.load(Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn set_kkp_for_tests(value: bool) {
    KKP_ENABLED.store(value, Ordering::Relaxed);
}

/// Try to detect Kitty Keyboard Protocol support by issuing a progressive
/// enhancement query and waiting briefly for a response.
#[cfg(unix)]
fn detect_kitty_protocol() -> std::io::Result<bool> {
    use std::io::Read;
    use std::io::Write;
    use std::io::{self};
    use std::os::unix::io::AsRawFd;

    let mut stdout = io::stdout();
    let mut stdin = io::stdin();

    // Send query for progressive enhancement + DA1
    write!(stdout, "\x1b[?u\x1b[c")?;
    stdout.flush()?;

    // Wait up to ~200ms for a response
    let fd = stdin.as_raw_fd();
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let rc = unsafe { libc::poll(&mut pfd as *mut libc::pollfd, 1, 200) };
    if rc > 0 && (pfd.revents & libc::POLLIN) != 0 {
        let mut buf = [0u8; 256];
        if let Ok(n) = stdin.read(&mut buf) {
            let response = String::from_utf8_lossy(&buf[..n]);
            if response.contains("[?") && response.contains('u') {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(not(unix))]
fn detect_kitty_protocol() -> std::io::Result<bool> {
    Ok(false)
}

/// Initialize the terminal (inline viewport; history stays in normal scrollback)
pub fn init(_config: &Config) -> Result<Tui> {
    execute!(stdout(), EnableBracketedPaste)?;

    enable_raw_mode()?;
    // Enable keyboard enhancement flags so modifiers for keys like Enter are disambiguated.
    // chat_composer.rs is using a keyboard event listener to enter for any modified keys
    // to create a new line that require this.
    execute!(
        stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
        )
    )?;

    // Detect KKP availability; used to adjust UI hints in the composer.
    let kkp = detect_kitty_protocol().unwrap_or(false);
    KKP_ENABLED.store(kkp, Ordering::Relaxed);

    set_panic_hook();

    let backend = CrosstermBackend::new(stdout());
    let tui = Terminal::with_options(backend)?;
    Ok(tui)
}

fn set_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore(); // ignore any errors as we are already failing
        hook(panic_info);
    }));
}

/// Restore the terminal to its original state
pub fn restore() -> Result<()> {
    execute!(stdout(), PopKeyboardEnhancementFlags)?;
    execute!(stdout(), DisableBracketedPaste)?;
    disable_raw_mode()?;
    Ok(())
}
