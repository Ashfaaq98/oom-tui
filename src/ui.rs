use crate::{app::App, model::OomEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap},
    Frame,
};

const PANEL: Color = Color::Rgb(30, 35, 48);
const SURFACE: Color = Color::Rgb(23, 27, 38);
const MUTED: Color = Color::Rgb(143, 151, 171);
const TEXT: Color = Color::Rgb(229, 231, 235);
const ACCENT: Color = Color::Rgb(96, 165, 250);
const CYAN: Color = Color::Rgb(45, 212, 191);
const CRITICAL: Color = Color::Rgb(248, 113, 113);
const WARNING: Color = Color::Rgb(251, 191, 36);
const GOOD: Color = Color::Rgb(74, 222, 128);

pub fn draw(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(38),
            Constraint::Min(12),
            Constraint::Length(2),
        ])
        .split(f.size());

    f.render_widget(Block::default().style(Style::default().bg(SURFACE)), f.size());
    draw_header(f, root[0], app);
    draw_timeline(f, root[1], app);
    draw_detail(f, root[2], app);
    draw_footer(f, root[3], app);
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PANEL))
        .style(Style::default().bg(SURFACE))
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let count = app.events.len();
    let title = Line::from(vec![
        Span::styled(" OOM", Style::default().fg(CRITICAL).add_modifier(Modifier::BOLD)),
        Span::styled(" // INCIDENT CONSOLE", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
    ]);
    let meta = Line::from(vec![
        Span::styled(format!(" {count} incident{} ", if count == 1 { "" } else { "s" }), Style::default().fg(CYAN)),
        Span::styled("•  ", Style::default().fg(MUTED)),
        Span::styled(&app.source_description, Style::default().fg(MUTED)),
    ]);
    f.render_widget(
        Paragraph::new(vec![title, meta])
            .style(Style::default().bg(SURFACE))
            .alignment(Alignment::Left),
        area,
    );
}

fn draw_timeline(f: &mut Frame, area: Rect, app: &mut App) {
    if app.events.is_empty() {
        let message = vec![
            Line::styled("No OOM kills found", Style::default().fg(GOOD).add_modifier(Modifier::BOLD)),
            Line::styled("The selected kernel log source is clear.", Style::default().fg(MUTED)),
            Line::from(""),
            Line::styled("Tip  Run a memory-limited workload, then press R to refresh.", Style::default().fg(ACCENT)),
        ];
        f.render_widget(
            Paragraph::new(message).block(panel("INCIDENT TIMELINE")).wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app.events.iter().map(timeline_item).collect();
    let list = List::new(items)
        .block(panel("INCIDENT TIMELINE  ·  newest last"))
        .highlight_style(Style::default().bg(PANEL).add_modifier(Modifier::BOLD))
        .highlight_symbol("▌ ");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn timeline_item(event: &OomEvent) -> ListItem<'static> {
    let color = severity_color(event);
    let timestamp = event.timestamp.as_deref().unwrap_or("unknown time");
    let memory = memory(event.rss_total_kb());
    let reaped = if event.reaped { "reclaimed" } else { "unconfirmed" };
    ListItem::new(vec![
        Line::from(vec![
            Span::styled("● ", Style::default().fg(color)),
            Span::styled(event.victim_name.clone(), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  PID {}", event.victim_pid), Style::default().fg(MUTED)),
            Span::styled(format!("  {memory}"), Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(timestamp.to_string(), Style::default().fg(MUTED)),
            Span::styled("  ·  ", Style::default().fg(MUTED)),
            Span::styled(reaped, Style::default().fg(if event.reaped { GOOD } else { WARNING })),
            Span::styled("  ·  ", Style::default().fg(MUTED)),
            Span::styled(event.cgroup.clone().unwrap_or_else(|| "host / unspecified cgroup".to_string()), Style::default().fg(MUTED)),
        ]),
    ])
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let Some(event) = app.selected() else {
        f.render_widget(
            Paragraph::new("Select an incident to inspect its memory profile and kernel context.")
                .style(Style::default().fg(MUTED))
                .alignment(Alignment::Center)
                .block(panel("INCIDENT DETAIL")),
            area,
        );
        return;
    };

    if app.show_raw {
        f.render_widget(
            Paragraph::new(event.raw_lines.join("\n"))
                .block(panel("RAW KERNEL EVIDENCE  ·  press l to return"))
                .style(Style::default().fg(TEXT))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    // The task dump only exists on some logs, so it earns a column only when
    // there is something to put in it.
    let constraints = if event.processes.is_empty() {
        vec![Constraint::Percentage(50), Constraint::Percentage(50)]
    } else {
        vec![
            Constraint::Percentage(36),
            Constraint::Percentage(28),
            Constraint::Percentage(36),
        ]
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    draw_identity(f, columns[0], event);
    draw_memory(f, columns[1], event);
    if !event.processes.is_empty() {
        draw_top_consumers(f, columns[2], event);
    }
}

/// Who else was holding memory when the kernel fired.
///
/// The OOM killer targets the largest resident set, which is regularly not the
/// process responsible for the pressure, so the victim being absent from the
/// top of this list is the single most useful signal in the whole tool.
fn draw_top_consumers(f: &mut Frame, area: Rect, event: &OomEvent) {
    let visible = area.height.saturating_sub(2) as usize;
    let mut rows: Vec<Row<'static>> = Vec::new();

    for process in event.top_consumers(visible.min(20)) {
        let is_victim = process.pid == event.victim_pid;
        let color = if is_victim { CRITICAL } else { TEXT };
        let marker = if is_victim { "▶ " } else { "  " };
        rows.push(Row::new(vec![
            Cell::from(format!("{marker}{}", truncate(&process.name, 16)))
                .style(Style::default().fg(color)),
            Cell::from(format!("{:.0} MiB", process.rss_kb as f64 / 1024.0))
                .style(Style::default().fg(color)),
        ]));
    }

    let title = match event.victim_was_largest() {
        Some(false) => "TOP CONSUMERS  ·  victim was NOT the largest",
        _ => "TOP CONSUMERS",
    };

    let table = Table::new(rows, [Constraint::Min(10), Constraint::Length(10)])
        .block(panel(title))
        .column_spacing(1)
        .style(Style::default().fg(TEXT).bg(SURFACE));
    f.render_widget(table, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

fn draw_identity(f: &mut Frame, area: Rect, event: &OomEvent) {
    let rows = vec![
        detail_row("VICTIM", format!("{}  (PID {})", event.victim_name, event.victim_pid), TEXT),
        detail_row("SCOPE", scope_label(event), if event.memcg_kill { CYAN } else { WARNING }),
        detail_row("WHEN", when(event), TEXT),
        detail_row("UID", option_u32(event.uid), TEXT),
        detail_row("OOM SCORE ADJ", option_i32(event.oom_score_adj), TEXT),
        detail_row("CONSTRAINT", event.constraint.clone().unwrap_or_else(|| "—".to_string()), WARNING),
        detail_row("WORKLOAD", workload(event), CYAN),
        detail_row("CGROUP", event.cgroup.clone().unwrap_or_else(|| "—".to_string()), CYAN),
        detail_row("LIMIT HIT", limit_cgroup(event), WARNING),
        detail_row("TRIGGER", event.trigger_process.clone().unwrap_or_else(|| "—".to_string()), TEXT),
        detail_row("ALLOCATION", allocation(event), TEXT),
        detail_row("REAPER", if event.reaped { "confirmed — memory reclaimed".to_string() } else { "not confirmed in log window".to_string() }, if event.reaped { GOOD } else { WARNING }),
    ];
    f.render_widget(detail_table(rows, "INCIDENT CONTEXT"), area);
}

/// The first question to answer about any containerised kill, because the two
/// answers have completely different fixes: raise the limit / fix the leak, or
/// stop oversubscribing the host.
fn scope_label(event: &OomEvent) -> String {
    // Kept short deliberately: this renders in a half-width column and the
    // longer phrasing was silently truncated at the panel edge.
    if event.memcg_kill {
        "cgroup / container limit".to_string()
    } else {
        "host-wide exhaustion".to_string()
    }
}

fn draw_memory(f: &mut Frame, area: Rect, event: &OomEvent) {
    let total_color = severity_color(event);
    let rows = vec![
        detail_row("RSS TOTAL", memory(event.rss_total_kb()), total_color),
        detail_row("ANONYMOUS RSS", memory(event.anon_rss_kb), TEXT),
        detail_row("FILE RSS", memory(event.file_rss_kb), TEXT),
        detail_row("SHMEM RSS", memory(event.shmem_rss_kb), TEXT),
        detail_row("PAGE TABLES", memory(event.pgtables_kb), TEXT),
        detail_row("TOTAL VIRTUAL", memory(event.total_vm_kb), MUTED),
        detail_row("SHARE OF RAM", share_of_ram(event), total_color),
        detail_row("MACHINE RAM", memory(event.mem.as_ref().and_then(|m| m.total_ram_kb)), MUTED),
        detail_row("RAW LINES", format!("{} captured", event.raw_lines.len()), MUTED),
    ];
    f.render_widget(detail_table(rows, "MEMORY AUTOPSY"), area);
}

fn detail_table(rows: Vec<Row<'static>>, title: &'static str) -> Table<'static> {
    Table::new(rows, [Constraint::Length(16), Constraint::Min(8)])
        .block(panel(title))
        .column_spacing(1)
        .style(Style::default().fg(TEXT).bg(SURFACE))
}

fn detail_row(label: &'static str, value: String, color: Color) -> Row<'static> {
    Row::new(vec![
        Cell::from(label).style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)),
        Cell::from(value).style(Style::default().fg(color)),
    ])
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut help = vec![
        Span::styled(" ↑/k ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)), Span::styled("navigate   ", Style::default().fg(MUTED)),
        Span::styled("l ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)), Span::styled("raw evidence   ", Style::default().fg(MUTED)),
        Span::styled("R ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)), Span::styled("reload   ", Style::default().fg(MUTED)),
        Span::styled("q ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)), Span::styled("quit", Style::default().fg(MUTED)),
    ];
    if let Some(status) = &app.status {
        help.push(Span::styled("   │   ", Style::default().fg(MUTED)));
        help.push(Span::styled(status.clone(), Style::default().fg(WARNING).add_modifier(Modifier::BOLD)));
    }
    f.render_widget(
        Paragraph::new(Line::from(help)).style(Style::default().bg(PANEL)).alignment(Alignment::Center),
        area,
    );
}

/// Prefer share-of-RAM when the log told us how much RAM the machine had.
/// Absolute thresholds are misleading on their own: 400 MB is noise on a 64 GB
/// host and fatal on a 512 MB VM.
fn severity_color(event: &OomEvent) -> Color {
    if let Some(percent) = event.rss_share_of_ram() {
        return match percent {
            p if p >= 50.0 => CRITICAL,
            p if p >= 20.0 => WARNING,
            _ => GOOD,
        };
    }
    match event.rss_total_kb() {
        Some(kb) if kb > 2_000_000 => CRITICAL,
        Some(kb) if kb > 500_000 => WARNING,
        _ => GOOD,
    }
}

/// Human-readable workload identity decoded from the cgroup path.
fn workload(event: &OomEvent) -> String {
    event
        .cgroup
        .as_deref()
        .and_then(crate::container::identify)
        .map(|id| id.summary())
        .unwrap_or_else(|| "—".to_string())
}

/// Which cgroup's limit was actually breached. Only interesting when it differs
/// from where the task lived, which is the parent-slice case.
fn limit_cgroup(event: &OomEvent) -> String {
    match (&event.limit_cgroup, &event.cgroup) {
        (Some(limit), Some(task)) if limit == task => "own cgroup limit".to_string(),
        (Some(limit), _) => format!("parent: {limit}"),
        (None, _) => "—".to_string(),
    }
}

fn share_of_ram(event: &OomEvent) -> String {
    event
        .rss_share_of_ram()
        .map(|percent| format!("{percent:.1}% of system memory"))
        .unwrap_or_else(|| "— (no Mem-Info in log)".to_string())
}

fn memory(kb: Option<u64>) -> String {
    kb.map(|value| format!("{:.1} MiB  ·  {value} kB", value as f64 / 1024.0))
        .unwrap_or_else(|| "—".to_string())
}

fn option_u32(value: Option<u32>) -> String { value.map(|v| v.to_string()).unwrap_or_else(|| "—".to_string()) }
fn option_i32(value: Option<i32>) -> String { value.map(|v| v.to_string()).unwrap_or_else(|| "—".to_string()) }

fn allocation(event: &OomEvent) -> String {
    match (&event.gfp_mask, event.order) {
        (Some(mask), Some(order)) => format!("order {order}  ·  {mask}"),
        _ => "—".to_string(),
    }
}

/// Prefer resolved wall-clock time with a relative hint; fall back to the raw
/// stamp when the log's epoch could not be trusted.
fn when(event: &OomEvent) -> String {
    let raw = event.timestamp.as_deref().unwrap_or("—");
    match event.occurred_at {
        Some(at) => format!("{}  ({})", at.format("%Y-%m-%d %H:%M:%S"), ago(at)),
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
