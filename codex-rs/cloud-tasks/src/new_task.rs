use codex_tui::ComposerAction;
use codex_tui::ComposerInput;

#[derive(Default)]
pub struct NewTaskPage {
    pub composer: ComposerInput,
    pub submitting: bool,
    pub env_id: Option<String>,
}

impl NewTaskPage {
    pub fn new(env_id: Option<String>) -> Self {
        let mut composer = ComposerInput::new();
        composer.set_hint_items(vec![
            ("⏎", "send"),
            ("Shift+⏎", "newline"),
            ("Ctrl+O", "env"),
            ("Ctrl+C", "quit"),
        ]);
        Self {
            composer,
            submitting: false,
            env_id,
        }
    }

    pub fn can_submit(&self) -> bool {
        self.env_id.is_some() && !self.submitting
    }
}
