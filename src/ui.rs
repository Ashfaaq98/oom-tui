use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // title bar
            Constraint::Percentage(35), // timeline
            Constraint::Min(0),         // detail pane
            Constraint::Length(1),      // help bar
        ])
        .split(f.size());

    draw_title(f, root[0], app);
    draw_timeline(f, root[1], app);
    draw_detail(f, root[2], app);
    draw_help(f, root[3], app);
}

fn draw_title(f: &mut Frame, area: Rect, app: &App) {
    let count = app.events.len();
    let text = format!(
        " oom-tui  —  {count} OOM-kill event{}  —  source: {}",
        if count == 1 { "" } else { "s" },
        app.source_description
    );
    let p = Paragraph::new(text).style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(p, area);
}

fn draw_timeline(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .title(" Timeline (most recent last) ")
        .borders(Borders::ALL);

    if app.events.is_empty() {
        let p = Paragraph::new(
            "No OOM-kill events found in the current log source.\n\
             That's good news for your system's memory health!\n\n\
             To generate a test event: run a memory-hog under a tight cgroup limit,\n\
             or point oom-tui at a log file that contains one with --file <path>.",
        )
        .block(block)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
        return;
    }

    let items: Vec<ListItem> = app
        .events
        .iter()
        .map(|e| {
            let severity_color = severity_color(e);
            ListItem::new(Line::from(vec![Span::styled(
                e.summary_line(),
                Style::default().fg(severity_color),
            )]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn severity_color(e: &crate::model::OomEvent) -> Color {
    match e.rss_total_kb() {
        Some(kb) if kb > 2_000_000 => Color::Red,
        Some(kb) if kb > 500_000 => Color::Yellow,
        _ => Color::Green,
    }
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Autopsy ")
        .borders(Borders::ALL);

    let Some(e) = app.selected() else {
        f.render_widget(
            Paragraph::new("Select an event above to see details.").block(block),
            area,
        );
        return;
    };

    if app.show_raw {
        let raw = e.raw_lines.join("\n");
        let raw_block = Block::default()
            .title(" Autopsy — raw log lines (press 'l' to go back) ")
            .borders(Borders::ALL);
        let p = Paragraph::new(raw)
            .block(raw_block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::Gray));
        f.render_widget(p, area);
        return;
    }

    let label_style = Style::default().fg(Color::Cyan);
    let value_style = Style::default().fg(Color::White);
    let dim = Style::default().fg(Color::DarkGray);

    let row = |label: &str, value: String| {
        Line::from(vec![
            Span::styled(format!("{label:<16}"), label_style),
            Span::styled(value, value_style),
        ])
    };
    let na = |o: Option<String>| o.unwrap_or_else(|| "—".to_string());
    let kb = |v: Option<u64>| v.map(|v| format!("{v} kB ({:.1} MB)", v as f64 / 1024.0));

    let mut lines = vec![
        row(
            "Victim",
            format!("{} (pid {})", e.victim_name, e.victim_pid),
        ),
        row("Timestamp", na(e.timestamp.clone())),
        row("UID", na(e.uid.map(|u| u.to_string()))),
        row(
            "oom_score_adj",
            na(e.oom_score_adj.map(|v| v.to_string())),
        ),
        Line::from(""),
        row(
            "total-vm",
            na(kb(e.total_vm_kb)),
        ),
        row("anon-rss", na(kb(e.anon_rss_kb))),
        row("file-rss", na(kb(e.file_rss_kb))),
        row("shmem-rss", na(kb(e.shmem_rss_kb))),
        row("pgtables", na(kb(e.pgtables_kb))),
        row(
            "RSS total",
            na(kb(e.rss_total_kb())),
        ),
        Line::from(""),
        row("Constraint", na(e.constraint.clone())),
        row("Cgroup", na(e.cgroup.clone())),
        Line::from(""),
        row("Triggered by", na(e.trigger_process.clone())),
        row("gfp_mask", na(e.gfp_mask.clone())),
        row("alloc order", na(e.order.map(|v| v.to_string()))),
        row(
            "Reaped",
            if e.reaped {
                "yes — memory reclaimed".to_string()
            } else {
                "not confirmed in log window".to_string()
            },
        ),
    ];

    lines.push(Line::from(""));
    lines.push(Line::styled(
        "press 'l' to toggle the raw kernel log lines for this event",
        dim,
    ));

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn draw_help(f: &mut Frame, area: Rect, _app: &App) {
    let p = Paragraph::new(
        " ↑/k up   ↓/j down   l raw log   R reload   q quit",
    )
    .style(Style::default().fg(Color::Black).bg(Color::Gray));
    f.render_widget(p, area);
}
