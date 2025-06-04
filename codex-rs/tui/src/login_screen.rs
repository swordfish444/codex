use std::path::PathBuf;

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

pub(crate) struct LoginScreen {
    /// Use this with login_with_chatgpt() in login/src/lib.rs and, if
    /// successful, update the in-memory config via
    /// codex_core::openai_api_key::set_openai_api_key().
    #[allow(dead_code)]
    codex_home: PathBuf,
}

impl LoginScreen {
    pub(crate) fn new(codex_home: PathBuf) -> Self {
        Self { codex_home }
    }

    pub(crate) fn handle_key_event(&mut self, _key_event: KeyEvent) {
        // TODO: Handle key events.
    }
}

impl WidgetRef for &LoginScreen {
    fn render_ref(&self, _area: Rect, _buf: &mut Buffer) {
        // TODO: Draw things.
    }
}
