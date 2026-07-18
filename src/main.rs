mod app;
mod model;
mod parser;
mod source;
mod ui;

use anyhow::Result;
use app::App;
use clap::Parser as ClapParser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

/// oom-tui: a single-dashboard TUI for OOM-killer forensics.
///
/// Reads kernel logs (journalctl -k / dmesg / a file you point it at),
/// reconstructs each OOM-kill event from the scattered log lines the
/// kernel actually prints, and shows them as a browsable timeline with
/// a clean per-event autopsy view.
#[derive(ClapParser, Debug)]
#[command(name = "oom-tui", version, about)]
struct Cli {
    /// Read logs from this file instead of journalctl/dmesg.
    /// Useful for analyzing a log you copied off another machine.
    #[arg(short, long)]
    file: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let source = source::load(cli.file.as_deref())?;
    let events = parser::parse_log(&source.text);
    let app = App::new(events, source.description);

    run_tui(app)
}

fn run_tui(mut app: App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
        disable_raw_mode()?;
        return Err(error.into());
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                    KeyCode::Char('l') => app.toggle_raw(),
                    KeyCode::Char('R') => reload(app)?,
                    _ => {}
                }
            }
        }
    }
}

fn reload(app: &mut App) -> Result<()> {
    // Re-run the same source description's logic: if it was a file, we'd
    // need the original path; for the common case (journalctl/dmesg) we
    // just re-invoke `source::load(None)`. Keep it simple and safe.
    if app.source_description.starts_with("file:") {
        app.status = Some("file source: no reload".to_string());
        return Ok(());
    }
    let source = source::load(None)?;
    let events = parser::parse_log(&source.text);
    let selected_pid = app.selected().map(|e| e.victim_pid);
    app.events = events;
    app.source_description = source.description;
    app.status = None;
    // try to keep selection on the same pid if it still exists, else jump to newest
    if let Some(pid) = selected_pid {
        if let Some(idx) = app.events.iter().position(|e| e.victim_pid == pid) {
            app.list_state.select(Some(idx));
            return Ok(());
        }
    }
    if !app.events.is_empty() {
        app.list_state.select(Some(app.events.len() - 1));
    }
    Ok(())
}
