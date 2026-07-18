use crate::model::OomEvent;
use crate::source::SourceOptions;
use ratatui::widgets::ListState;

pub struct App {
    pub events: Vec<OomEvent>,
    pub list_state: ListState,
    pub source_description: String,
    pub show_raw: bool,
    /// Scroll offset within the raw-log pane. Without this the pane silently
    /// truncates long events, which defeats its whole purpose as the escape
    /// hatch for checking the parse.
    pub raw_scroll: u16,
    pub status: Option<String>,
    /// Kept so `R` can re-query the exact same source, including a `--file`
    /// path or a `--boot`/`--since` window.
    pub source_options: SourceOptions,
    /// Set when the log source could not honour the requested filters.
    pub warning: Option<String>,
}

impl App {
    pub fn new(
        events: Vec<OomEvent>,
        source_description: String,
        source_options: SourceOptions,
        warning: Option<String>,
    ) -> Self {
        let mut list_state = ListState::default();
        if !events.is_empty() {
            list_state.select(Some(events.len() - 1)); // most recent by default
        }
        Self {
            events,
            list_state,
            source_description,
            show_raw: false,
            raw_scroll: 0,
            status: None,
            source_options,
            warning,
        }
    }

    pub fn selected(&self) -> Option<&OomEvent> {
        self.list_state.selected().and_then(|i| self.events.get(i))
    }

    pub fn select_next(&mut self) {
        if self.events.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) if i + 1 < self.events.len() => i + 1,
            Some(i) => i,
            None => 0,
        };
        self.list_state.select(Some(i));
        self.raw_scroll = 0;
    }

    pub fn select_prev(&mut self) {
        if self.events.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(0) | None => 0,
            Some(i) => i - 1,
        };
        self.list_state.select(Some(i));
        self.raw_scroll = 0;
    }

    pub fn toggle_raw(&mut self) {
        self.show_raw = !self.show_raw;
        self.raw_scroll = 0;
    }

    pub fn scroll_raw(&mut self, delta: i32) {
        let max = self
            .selected()
            .map(|e| e.raw_lines.len().saturating_sub(1) as u16)
            .unwrap_or(0);
        self.raw_scroll = (self.raw_scroll as i32 + delta).clamp(0, max as i32) as u16;
    }

    pub fn scroll_raw_to(&mut self, end: bool) {
        self.raw_scroll = if end {
            self.selected()
                .map(|e| e.raw_lines.len().saturating_sub(1) as u16)
                .unwrap_or(0)
        } else {
            0
        };
    }
}
