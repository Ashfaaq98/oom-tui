use crate::model::OomEvent;
use crate::source::SourceOptions;
use crate::system::DeviceInfo;
use ratatui::widgets::ListState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Incidents,
    Details,
    Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Midnight,
    Gruvbox,
}

impl Theme {
    pub fn next(self) -> Self {
        match self {
            Self::Midnight => Self::Gruvbox,
            Self::Gruvbox => Self::Midnight,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Midnight => "MIDNIGHT",
            Self::Gruvbox => "GRUVBOX",
        }
    }
}

pub struct App {
    pub events: Vec<OomEvent>,
    pub list_state: ListState,
    pub source_description: String,
    /// Scroll offset within the persistent raw-evidence pane.
    pub raw_scroll: u16,
    pub detail_scroll: u16,
    pub focus: FocusPane,
    pub theme: Theme,
    pub device: DeviceInfo,
    pub status: Option<String>,
    /// Kept so `r` can re-query the exact same source, including a `--file`
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
            raw_scroll: 0,
            detail_scroll: 0,
            focus: FocusPane::Incidents,
            theme: Theme::Midnight,
            device: DeviceInfo::detect(),
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
        self.detail_scroll = 0;
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
        self.detail_scroll = 0;
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            FocusPane::Incidents => FocusPane::Details,
            FocusPane::Details => FocusPane::Evidence,
            FocusPane::Evidence => FocusPane::Incidents,
        };
    }

    pub fn focus_details(&mut self) {
        self.focus = FocusPane::Details;
    }

    pub fn focus_evidence(&mut self) {
        self.focus = FocusPane::Evidence;
    }

    pub fn cycle_theme(&mut self) {
        self.theme = self.theme.next();
        self.status = Some(format!("theme — {}", self.theme.label().to_ascii_lowercase()));
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

    pub fn scroll_details(&mut self, delta: i32) {
        let max = self
            .selected()
            .map(detail_line_count)
            .unwrap_or(0)
            .saturating_sub(1) as u16;
        self.detail_scroll = (self.detail_scroll as i32 + delta).clamp(0, max as i32) as u16;
    }

    pub fn scroll_details_to(&mut self, end: bool) {
        self.detail_scroll = if end {
            self.selected()
                .map(detail_line_count)
                .unwrap_or(0)
                .saturating_sub(1) as u16
        } else {
            0
        };
    }
}

fn detail_line_count(event: &OomEvent) -> usize {
    17 + event.processes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(
            vec![OomEvent {
                raw_lines: vec!["one".to_string(), "two".to_string()],
                ..Default::default()
            }],
            "test".to_string(),
            SourceOptions::default(),
            None,
        )
    }

    #[test]
    fn evidence_and_details_have_independent_scroll_positions() {
        let mut app = app();
        app.scroll_raw(1);
        assert_eq!(app.raw_scroll, 1);

        app.scroll_details(1);
        assert_eq!(app.detail_scroll, 1);
    }

    #[test]
    fn changing_incidents_resets_evidence_scroll_positions() {
        let mut app = app();
        app.events.push(OomEvent::default());
        app.list_state.select(Some(0));
        app.raw_scroll = 1;
        app.detail_scroll = 1;
        app.select_next();
        assert_eq!(app.raw_scroll, 0);
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn focus_cycles_through_all_master_detail_panes() {
        let mut app = app();
        assert_eq!(app.focus, FocusPane::Incidents);
        app.focus_next();
        assert_eq!(app.focus, FocusPane::Details);
        app.focus_next();
        assert_eq!(app.focus, FocusPane::Evidence);
        app.focus_next();
        assert_eq!(app.focus, FocusPane::Incidents);
    }

    #[test]
    fn themes_cycle_through_every_available_palette() {
        let mut app = app();
        assert_eq!(app.theme, Theme::Midnight);
        app.cycle_theme();
        assert_eq!(app.theme, Theme::Gruvbox);
        app.cycle_theme();
        assert_eq!(app.theme, Theme::Midnight);
    }
}
