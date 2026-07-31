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
    Catppuccin,
}

impl Theme {
    pub fn next(self) -> Self {
        match self {
            Self::Midnight => Self::Gruvbox,
            Self::Gruvbox => Self::Catppuccin,
            Self::Catppuccin => Self::Midnight,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Midnight => "MIDNIGHT",
            Self::Gruvbox => "GRUVBOX",
            Self::Catppuccin => "CATPPUCCIN",
        }
    }
}

pub struct App {
    pub events: Vec<OomEvent>,
    pub list_state: ListState,
    pub source_description: String,
    /// Scroll offset within the persistent raw-evidence pane.
    pub raw_scroll: u16,
    pub raw_horizontal_scroll: u16,
    pub detail_scroll: u16,
    raw_max_scroll: u16,
    raw_horizontal_max_scroll: u16,
    detail_max_scroll: u16,
    pub focus: FocusPane,
    pub help_visible: bool,
    pub show_landing: bool,
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
        let show_landing = events.is_empty();
        Self {
            events,
            list_state,
            source_description,
            raw_scroll: 0,
            raw_horizontal_scroll: 0,
            detail_scroll: 0,
            raw_max_scroll: 0,
            raw_horizontal_max_scroll: 0,
            detail_max_scroll: 0,
            focus: FocusPane::Incidents,
            help_visible: false,
            show_landing,
            theme: Theme::Midnight,
            device: DeviceInfo::detect(),
            status: None,
            source_options,
            warning,
        }
    }

    pub fn toggle_landing(&mut self) {
        self.show_landing = !self.show_landing;
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
        self.raw_horizontal_scroll = 0;
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
        self.raw_horizontal_scroll = 0;
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
        self.status = Some(format!(
            "theme — {}",
            self.theme.label().to_ascii_lowercase()
        ));
    }

    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }

    pub fn set_raw_scroll_limits(
        &mut self,
        content_lines: usize,
        viewport_lines: u16,
        max_width: usize,
        viewport_width: u16,
    ) {
        self.raw_max_scroll = content_lines
            .saturating_sub(viewport_lines as usize)
            .min(u16::MAX as usize) as u16;
        self.raw_horizontal_max_scroll = max_width
            .saturating_sub(viewport_width as usize)
            .min(u16::MAX as usize) as u16;
        self.raw_scroll = self.raw_scroll.min(self.raw_max_scroll);
        self.raw_horizontal_scroll = self
            .raw_horizontal_scroll
            .min(self.raw_horizontal_max_scroll);
    }

    pub fn set_detail_scroll_limits(&mut self, content_lines: usize, viewport_lines: u16) {
        self.detail_max_scroll = content_lines
            .saturating_sub(viewport_lines as usize)
            .min(u16::MAX as usize) as u16;
        self.detail_scroll = self.detail_scroll.min(self.detail_max_scroll);
    }

    pub fn scroll_raw(&mut self, delta: i32) {
        self.raw_scroll =
            (self.raw_scroll as i32 + delta).clamp(0, self.raw_max_scroll as i32) as u16;
    }

    pub fn scroll_raw_horizontal(&mut self, delta: i32) {
        self.raw_horizontal_scroll = (self.raw_horizontal_scroll as i32 + delta)
            .clamp(0, self.raw_horizontal_max_scroll as i32)
            as u16;
    }

    pub fn scroll_raw_to(&mut self, end: bool) {
        self.raw_scroll = if end { self.raw_max_scroll } else { 0 };
    }

    pub fn scroll_details(&mut self, delta: i32) {
        self.detail_scroll =
            (self.detail_scroll as i32 + delta).clamp(0, self.detail_max_scroll as i32) as u16;
    }

    pub fn scroll_details_to(&mut self, end: bool) {
        self.detail_scroll = if end { self.detail_max_scroll } else { 0 };
    }
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
        app.set_raw_scroll_limits(2, 1, 20, 10);
        app.set_detail_scroll_limits(3, 1);
        app.scroll_raw(1);
        assert_eq!(app.raw_scroll, 1);

        app.scroll_details(1);
        assert_eq!(app.detail_scroll, 1);
    }

    #[test]
    fn scroll_limits_follow_rendered_viewports() {
        let mut app = app();
        app.set_raw_scroll_limits(10, 4, 40, 12);
        app.set_detail_scroll_limits(12, 5);
        app.scroll_raw(100);
        app.scroll_raw_horizontal(100);
        app.scroll_details(100);
        assert_eq!(app.raw_scroll, 6);
        assert_eq!(app.raw_horizontal_scroll, 28);
        assert_eq!(app.detail_scroll, 7);
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
        assert_eq!(app.theme, Theme::Catppuccin);
        app.cycle_theme();
        assert_eq!(app.theme, Theme::Midnight);
    }
}
