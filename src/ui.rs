use crate::{
    analysis::investigate,
    app::{App, FocusPane, Theme},
    model::OomEvent,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const MISSING: &str = "— not reported by kernel";

#[derive(Clone, Copy)]
struct Palette {
    surface: Color,
    panel: Color,
    border: Color,
    muted: Color,
    text: Color,
    selection: Color,
    accent: Color,
    critical: Color,
    warning: Color,
    good: Color,
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Midnight => Palette {
            surface: Color::Rgb(23, 27, 38),
            panel: Color::Rgb(30, 35, 48),
            border: Color::Rgb(100, 116, 139),
            muted: Color::Rgb(148, 163, 184),
            text: Color::Rgb(226, 232, 240),
            selection: Color::Rgb(59, 130, 246),
            accent: Color::Rgb(34, 211, 238),
            critical: Color::Rgb(248, 113, 113),
            warning: Color::Rgb(251, 191, 36),
            good: Color::Rgb(74, 222, 128),
        },
        Theme::Gruvbox => Palette {
            surface: Color::Rgb(40, 40, 40),
            panel: Color::Rgb(60, 56, 54),
            border: Color::Rgb(146, 131, 116),
            muted: Color::Rgb(168, 153, 132),
            text: Color::Rgb(235, 219, 178),
            selection: Color::Rgb(69, 133, 136),
            accent: Color::Rgb(142, 192, 124),
            critical: Color::Rgb(251, 73, 52),
            warning: Color::Rgb(250, 189, 47),
            good: Color::Rgb(184, 187, 38),
        },
        Theme::Catppuccin => Palette {
            surface: Color::Rgb(30, 30, 46),
            panel: Color::Rgb(24, 24, 37),
            border: Color::Rgb(108, 112, 134),
            muted: Color::Rgb(166, 173, 200),
            text: Color::Rgb(205, 214, 244),
            selection: Color::Rgb(137, 180, 250),
            accent: Color::Rgb(203, 166, 247),
            critical: Color::Rgb(243, 139, 168),
            warning: Color::Rgb(249, 226, 175),
            good: Color::Rgb(166, 227, 161),
        },
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.size();
    let colors = palette(app.theme);
    f.render_widget(
        Block::default().style(Style::default().bg(colors.surface)),
        area,
    );
    if app.show_landing {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(0),
                Constraint::Length(2),
            ])
            .split(area);
        draw_header(f, root[0], app, colors);
        draw_landing_page(f, root[1], app, colors);
        draw_footer(f, root[2], app, colors);
    } else if area.width >= 90 {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(0),
                Constraint::Length(2),
            ])
            .split(area);
        draw_header(f, root[0], app, colors);
        draw_master_detail(f, root[1], app, colors);
        draw_footer(f, root[2], app, colors);
    } else {
        let timeline = timeline_height(area.height, app.events.len());
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(timeline),
                Constraint::Min(0),
                Constraint::Length(2),
            ])
            .split(area);
        draw_header(f, root[0], app, colors);
        draw_incident_list(f, root[1], app, "INCIDENT TIMELINE  ·  newest last", colors);
        draw_detail(f, root[2], app, colors);
        draw_footer(f, root[3], app, colors);
    }
    if app.help_visible {
        draw_help(f, area, colors);
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

fn panel(title: impl Into<Line<'static>>, colors: Palette) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.border))
        .style(Style::default().bg(colors.surface))
}

fn panel_title(title: impl Into<String>, colors: Palette) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {} ", title.into()),
        Style::default()
            .fg(colors.accent)
            .add_modifier(Modifier::BOLD),
    ))
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, colors: Palette) {
    let count = app.events.len();
    let cgroup_count = app.events.iter().filter(|event| event.memcg_kill).count();
    let selected = app
        .list_state
        .selected()
        .map(|index| index + 1)
        .unwrap_or(0);
    let selected_scope = app.selected().map(scope_short).unwrap_or("no incident");
    let mode_label = if app.show_landing {
        "LANDING DASHBOARD"
    } else {
        "INCIDENT CONSOLE"
    };
    let title = Line::from(vec![
        Span::styled(
            " OOM",
            Style::default()
                .fg(colors.critical)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " INCIDENT FORENSICS",
            Style::default()
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  │  ", Style::default().fg(colors.muted)),
        Span::styled(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(colors.muted),
        ),
        Span::styled("  │  ", Style::default().fg(colors.muted)),
        Span::styled(
            format!("[ MODE: {mode_label} ]"),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let context = Line::from(vec![
        header_value(
            format!(" {count} INCIDENT{} ", if count == 1 { "" } else { "S" }),
            colors,
        ),
        separator(colors),
        header_value(format!(" SELECTED {selected}/{count} "), colors),
        separator(colors),
        header_value(format!(" {selected_scope} "), colors),
        separator(colors),
        Span::styled(
            format!(" {cgroup_count} CGROUP "),
            Style::default().fg(colors.muted),
        ),
        separator(colors),
        Span::styled(
            format!(" {} ", app.device.ram),
            Style::default().fg(colors.accent),
        ),
    ]);
    let viewing = vec![
        Span::styled(
            " VIEWING: ",
            Style::default()
                .fg(colors.muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_to_width(
                &app.source_description,
                area.width.saturating_sub(12) as usize,
            ),
            Style::default().fg(colors.text),
        ),
    ];
    f.render_widget(
        Paragraph::new(vec![title, context, Line::from(viewing)])
            .style(Style::default().bg(colors.panel))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(colors.border)),
            ),
        area,
    );
}

fn draw_landing_page(f: &mut Frame, area: Rect, app: &App, colors: Palette) {
    if area.width >= 80 && area.height >= 16 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(54), Constraint::Percentage(46)])
            .split(area);

        let left_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(9), Constraint::Min(0)])
            .split(columns[0]);

        draw_landing_hero(f, left_rows[0], colors);
        draw_landing_system(f, left_rows[1], app, colors);

        let right_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(12), Constraint::Min(0)])
            .split(columns[1]);

        draw_landing_quick_actions(f, right_rows[0], colors);
        draw_landing_guide(f, right_rows[1], colors);
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(9),
                Constraint::Length(11),
                Constraint::Min(0),
            ])
            .split(area);
        draw_landing_hero(f, rows[0], colors);
        draw_landing_quick_actions(f, rows[1], colors);
        draw_landing_system(f, rows[2], app, colors);
    }
}

fn draw_landing_hero(f: &mut Frame, area: Rect, colors: Palette) {
    let logo = vec![
        Line::styled(
            r"  ██████╗  ██████╗ ███╗   ███╗  ████████╗██╗  ██╗██╗",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            r" ██╔═══██╗██╔═══██╗████╗ ████║  ╚══██╔══╝██║  ██║██║",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            r" ██║   ██║██║   ██║██╔████╔██║     ██║   ██║  ██║██║",
            Style::default().fg(colors.accent),
        ),
        Line::styled(
            r" ╚██████╔╝╚██████╔╝██║ ╚═╝ ██║     ██║   ╚██████╔╝██║",
            Style::default().fg(colors.border),
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " Linux Kernel OOM Incident Forensics Console ",
                Style::default()
                    .fg(colors.text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(colors.muted),
            ),
        ]),
        Line::styled(
            " Reconstructs scattered kernel log lines into precise incident evidence.",
            Style::default().fg(colors.muted),
        ),
    ];
    f.render_widget(
        Paragraph::new(logo).block(panel(panel_title("WELCOME TO OOM-TUI", colors), colors)),
        area,
    );
}

fn draw_landing_system(f: &mut Frame, area: Rect, app: &App, colors: Palette) {
    let mut lines = vec![
        section("HOST ENVIRONMENT SPECS", colors),
        spec_row("OS Release", &app.device.os, colors.text, colors),
        spec_row("Processor", &app.device.cpu, colors.text, colors),
        spec_row("Graphics", &app.device.gpu, colors.text, colors),
        spec_row("System RAM", &app.device.ram, colors.accent, colors),
        Line::from(""),
        section("ACTIVE LOG SOURCE STATUS", colors),
        spec_row(
            "Target Source",
            &app.source_description,
            colors.text,
            colors,
        ),
    ];

    if app.is_loading {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            "  ⏳ SCANNING LOG SOURCE IN BACKGROUND...",
            Style::default()
                .fg(colors.warning)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::styled(
            format!("  {}", app.loading_message),
            Style::default().fg(colors.accent),
        ));
    } else if app.events.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            "  ● SYSTEM HEALTH: CLEAN (0 OOM Kills)",
            Style::default()
                .fg(colors.good)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::styled(
            "  No Out-Of-Memory kills detected in current log source window.",
            Style::default().fg(colors.muted),
        ));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            format!(
                "  ● ALERT: {} OOM INCIDENT(S) LOADED AND READY",
                app.events.len()
            ),
            Style::default()
                .fg(colors.critical)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::styled(
            "  Press 'h' to open the Master-Detail Incident Console view.",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(warning) = &app.warning {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            format!("  ⚠ Warning: {warning}"),
            Style::default().fg(colors.warning),
        ));
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(panel(
                panel_title("SYSTEM & HEALTH MONITOR", colors),
                colors,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn spec_row(label: &str, value: &str, value_color: Color, colors: Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {label:<14} "),
            Style::default()
                .fg(colors.muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(value_color)),
    ])
}

fn draw_landing_quick_actions(f: &mut Frame, area: Rect, colors: Palette) {
    let lines = vec![
        Line::styled(
            " Press hotkey to query log source:",
            Style::default().fg(colors.muted),
        ),
        Line::from(""),
        quick_action_item(
            "1",
            "Scan Current Boot Journal",
            "journalctl -k (boot 0)",
            colors,
        ),
        quick_action_item(
            "2",
            "Scan All Journal Boots",
            "journalctl --all-boots",
            colors,
        ),
        quick_action_item("3", "Inspect Previous Boot", "journalctl -b -1", colors),
        quick_action_item(
            "4",
            "Load Sample OOM Log File",
            "examples/sample-oom.log",
            colors,
        ),
        quick_action_item("h", "Toggle Incident Console", "Master-Detail view", colors),
        quick_action_item(
            "t",
            "Cycle Theme Palette",
            "Midnight/Gruvbox/Catppuccin",
            colors,
        ),
        quick_action_item("?", "Keybind Shortcuts Guide", "Help popup modal", colors),
    ];

    f.render_widget(
        Paragraph::new(lines).block(panel(
            panel_title("QUICK ACTIONS & SOURCES", colors),
            colors,
        )),
        area,
    );
}

fn quick_action_item(
    key: &'static str,
    title: &'static str,
    desc: &'static str,
    colors: Palette,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" [{key}] "),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{title:<26}"),
            Style::default()
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {desc}"), Style::default().fg(colors.muted)),
    ])
}

fn draw_landing_guide(f: &mut Frame, area: Rect, colors: Palette) {
    let lines = vec![
        section("HOW OOM-TUI FORENSICS WORKS", colors),
        Line::styled(
            " • Host vs Cgroup:",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            "   Differentiates host-wide exhaustion from cgroup limit breaches.",
            Style::default().fg(colors.text),
        ),
        Line::from(""),
        Line::styled(
            " • Culprit Identification:",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            "   Flags when victim process was collateral damage rather than culprit.",
            Style::default().fg(colors.text),
        ),
        Line::from(""),
        Line::styled(
            " • Raw Kernel Evidence:",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            "   Preserves exact dmesg / journal lines for audit and proof.",
            Style::default().fg(colors.text),
        ),
    ];

    f.render_widget(
        Paragraph::new(lines).block(panel(panel_title("FORENSICS CHEAT-SHEET", colors), colors)),
        area,
    );
}

fn header_value(value: String, colors: Palette) -> Span<'static> {
    Span::styled(
        value,
        Style::default()
            .fg(colors.accent)
            .add_modifier(Modifier::BOLD),
    )
}

fn separator(colors: Palette) -> Span<'static> {
    Span::styled("│", Style::default().fg(colors.border))
}

fn draw_incident_list(f: &mut Frame, area: Rect, app: &mut App, title: &str, colors: Palette) {
    if app.events.is_empty() {
        let message = vec![
            Line::styled(
                "No OOM kills found",
                Style::default()
                    .fg(colors.good)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(
                "The selected kernel log source is clear.",
                Style::default().fg(colors.muted),
            ),
        ];
        f.render_widget(
            Paragraph::new(message)
                .block(panel(panel_title(title, colors), colors))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let item_width = area.width.saturating_sub(5) as usize;
    let items: Vec<ListItem> = app
        .events
        .iter()
        .map(|event| timeline_item(event, item_width, colors))
        .collect();
    let list = List::new(items)
        .block(panel(panel_title(title, colors), colors))
        .highlight_style(
            Style::default()
                .bg(if app.focus == FocusPane::Incidents {
                    colors.selection
                } else {
                    colors.panel
                })
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_master_detail(f: &mut Frame, area: Rect, app: &mut App, colors: Palette) {
    let left_width = if app.events.len() <= 4 { 28 } else { 34 };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_width),
            Constraint::Percentage(100 - left_width),
        ])
        .split(area);
    draw_incident_list(f, columns[0], app, "INCIDENTS  ·  ↑/↓ select", colors);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(columns[1]);
    draw_detail(f, right[0], app, colors);
    draw_raw_evidence(f, right[1], app, colors);
}

fn draw_raw_evidence(f: &mut Frame, area: Rect, app: &mut App, colors: Palette) {
    let Some(event) = app.selected() else {
        f.render_widget(
            Paragraph::new("Select an incident to inspect its unmodified kernel evidence.")
                .style(Style::default().fg(colors.muted))
                .block(panel(panel_title("RAW KERNEL EVIDENCE", colors), colors)),
            area,
        );
        return;
    };
    let raw_lines = event.raw_lines.clone();
    let viewport_lines = area.height.saturating_sub(2);
    let viewport_width = area.width.saturating_sub(2);
    let max_width = raw_lines
        .iter()
        .map(|line| UnicodeWidthStr::width(line.as_str()))
        .max()
        .unwrap_or(0);
    app.set_raw_scroll_limits(raw_lines.len(), viewport_lines, max_width, viewport_width);
    let title = if app.focus == FocusPane::Evidence {
        "RAW KERNEL EVIDENCE  ·  FOCUSED  ·  ↑/↓ vertical  ←/→ horizontal"
    } else {
        "RAW KERNEL EVIDENCE  ·  Tab to focus"
    };
    let lines = raw_lines
        .iter()
        .map(|line| highlight_raw_line(line, colors))
        .collect::<Vec<_>>();
    f.render_widget(
        Paragraph::new(lines)
            .block(panel(panel_title(title, colors), colors))
            .style(Style::default().fg(colors.text))
            .scroll((app.raw_scroll, app.raw_horizontal_scroll)),
        area,
    );
    draw_vertical_scrollbar(
        f,
        area,
        raw_lines.len(),
        viewport_lines,
        app.raw_scroll,
        colors,
    );
}

fn timeline_item(event: &OomEvent, width: usize, colors: Palette) -> ListItem<'static> {
    let impact = impact(event);
    let ram_bar = match event.rss_share_of_ram() {
        Some(pct) => {
            let filled = ((pct / 100.0) * 5.0).clamp(0.0, 5.0) as usize;
            format!(
                " [ {}{} {:.0}% ]",
                "█".repeat(filled),
                "░".repeat(5 - filled),
                pct
            )
        }
        None => "".to_string(),
    };
    let first = format!(
        "{} {:<4} {} · PID {}{}",
        impact.marker(),
        impact.label(),
        event.victim_name,
        event.victim_pid,
        ram_bar
    );
    let second = format!(
        "  [{}] · {} · {}",
        scope_short(event).to_ascii_uppercase(),
        if event.reaped {
            "✓ reaped"
        } else {
            "⧖ pending"
        },
        event.timestamp.as_deref().unwrap_or("unknown time")
    );
    ListItem::new(vec![
        Line::styled(
            truncate_to_width(&first, width),
            Style::default().fg(impact.color(colors)),
        ),
        Line::styled(
            truncate_to_width(&second, width),
            Style::default().fg(colors.muted),
        ),
    ])
}

fn draw_detail(f: &mut Frame, area: Rect, app: &mut App, colors: Palette) {
    let Some(event) = app.selected() else {
        f.render_widget(
            Paragraph::new("Select an incident to inspect its recorded kernel context.")
                .style(Style::default().fg(colors.muted))
                .alignment(Alignment::Center)
                .block(panel(panel_title("INCIDENT INVESTIGATION", colors), colors)),
            area,
        );
        return;
    };
    let lines = detail_lines(event, colors);
    let content_lines = wrapped_line_count(&lines, area.width.saturating_sub(2) as usize);
    app.set_detail_scroll_limits(content_lines, area.height.saturating_sub(2));
    let title = if app.focus == FocusPane::Details {
        "INCIDENT INVESTIGATION  ·  FOCUSED  ·  ↑/↓ scroll"
    } else {
        "INCIDENT INVESTIGATION  ·  Tab to focus"
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(panel(panel_title(title, colors), colors))
            .style(Style::default().fg(colors.text))
            .scroll((app.detail_scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
    draw_vertical_scrollbar(
        f,
        area,
        content_lines,
        area.height.saturating_sub(2),
        app.detail_scroll,
        colors,
    );
}

fn draw_vertical_scrollbar(
    f: &mut Frame,
    area: Rect,
    content_lines: usize,
    viewport_lines: u16,
    position: u16,
    colors: Palette,
) {
    if content_lines <= viewport_lines as usize {
        return;
    }
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .thumb_symbol("█")
        .track_symbol(Some("│"))
        .thumb_style(Style::default().fg(colors.accent))
        .track_style(Style::default().fg(colors.border));
    let mut state = ScrollbarState::new(content_lines)
        .viewport_content_length(viewport_lines as usize)
        .position(position as usize);
    f.render_stateful_widget(
        scrollbar,
        area.inner(&Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut state,
    );
}

fn wrapped_line_count(lines: &[Line<'_>], width: usize) -> usize {
    if width == 0 {
        return lines.len();
    }
    lines
        .iter()
        .map(|line| {
            let line_width = line
                .spans
                .iter()
                .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                .sum::<usize>();
            line_width.max(1).div_ceil(width)
        })
        .sum()
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App, colors: Palette) {
    let mut help = if app.show_landing {
        vec![
            shortcut("h", "console", colors),
            shortcut("1-4", "sources", colors),
            shortcut("r", "reload", colors),
            shortcut("t", "theme", colors),
            shortcut("?", "help", colors),
            shortcut("q", "quit", colors),
        ]
    } else {
        let nav_action = match app.focus {
            FocusPane::Incidents => "select",
            FocusPane::Details | FocusPane::Evidence => "scroll",
        };
        let mut items = vec![
            shortcut("h", "landing", colors),
            shortcut("Tab", focus_label(app.focus), colors),
            shortcut("↑/↓", nav_action, colors),
        ];
        if app.focus == FocusPane::Evidence {
            items.push(shortcut("←/→", "h-scroll", colors));
        }
        items.extend([
            shortcut("r", "reload", colors),
            shortcut("t", "theme", colors),
            shortcut("?", "help", colors),
            shortcut("q", "quit", colors),
        ]);
        items
    };

    help.push(separator(colors));
    help.push(Span::styled(
        format!(" {} ", app.theme.label().to_ascii_lowercase()),
        Style::default().fg(colors.muted),
    ));

    if app.is_loading {
        help.push(separator(colors));
        help.push(Span::styled(
            format!(" ⏳ SCANNING: {} ", app.loading_message),
            Style::default()
                .fg(colors.warning)
                .add_modifier(Modifier::BOLD),
        ));
    } else if let Some(status) = &app.status {
        help.push(separator(colors));
        let status_color = if status.contains("0 event") || status.contains("CLEAN") {
            colors.good
        } else if status.contains("Failed") || status.contains("❌") {
            colors.critical
        } else {
            colors.warning
        };
        help.push(Span::styled(
            format!(" {status} "),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(help))
            .style(Style::default().bg(colors.panel))
            .alignment(Alignment::Center),
        area,
    );
}

fn draw_help(f: &mut Frame, area: Rect, colors: Palette) {
    let popup = centered_rect(74, 18, area);
    let lines = vec![
        Line::styled(
            " Navigation",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(" h / Home  toggle landing page dashboard"),
        Line::from(" Tab       cycle incidents, investigation, evidence"),
        Line::from(" ↑/↓      select an incident or scroll focused pane"),
        Line::from(" PgUp/PgDn, g/G  fast scroll focused pane"),
        Line::from(" ←/→      horizontal evidence scroll"),
        Line::from(""),
        Line::styled(
            " Actions",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(" 1 - 4     quick switch log sources (1:current, 2:all, 3:prev, 4:sample)"),
        Line::from(" r         reload current log source"),
        Line::from(" t         cycle theme (Midnight -> Gruvbox -> Catppuccin)"),
        Line::from(" ? / Esc   close help popup"),
        Line::from(" q         quit application"),
    ];
    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(lines)
            .block(panel(panel_title("HELP", colors), colors))
            .style(Style::default().bg(colors.panel).fg(colors.text)),
        popup,
    );
}

fn centered_rect(width_percent: u16, requested_height: u16, area: Rect) -> Rect {
    let width = (area.width.saturating_mul(width_percent) / 100).max(1);
    let height = requested_height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn focus_label(focus: FocusPane) -> &'static str {
    match focus {
        FocusPane::Incidents => "incidents",
        FocusPane::Details => "details",
        FocusPane::Evidence => "evidence",
    }
}

fn shortcut(key: &'static str, label: &'static str, colors: Palette) -> Span<'static> {
    Span::styled(
        format!(" {key}:{label} "),
        Style::default()
            .fg(colors.accent)
            .add_modifier(Modifier::BOLD),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Impact {
    Low,
    High,
    Critical,
    Unknown,
}

impl Impact {
    fn label(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
            Self::Unknown => "UNKNOWN",
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Self::Low => ".",
            Self::High => "!!",
            Self::Critical => "!!!",
            Self::Unknown => "?",
        }
    }

    fn color(self, colors: Palette) -> Color {
        match self {
            Self::Low => colors.good,
            Self::High => colors.warning,
            Self::Critical => colors.critical,
            Self::Unknown => colors.warning,
        }
    }
}

fn impact(event: &OomEvent) -> Impact {
    match event.rss_share_of_ram() {
        Some(percent) if percent >= 50.0 => Impact::Critical,
        Some(percent) if percent >= 20.0 => Impact::High,
        Some(_) => Impact::Low,
        None => Impact::Unknown,
    }
}

fn scope_label(event: &OomEvent) -> &'static str {
    if event.memcg_kill {
        "cgroup memory limit"
    } else {
        "host-wide exhaustion"
    }
}

fn scope_short(event: &OomEvent) -> &'static str {
    if event.memcg_kill {
        "cgroup"
    } else {
        "host"
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

fn reaper(event: &OomEvent) -> &'static str {
    if event.reaped {
        "confirmed — memory reclaimed"
    } else {
        "not confirmed in log"
    }
}

fn memory(kb: Option<u64>) -> String {
    kb.map(|value| format!("{:.1} MiB", value as f64 / 1024.0))
        .unwrap_or_else(|| MISSING.to_string())
}

fn exact_memory(kb: Option<u64>) -> String {
    kb.map(|value| format!("{:.1} MiB · {value} KiB", value as f64 / 1024.0))
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

fn swap(event: &OomEvent) -> String {
    match event.mem.as_ref() {
        Some(mem) => match (mem.swap_total_kb, mem.swap_free_kb) {
            (Some(total), Some(free)) => {
                format!("{} free of {}", memory(Some(free)), memory(Some(total)))
            }
            (Some(total), None) => format!("{} total", memory(Some(total))),
            _ => MISSING.to_string(),
        },
        None => MISSING.to_string(),
    }
}

fn detail_lines(event: &OomEvent, colors: Palette) -> Vec<Line<'static>> {
    let investigation = investigate(event);
    let mut lines = Vec::new();

    if event.victim_was_largest() == Some(false) {
        if let Some(top) = event.top_consumers(1).first() {
            lines.push(Line::styled(
                format!(
                    " ⚠ CULPRIT MISMATCH: Victim was collateral damage! Real culprit: {} (PID {})",
                    top.name, top.pid
                ),
                Style::default()
                    .fg(colors.critical)
                    .add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::from(""));
        }
    }

    lines.push(section("SUMMARY", colors));
    lines.push(detail_row(
        "Victim",
        format!("{} (PID {})", event.victim_name, event.victim_pid),
        colors.text,
        colors,
    ));
    lines.push(detail_row(
        "Impact",
        format!("{} {}", impact(event).marker(), impact(event).label()),
        impact(event).color(colors),
        colors,
    ));
    lines.push(detail_row("Cause", scope_label(event), colors.text, colors));
    lines.push(detail_row("Scope", scope_short(event), colors.text, colors));
    lines.extend(
        investigation
            .summary
            .iter()
            .map(|line| diagnosis_line(line, colors)),
    );
    lines.push(Line::from(""));
    lines.push(section("DIAGNOSIS", colors));
    lines.extend(
        investigation
            .diagnosis
            .iter()
            .map(|line| diagnosis_line(line, colors)),
    );
    lines.push(Line::from(""));
    lines.push(section("MEMORY", colors));
    lines.extend([
        detail_row(
            "RSS",
            exact_memory(event.rss_total_kb()),
            colors.text,
            colors,
        ),
        detail_row(
            "Anonymous RSS",
            exact_memory(event.anon_rss_kb),
            colors.text,
            colors,
        ),
        detail_row(
            "File RSS",
            exact_memory(event.file_rss_kb),
            colors.text,
            colors,
        ),
        detail_row(
            "Shared RSS",
            exact_memory(event.shmem_rss_kb),
            colors.text,
            colors,
        ),
        detail_row(
            "Virtual memory",
            exact_memory(event.total_vm_kb),
            colors.muted,
            colors,
        ),
        detail_row("Swap", swap(event), colors.muted, colors),
    ]);

    if let Some(pct) = event.rss_share_of_ram() {
        let bar_len = 20;
        let filled = ((pct / 100.0) * bar_len as f64).clamp(0.0, bar_len as f64) as usize;
        let bar = format!(
            "[{}{}] {:.1}% of system RAM",
            "█".repeat(filled),
            "░".repeat(bar_len - filled),
            pct
        );
        lines.push(detail_row("RAM Impact", bar, colors.warning, colors));
    }

    lines.push(Line::from(""));
    lines.push(section("SYSTEM", colors));
    lines.extend([
        detail_row(
            "Trigger",
            present(event.trigger_process.clone()),
            colors.text,
            colors,
        ),
        detail_row("Allocation", allocation(event), colors.text, colors),
        detail_row(
            "Constraint",
            present(event.constraint.clone()),
            colors.text,
            colors,
        ),
        detail_row(
            "OOM score",
            option_i32(event.oom_score_adj),
            colors.text,
            colors,
        ),
        detail_row("UID", option_u32(event.uid), colors.text, colors),
        detail_row("Cgroup", present(event.cgroup.clone()), colors.text, colors),
        detail_row(
            "Limit cgroup",
            present(event.limit_cgroup.clone()),
            colors.text,
            colors,
        ),
        detail_row("Workload", workload(event), colors.text, colors),
    ]);
    lines.push(Line::from(""));
    lines.push(section("TIMELINE", colors));
    lines.extend([
        detail_row("Timestamp", when(event), colors.text, colors),
        detail_row("Classification", scope_label(event), colors.text, colors),
        detail_row(
            "Confirmation",
            reaper(event),
            if event.reaped {
                colors.good
            } else {
                colors.warning
            },
            colors,
        ),
    ]);
    lines.push(Line::from(""));
    lines.push(section("TASK SNAPSHOT", colors));
    if event.processes.is_empty() {
        lines.push(Line::styled(
            format!("  {MISSING}"),
            Style::default().fg(colors.muted),
        ));
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

fn section(label: &str, colors: Palette) -> Line<'static> {
    Line::styled(
        format!(" {label}"),
        Style::default()
            .fg(colors.accent)
            .add_modifier(Modifier::BOLD),
    )
}

fn diagnosis_line(value: &str, colors: Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(" • ", Style::default().fg(colors.accent)),
        Span::styled(value.to_string(), Style::default().fg(colors.text)),
    ])
}

fn detail_row(
    label: &str,
    value: impl Into<String>,
    value_color: Color,
    colors: Palette,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {label:<16}"),
            Style::default()
                .fg(colors.muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.into(), Style::default().fg(value_color)),
    ])
}

fn allocation(event: &OomEvent) -> String {
    match (&event.gfp_mask, event.order) {
        (Some(mask), Some(order)) => format!("order {order} · {mask}"),
        (Some(mask), None) => mask.to_string(),
        _ => MISSING.to_string(),
    }
}

fn option_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| MISSING.to_string())
}

fn option_i32(value: Option<i32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| MISSING.to_string())
}

const RAW_TOKENS: &[&str] = &[
    "Memory cgroup out of memory",
    "Out of memory",
    "Killed process",
    "invoked oom-killer",
    "page allocation failure",
    "Mem-Info",
    "Task state",
    "oom_score_adj",
    "constraint",
    "Total swap",
    "Free swap",
    "GFP_",
];

fn highlight_raw_line(line: &str, colors: Palette) -> Line<'static> {
    let mut spans = Vec::new();
    let mut cursor = 0;
    while cursor < line.len() {
        let mut next: Option<(usize, &str)> = None;
        for token in RAW_TOKENS {
            if let Some(relative) = line[cursor..].find(token) {
                let candidate = (cursor + relative, *token);
                if next
                    .map(|current| {
                        candidate.0 < current.0
                            || (candidate.0 == current.0 && candidate.1.len() > current.1.len())
                    })
                    .unwrap_or(true)
                {
                    next = Some(candidate);
                }
            }
        }
        let Some((start, token)) = next else {
            spans.push(Span::raw(line[cursor..].to_string()));
            break;
        };
        if start > cursor {
            spans.push(Span::raw(line[cursor..start].to_string()));
        }
        let style = if matches!(
            token,
            "Memory cgroup out of memory"
                | "Out of memory"
                | "Killed process"
                | "invoked oom-killer"
                | "page allocation failure"
        ) {
            Style::default()
                .fg(colors.critical)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(colors.warning)
                .add_modifier(Modifier::BOLD)
        };
        spans.push(Span::styled(token.to_string(), style));
        cursor = start + token.len();
    }
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    Line::from(spans)
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
    use crate::{
        model::{MemInfo, ProcessEntry},
        source::SourceOptions,
    };
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
            raw_lines: vec!["Out of memory: Killed process 42 (worker) oom_score_adj=0".to_string()],
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

    fn render(width: u16, height: u16, event: OomEvent, help: bool) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut app = App::new(
            vec![event],
            "test log".to_string(),
            SourceOptions::default(),
            None,
        );
        app.help_visible = help;
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
    fn layouts_render_investigation_sections() {
        for (width, height) in [(140, 50), (100, 45), (70, 45)] {
            let output = render(width, height, event(true, true), false);
            assert!(output.contains("SUMMARY"));
            assert!(output.contains("DIAGNOSIS"));
        }
    }

    #[test]
    fn wide_layout_keeps_evidence_and_metadata_visible() {
        let output = render(140, 40, event(true, true), false);
        assert!(output.contains("VIEWING"));
        assert!(output.contains("RAW KERNEL EVIDENCE"));
        assert!(output.contains("Killed process"));
    }

    #[test]
    fn help_overlay_is_available_without_replacing_the_console() {
        let output = render(140, 40, event(false, false), true);
        assert!(output.contains("HELP"));
        assert!(output.contains("quick switch log sources"));
    }

    #[test]
    fn landing_page_renders_welcome_dashboard() {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let mut app = App::new(
            vec![],
            "test log".to_string(),
            SourceOptions::default(),
            None,
        );
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let output: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(output.contains("WELCOME TO OOM-TUI"));
        assert!(output.contains("SYSTEM & HEALTH MONITOR"));
        assert!(output.contains("QUICK ACTIONS"));
    }

    #[test]
    fn footer_displays_contextual_keybinds_per_page() {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();

        // 1. Landing Page footer
        let mut app_landing = App::new(
            vec![],
            "test log".to_string(),
            SourceOptions::default(),
            None,
        );
        terminal
            .draw(|frame| draw(frame, &mut app_landing))
            .unwrap();
        let landing_out: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(landing_out.contains("h:console"));
        assert!(landing_out.contains("1-4:sources"));
        assert!(!landing_out.contains("Tab:incidents"));

        // 2. Incident Console footer
        let mut app_console = App::new(
            vec![event(true, true)],
            "test log".to_string(),
            SourceOptions::default(),
            None,
        );
        terminal
            .draw(|frame| draw(frame, &mut app_console))
            .unwrap();
        let console_out: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(console_out.contains("h:landing"));
        assert!(console_out.contains("Tab:incidents"));
    }

    #[test]
    fn long_investigations_render_a_visible_scrollbar() {
        let output = render(140, 20, event(true, true), false);
        assert!(output.contains('█'));
    }
}
