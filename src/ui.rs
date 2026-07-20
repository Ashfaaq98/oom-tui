use crate::{
    app::{App, FocusPane},
    model::OomEvent,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const SURFACE: Color = Color::Rgb(23, 27, 38);
const PANEL: Color = Color::Rgb(30, 35, 48);
const BORDER: Color = Color::Rgb(100, 116, 139);
const MUTED: Color = Color::Rgb(148, 163, 184);
const TEXT: Color = Color::Rgb(226, 232, 240);
const BLUE: Color = Color::Rgb(59, 130, 246);
const CYAN: Color = Color::Rgb(34, 211, 238);
const CRITICAL: Color = Color::Rgb(248, 113, 113);
const WARNING: Color = Color::Rgb(251, 191, 36);
const GOOD: Color = Color::Rgb(74, 222, 128);
const MISSING: &str = "— not reported in log";

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.size();
    f.render_widget(Block::default().style(Style::default().bg(SURFACE)), area);
    if area.width >= 90 {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(0), Constraint::Length(2)])
            .split(area);
        draw_header(f, root[0], app);
        draw_master_detail(f, root[1], app);
        draw_footer(f, root[2], app);
    } else {
        let timeline = timeline_height(area.height, app.events.len());
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(timeline),
                Constraint::Min(0),
                Constraint::Length(2),
            ])
            .split(area);
        draw_header(f, root[0], app);
        draw_incident_list(f, root[1], app, "INCIDENT TIMELINE  ·  newest last");
        draw_detail(f, root[2], app);
        draw_footer(f, root[3], app);
    }
}

fn timeline_height(terminal_height: u16, events: usize) -> u16 {
    let usable = terminal_height.saturating_sub(5);
    let max_timeline = (usable * 2 / 5).min(usable.saturating_sub(12));
    let wanted = if events <= 1 {
        5
    } else {
        2 + (events.min(4) as u16 * 2)
    };
    wanted.min(max_timeline.max(3))
}

fn panel(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE))
}

fn panel_title(title: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {} ", title.into()),
        Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
    ))
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let count = app.events.len();
    let cgroup_count = app.events.iter().filter(|event| event.memcg_kill).count();
    let host_count = count.saturating_sub(cgroup_count);
    let selected = app.list_state.selected().map(|index| index + 1).unwrap_or(0);
    let title = Line::from(vec![
        Span::styled(" OOM", Style::default().fg(CRITICAL).add_modifier(Modifier::BOLD)),
        Span::styled(
            " // INCIDENT CONSOLE",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  │  ", Style::default().fg(MUTED)),
        Span::styled(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  │  ", Style::default().fg(MUTED)),
        Span::styled("KERNEL LOG FORENSICS", Style::default().fg(MUTED)),
    ]);
    let meta = Line::from(vec![
        Span::styled(
            format!(" {count} INCIDENT{} ", if count == 1 { "" } else { "S" }),
            Style::default().fg(CYAN),
        ),
        Span::styled(format!(" {cgroup_count} CGROUP "), Style::default().fg(CYAN)),
        Span::styled("│", Style::default().fg(BORDER)),
        Span::styled(format!(" {host_count} HOST-WIDE "), Style::default().fg(CYAN)),
        Span::styled("│", Style::default().fg(BORDER)),
        Span::styled(format!(" SELECTED {selected}/{count} "), Style::default().fg(TEXT)),
        Span::styled("│", Style::default().fg(BORDER)),
        Span::styled(
            truncate_to_width(&app.source_description, area.width.saturating_sub(56) as usize),
            Style::default().fg(MUTED),
        ),
    ]);
    f.render_widget(
        Paragraph::new(vec![title, meta])
            .style(Style::default().bg(PANEL))
            .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(BORDER))),
        area,
    );
}

fn draw_incident_list(f: &mut Frame, area: Rect, app: &mut App, title: &str) {
    if app.events.is_empty() {
        let message = vec![
            Line::styled("No OOM kills found", Style::default().fg(GOOD).add_modifier(Modifier::BOLD)),
            Line::styled("The selected kernel log source is clear.", Style::default().fg(MUTED)),
        ];
        f.render_widget(
            Paragraph::new(message)
                .block(panel(panel_title(title)))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let item_width = area.width.saturating_sub(5) as usize;
    let items: Vec<ListItem> = app
        .events
        .iter()
        .map(|event| timeline_item(event, item_width))
        .collect();
    let list = List::new(items)
        .block(panel(panel_title(title)))
        .highlight_style(
            Style::default()
                .bg(if app.focus == FocusPane::Incidents { BLUE } else { PANEL })
                .fg(TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_master_detail(f: &mut Frame, area: Rect, app: &mut App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(area);
    draw_incident_list(f, columns[0], app, "INCIDENTS  ·  j/k select");

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(columns[1]);
    draw_detail(f, right[0], app);
    draw_raw_evidence(f, right[1], app);
}

fn draw_raw_evidence(f: &mut Frame, area: Rect, app: &App) {
    let Some(event) = app.selected() else {
        f.render_widget(
            Paragraph::new("Select an incident to inspect its unmodified kernel evidence.")
                .style(Style::default().fg(MUTED))
                .block(panel(panel_title("RAW KERNEL EVIDENCE"))),
            area,
        );
        return;
    };
    let title = if app.focus == FocusPane::Evidence {
        "RAW KERNEL EVIDENCE  ·  FOCUSED  ·  j/k scroll"
    } else {
        "RAW KERNEL EVIDENCE  ·  l or Tab to focus"
    };
    f.render_widget(
        Paragraph::new(event.raw_lines.join("\n"))
            .block(panel(panel_title(title)))
            .style(Style::default().fg(TEXT))
            .scroll((app.raw_scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn timeline_item(event: &OomEvent, width: usize) -> ListItem<'static> {
    let impact = impact(event);
    let timestamp = event.timestamp.as_deref().unwrap_or("unknown time");
    let first = format!(
        "● {:<8} {} · PID {} · {}",
        impact.label(),
        event.victim_name,
        event.victim_pid,
        memory(event.rss_total_kb())
    );
    let second = format!(
        "  {} · {} · {}",
        timestamp,
        scope_short(event),
        if event.reaped { "reclaimed" } else { "not confirmed" }
    );
    ListItem::new(vec![
        Line::styled(truncate_to_width(&first, width), Style::default().fg(impact.color())),
        Line::styled(truncate_to_width(&second, width), Style::default().fg(MUTED)),
    ])
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let Some(event) = app.selected() else {
        f.render_widget(
            Paragraph::new("Select an incident to inspect its recorded kernel context.")
                .style(Style::default().fg(MUTED))
                .alignment(Alignment::Center)
                .block(panel(panel_title("INCIDENT DETAIL"))),
            area,
        );
        return;
    };

    if app.show_raw {
        f.render_widget(
            Paragraph::new(event.raw_lines.join("\n"))
                .block(panel(panel_title("RAW KERNEL EVIDENCE  ·  l to return")))
                .style(Style::default().fg(TEXT))
                .scroll((app.raw_scroll, 0))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let title = if app.focus == FocusPane::Details {
        "INCIDENT DETAILS  ·  FOCUSED  ·  j/k scroll"
    } else {
        "INCIDENT DETAILS  ·  Tab to focus"
    };
    f.render_widget(
        Paragraph::new(full_detail_lines(event))
            .block(panel(panel_title(title)))
            .style(Style::default().fg(TEXT))
            .scroll((app.detail_scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut help = vec![
        shortcut("Tab", focus_label(app.focus)),
        shortcut("j/k", "move/scroll"),
        shortcut("i", "details"),
        shortcut("l", "evidence"),
        shortcut("R", "reload"),
        shortcut("q", "quit"),
    ];
    if let Some(status) = &app.status {
        help.push(Span::styled("  │  ", Style::default().fg(MUTED)));
        help.push(Span::styled(status, Style::default().fg(WARNING).add_modifier(Modifier::BOLD)));
    }
    f.render_widget(
        Paragraph::new(Line::from(help))
            .style(Style::default().bg(PANEL))
            .alignment(Alignment::Center),
        area,
    );
}

fn focus_label(focus: FocusPane) -> &'static str {
    match focus {
        FocusPane::Incidents => "incidents",
        FocusPane::Details => "details",
        FocusPane::Evidence => "evidence",
    }
}

fn shortcut(key: &'static str, label: &'static str) -> Span<'static> {
    Span::styled(
        format!(" {key}:{label} "),
        Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Impact {
    Low,
    Elevated,
    Critical,
    Unknown,
}

impl Impact {
    fn label(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Elevated => "ELEVATED",
            Self::Critical => "CRITICAL",
            Self::Unknown => "UNKNOWN",
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Low => GOOD,
            Self::Elevated => WARNING,
            Self::Critical => CRITICAL,
            Self::Unknown => MUTED,
        }
    }
}

fn impact(event: &OomEvent) -> Impact {
    match event.rss_share_of_ram() {
        Some(percent) if percent >= 50.0 => Impact::Critical,
        Some(percent) if percent >= 20.0 => Impact::Elevated,
        Some(_) => Impact::Low,
        None => Impact::Unknown,
    }
}

fn scope_label(event: &OomEvent) -> String {
    if event.memcg_kill {
        "cgroup / container limit".to_string()
    } else {
        "host-wide exhaustion".to_string()
    }
}

fn scope_short(event: &OomEvent) -> &'static str {
    if event.memcg_kill {
        "cgroup limit"
    } else {
        "host-wide"
    }
}

fn workload(event: &OomEvent) -> String {
    present(
        event
            .cgroup
            .as_deref()
            .and_then(crate::container::identify)
            .map(|id| id.summary()),
    )
}

fn reaper(event: &OomEvent) -> String {
    if event.reaped {
        "confirmed — memory reclaimed".to_string()
    } else {
        "not confirmed in log".to_string()
    }
}

fn share_of_ram(event: &OomEvent) -> String {
    event
        .rss_share_of_ram()
        .map(|percent| format!("{percent:.1}% of machine RAM"))
        .unwrap_or_else(|| MISSING.to_string())
}

fn memory(kb: Option<u64>) -> String {
    kb.map(|value| format!("{:.1} MiB", value as f64 / 1024.0))
        .unwrap_or_else(|| MISSING.to_string())
}

fn present(value: Option<String>) -> String {
    value.unwrap_or_else(|| MISSING.to_string())
}

fn when(event: &OomEvent) -> String {
    let raw = event.timestamp.as_deref().unwrap_or(MISSING);
    match event.occurred_at {
        Some(at) => format!("{} ({})", at.format("%Y-%m-%d %H:%M:%S"), ago(at)),
        None => raw.to_string(),
    }
}

fn ago(at: chrono::DateTime<chrono::Local>) -> String {
    let delta = chrono::Local::now() - at;
    let minutes = delta.num_minutes();
    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if delta.num_hours() < 48 {
        format!("{}h ago", delta.num_hours())
    } else {
        format!("{}d ago", delta.num_days())
    }
}

fn full_detail_lines(event: &OomEvent) -> Vec<Line<'static>> {
    let mut lines = vec![
        full_row("Victim", format!("{} (PID {})", event.victim_name, event.victim_pid)),
        full_row("Impact", impact(event).label()),
        full_row("Scope", scope_label(event)),
        full_row("When", when(event)),
        full_row("UID", event.uid.map(|id| id.to_string()).unwrap_or_else(|| MISSING.to_string())),
        full_row("OOM score adj", event.oom_score_adj.map(|value| value.to_string()).unwrap_or_else(|| MISSING.to_string())),
        full_row("Workload", workload(event)),
        full_row("Cgroup", present(event.cgroup.clone())),
        full_row("Limit cgroup", present(event.limit_cgroup.clone())),
        full_row("Constraint", present(event.constraint.clone())),
        full_row("Trigger", present(event.trigger_process.clone())),
        full_row("Allocation", allocation(event)),
        Line::from(""),
        full_row("RSS total", exact_memory(event.rss_total_kb())),
        full_row("Anonymous RSS", exact_memory(event.anon_rss_kb)),
        full_row("File RSS", exact_memory(event.file_rss_kb)),
        full_row("Shared RSS", exact_memory(event.shmem_rss_kb)),
        full_row("Page tables", exact_memory(event.pgtables_kb)),
        full_row("Total virtual", exact_memory(event.total_vm_kb)),
        full_row("Machine RAM", exact_memory(event.mem.as_ref().and_then(|m| m.total_ram_kb))),
        full_row("RAM share", share_of_ram(event)),
        full_row("Reaper", reaper(event)),
        Line::from(""),
        Line::styled(" Process snapshot", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
    ];
    if event.processes.is_empty() {
        lines.push(Line::styled(format!("  {MISSING}"), Style::default().fg(MUTED)));
    } else {
        for process in event.top_consumers(usize::MAX) {
            lines.push(Line::from(format!(
                "  {:>7}  {:<24}  {}",
                process.pid,
                process.name,
                exact_memory(Some(process.rss_kb))
            )));
        }
    }
    lines
}

fn full_row(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {label:<16}"), Style::default().fg(MUTED).add_modifier(Modifier::BOLD)),
        Span::styled(value.into(), Style::default().fg(TEXT)),
    ])
}

fn allocation(event: &OomEvent) -> String {
    match (&event.gfp_mask, event.order) {
        (Some(mask), Some(order)) => format!("order {order} · {mask}"),
        _ => MISSING.to_string(),
    }
}

fn exact_memory(kb: Option<u64>) -> String {
    kb.map(|value| format!("{:.1} MiB · {value} KiB", value as f64 / 1024.0))
        .unwrap_or_else(|| MISSING.to_string())
}

fn truncate_to_width(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut width = 0;
    for character in value.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width + character_width + 1 > max_width {
            break;
        }
        out.push(character);
        width += character_width;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MemInfo, ProcessEntry};
    use crate::source::SourceOptions;
    use ratatui::{backend::TestBackend, Terminal};

    fn event(with_peers: bool, with_memory: bool) -> OomEvent {
        let mut event = OomEvent {
            timestamp: Some("+12.5s".to_string()),
            victim_name: "worker-with-a-very-long-name".to_string(),
            victim_pid: 42,
            anon_rss_kb: Some(512_000),
            file_rss_kb: Some(0),
            shmem_rss_kb: Some(0),
            cgroup: Some("/kubepods.slice/a-cgroup-path-that-is-deliberately-long".to_string()),
            raw_lines: vec!["raw evidence".to_string()],
            ..Default::default()
        };
        if with_memory {
            event.mem = Some(MemInfo {
                total_ram_kb: Some(1_024_000),
                ..Default::default()
            });
        }
        event.processes.push(ProcessEntry {
            pid: 42,
            name: "worker-with-a-very-long-name".to_string(),
            rss_kb: 512_000,
            ..Default::default()
        });
        if with_peers {
            event.processes.push(ProcessEntry {
                pid: 77,
                name: "peer-with-a-very-long-name".to_string(),
                rss_kb: 600_000,
                ..Default::default()
            });
        }
        event
    }

    fn render(width: u16, height: u16, event: OomEvent) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut app = App::new(vec![event], "test log".to_string(), SourceOptions::default(), None);
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn impact_requires_recorded_machine_memory() {
        assert_eq!(impact(&event(false, false)), Impact::Unknown);
        assert_eq!(impact(&event(false, true)), Impact::Critical);
    }

    #[test]
    fn truncation_respects_display_width_and_marks_omission() {
        let truncated = truncate_to_width("wide界界value", 7);
        assert!(UnicodeWidthStr::width(truncated.as_str()) <= 7);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn wide_medium_and_narrow_layouts_render() {
        for (width, height) in [(140, 40), (100, 30), (70, 24)] {
            let output = render(width, height, event(true, true));
            assert!(output.contains("INCIDENT"));
            assert!(output.contains("INCIDENT DETAILS"));
        }
    }

    #[test]
    fn wide_layout_keeps_navigation_and_raw_evidence_visible() {
        let output = render(140, 40, event(true, true));
        assert!(output.contains("SELECTED 1/1"));
        assert!(output.contains("RAW KERNEL EVIDENCE"));
        assert!(output.contains("raw evidence"));
    }

    #[test]
    fn incident_details_keep_missing_fields_explicit() {
        let output = render(140, 40, event(false, false));
        assert!(output.contains("not reported"));
    }
}
