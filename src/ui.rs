use crate::{app::App, model::OomEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap},
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

    f.render_widget(Block::default().style(Style::default().bg(SURFACE)), area);
    draw_header(f, root[0], app);
    draw_timeline(f, root[1], app);
    draw_detail(f, root[2], app);
    draw_footer(f, root[3], app);
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
    let title = Line::from(vec![
        Span::styled(" OOM", Style::default().fg(CRITICAL).add_modifier(Modifier::BOLD)),
        Span::styled(
            " // INCIDENT CONSOLE",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ]);
    let source_width = area.width.saturating_sub(18) as usize;
    let meta = Line::from(vec![
        Span::styled(
            format!(" {count} incident{} ", if count == 1 { "" } else { "s" }),
            Style::default().fg(CYAN),
        ),
        Span::styled("•  ", Style::default().fg(MUTED)),
        Span::styled(
            truncate_to_width(&app.source_description, source_width),
            Style::default().fg(MUTED),
        ),
    ]);
    f.render_widget(Paragraph::new(vec![title, meta]).style(Style::default().bg(SURFACE)), area);
}

fn draw_timeline(f: &mut Frame, area: Rect, app: &mut App) {
    if app.events.is_empty() {
        let message = vec![
            Line::styled("No OOM kills found", Style::default().fg(GOOD).add_modifier(Modifier::BOLD)),
            Line::styled("The selected kernel log source is clear.", Style::default().fg(MUTED)),
        ];
        f.render_widget(
            Paragraph::new(message)
                .block(panel(panel_title("INCIDENT TIMELINE")))
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
        .block(panel(panel_title("INCIDENT TIMELINE  ·  newest last")))
        .highlight_style(Style::default().bg(BLUE).fg(TEXT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▌ ");
    f.render_stateful_widget(list, area, &mut app.list_state);
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

    if app.show_details {
        f.render_widget(
            Paragraph::new(full_detail_lines(event))
                .block(panel(panel_title("INCIDENT DETAILS  ·  i to return")))
                .style(Style::default().fg(TEXT))
                .scroll((app.detail_scroll, 0))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let peers = peer_count(event);
    match area.width {
        110.. => draw_wide_detail(f, area, event, peers),
        80..=109 => draw_medium_detail(f, area, event, peers),
        _ => draw_narrow_detail(f, area, event, peers),
    }
}

fn draw_wide_detail(f: &mut Frame, area: Rect, event: &OomEvent, peers: usize) {
    let columns = if peers == 0 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(36),
                Constraint::Percentage(28),
                Constraint::Percentage(36),
            ])
            .split(area)
    };
    draw_context(f, columns[0], event);
    draw_memory(f, columns[1], event);
    if peers > 0 {
        draw_top_consumers(f, columns[2], event);
    }
}

fn draw_medium_detail(f: &mut Frame, area: Rect, event: &OomEvent, peers: usize) {
    if peers == 0 {
        draw_wide_detail(f, area, event, peers);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(54), Constraint::Percentage(46)])
        .split(rows[0]);
    draw_context(f, columns[0], event);
    draw_memory(f, columns[1], event);
    draw_top_consumers(f, rows[1], event);
}

fn draw_narrow_detail(f: &mut Frame, area: Rect, event: &OomEvent, peers: usize) {
    if peers > 0 {
        let rows =
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);
        draw_compact_summary(f, rows[0], event);
        draw_top_consumers(f, rows[1], event);
    } else {
        draw_compact_summary(f, area, event);
    }
}

fn draw_context(f: &mut Frame, area: Rect, event: &OomEvent) {
    let value_width = detail_value_width(area, 12);
    let rows = vec![
        detail_row("VICTIM", format!("{} (PID {})", event.victim_name, event.victim_pid), TEXT, value_width),
        detail_row("SCOPE", scope_label(event), CYAN, value_width),
        detail_row("WHEN", when(event), TEXT, value_width),
        detail_row("WORKLOAD", workload(event), CYAN, value_width),
        detail_row("TRIGGER", present(event.trigger_process.clone()), TEXT, value_width),
        detail_row("REAPER", reaper(event), if event.reaped { GOOD } else { WARNING }, value_width),
        detail_row("TASK DUMP", task_dump_state(event), MUTED, value_width),
    ];
    f.render_widget(detail_table(rows, "INCIDENT CONTEXT", 12), area);
}

fn draw_memory(f: &mut Frame, area: Rect, event: &OomEvent) {
    let value_width = detail_value_width(area, 14);
    let impact = impact(event);
    let rows = vec![
        detail_row("RSS TOTAL", memory(event.rss_total_kb()), impact.color(), value_width),
        detail_row("ANONYMOUS", memory(event.anon_rss_kb), TEXT, value_width),
        detail_row("FILE RSS", memory(event.file_rss_kb), TEXT, value_width),
        detail_row("SHMEM RSS", memory(event.shmem_rss_kb), TEXT, value_width),
        detail_row("PAGE TABLES", memory(event.pgtables_kb), TEXT, value_width),
        detail_row("RAM SHARE", share_of_ram(event), impact.color(), value_width),
        detail_row(
            "MACHINE RAM",
            memory(event.mem.as_ref().and_then(|m| m.total_ram_kb)),
            MUTED,
            value_width,
        ),
    ];
    f.render_widget(detail_table(rows, "MEMORY", 14), area);
}

fn draw_compact_summary(f: &mut Frame, area: Rect, event: &OomEvent) {
    let value_width = detail_value_width(area, 10);
    let impact = impact(event);
    let rows = vec![
        detail_row("VICTIM", format!("{} (PID {})", event.victim_name, event.victim_pid), TEXT, value_width),
        detail_row("IMPACT", impact.label().to_string(), impact.color(), value_width),
        detail_row("SCOPE", scope_short(event).to_string(), CYAN, value_width),
        detail_row("RSS", memory(event.rss_total_kb()), impact.color(), value_width),
        detail_row("WORKLOAD", workload(event), CYAN, value_width),
        detail_row("REAPER", reaper(event), if event.reaped { GOOD } else { WARNING }, value_width),
    ];
    f.render_widget(detail_table(rows, "INCIDENT SUMMARY  ·  i for complete details", 10), area);
}

fn draw_top_consumers(f: &mut Frame, area: Rect, event: &OomEvent) {
    let visible = area.height.saturating_sub(2) as usize;
    let name_width = area.width.saturating_sub(15).max(8) as usize;
    let mut rows: Vec<Row<'static>> = Vec::new();
    for process in event
        .top_consumers(visible.min(20))
        .into_iter()
        .filter(|process| process.pid != event.victim_pid)
    {
        rows.push(Row::new(vec![
            Cell::from(truncate_to_width(&process.name, name_width)).style(Style::default().fg(TEXT)),
            Cell::from(format!("{:.0} MiB", process.rss_kb as f64 / 1024.0))
                .style(Style::default().fg(TEXT)),
        ]));
    }
    let title = match event.victim_was_largest() {
        Some(false) => "OTHER CONSUMERS  ·  victim was not largest",
        _ => "OTHER CONSUMERS",
    };
    let table = Table::new(rows, [Constraint::Min(8), Constraint::Length(10)])
        .block(panel(panel_title(title)))
        .column_spacing(1)
        .style(Style::default().fg(TEXT).bg(SURFACE));
    f.render_widget(table, area);
}

fn detail_table(rows: Vec<Row<'static>>, title: impl Into<String>, label_width: u16) -> Table<'static> {
    Table::new(rows, [Constraint::Length(label_width), Constraint::Min(8)])
        .block(panel(panel_title(title)))
        .column_spacing(1)
        .style(Style::default().fg(TEXT).bg(SURFACE))
}

fn detail_row(label: &'static str, value: String, color: Color, width: usize) -> Row<'static> {
    let color = if value == MISSING { MUTED } else { color };
    Row::new(vec![
        Cell::from(label).style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)),
        Cell::from(truncate_to_width(&value, width)).style(Style::default().fg(color)),
    ])
}

fn detail_value_width(area: Rect, label_width: u16) -> usize {
    area.width.saturating_sub(label_width + 5).max(8) as usize
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut help = vec![
        shortcut("↑/k", "navigate"),
        shortcut("i", "details"),
        shortcut("l", "raw"),
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

fn task_dump_state(event: &OomEvent) -> String {
    match (event.processes.len(), peer_count(event)) {
        (0, _) => "not captured in log".to_string(),
        (_, 0) => "victim only".to_string(),
        (_, peers) => format!("{peers} peer process{}", if peers == 1 { "" } else { "es" }),
    }
}

fn peer_count(event: &OomEvent) -> usize {
    event
        .processes
        .iter()
        .filter(|process| process.pid != event.victim_pid)
        .count()
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
            assert!(output.contains("OTHER CONSUMERS"));
        }
    }

    #[test]
    fn victim_only_dump_has_no_redundant_consumer_panel() {
        let output = render(140, 40, event(false, false));
        assert!(output.contains("victim only"));
        assert!(!output.contains("OTHER CONSUMERS"));
        assert!(output.contains("not reported"));
    }
}
