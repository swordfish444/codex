use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, WidgetRef};
use ratatui::prelude::Widget;
use std::path::PathBuf;

pub enum PickerOutcome {
    Submit(Vec<String>),
    Cancel,
    None,
}

pub struct OllamaModelPickerScreen {
    pub host_root: String,
    pub config_path: PathBuf,
    available: Vec<String>,
    selected: Vec<bool>,
    cursor: usize,
    pub loading: bool,
}

impl OllamaModelPickerScreen {
    pub fn new(host_root: String, config_path: PathBuf, preselected: Vec<String>) -> Self {
        Self {
            host_root,
            config_path,
            available: Vec::new(),
            selected: preselected.into_iter().map(|_| false).collect(),
            cursor: 0,
            loading: true,
        }
    }

    pub fn desired_height(&self, _width: u16) -> u16 {
        18u16
    }

    pub fn update_available(&mut self, available: Vec<String>) {
        // Build selection state using existing selected names where possible.
        let prev_selected_names: Vec<String> = self
            .available
            .iter()
            .cloned()
            .zip(self.selected.iter().cloned())
            .filter_map(|(n, sel)| if sel { Some(n) } else { None })
            .collect();

        self.available = available.clone();
        self.selected = available
            .iter()
            .map(|n| prev_selected_names.iter().any(|p| p == n))
            .collect();
        if self.cursor >= self.available.len() {
            self.cursor = self.available.len().saturating_sub(1);
        }
        self.loading = false;
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> PickerOutcome {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.cursor > 0 { self.cursor -= 1; }
                PickerOutcome::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.cursor + 1 < self.available.len() { self.cursor += 1; }
                PickerOutcome::None
            }
            KeyCode::Char(' ') => {
                if let Some(s) = self.selected.get_mut(self.cursor) {
                    *s = !*s;
                }
                PickerOutcome::None
            }
            KeyCode::Char('a') => {
                let all = self.selected.iter().all(|s| *s);
                self.selected.fill(!all);
                PickerOutcome::None
            }
            KeyCode::Enter => {
                let chosen: Vec<String> = self
                    .available
                    .iter()
                    .cloned()
                    .zip(self.selected.iter().cloned())
                    .filter_map(|(n, sel)| if sel { Some(n) } else { None })
                    .collect();
                PickerOutcome::Submit(chosen)
            }
            KeyCode::Esc | KeyCode::Char('q') => PickerOutcome::Cancel,
            _ => PickerOutcome::None,
        }
    }
}

impl WidgetRef for &OllamaModelPickerScreen {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        const MIN_WIDTH: u16 = 40;
        const MIN_HEIGHT: u16 = 15;
        let popup_width = std::cmp::max(MIN_WIDTH, (area.width as f32 * 0.7) as u16);
        let popup_height = std::cmp::max(MIN_HEIGHT, (area.height as f32 * 0.6) as u16);
        let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        let popup_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .title(Span::styled(
                "Select Ollama models",
                Style::default().add_modifier(Modifier::BOLD),
            ));
        let inner = popup_block.inner(popup_area);
        popup_block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(3)])
            .split(inner);

        // Header
        let header = format!("endpoint: {}\n↑/↓ move, space toggle, 'a' (un)select all, enter confirm, 'q' skip", self.host_root);
        Paragraph::new(header).alignment(Alignment::Left).render(chunks[0], buf);

        // Body: list of models or a loading message
        if self.loading {
            Paragraph::new("discovering models...").alignment(Alignment::Center).render(chunks[1], buf);
        } else if self.available.is_empty() {
            Paragraph::new("No models discovered on the local Ollama instance.")
                .alignment(Alignment::Center)
                .render(chunks[1], buf);
        } else {
            // Render each line manually with highlight for cursor.
            let mut lines: Vec<Line> = Vec::with_capacity(self.available.len());
            for (i, name) in self.available.iter().enumerate() {
                let mark = if self.selected.get(i).copied().unwrap_or(false) { "[x]" } else { "[ ]" };
                let content = format!("{mark} {name}");
                if i == self.cursor {
                    lines.push(Line::from(content).style(Style::default().add_modifier(Modifier::REVERSED)));
                } else {
                    lines.push(Line::from(content));
                }
            }
            Paragraph::new(lines).render(chunks[1], buf);
        }

        // Footer/help
        Paragraph::new("press Enter to save, 'q' to continue without changes")
            .alignment(Alignment::Center)
            .render(chunks[2], buf);
    }
}
