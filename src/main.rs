mod app;
mod container;
mod model;
mod parser;
mod report;
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
use report::OutputFormat;
use source::{BootScope, SourceOptions};
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::time::Duration;

/// oom-tui: OOM-killer forensics for Linux.
///
/// Reads kernel logs (journalctl / dmesg / a file or stdin you point it at),
/// reconstructs each OOM-kill event from the scattered log lines the kernel
/// actually prints, and shows them as a browsable timeline with a clean
/// per-event autopsy view.
#[derive(ClapParser, Debug)]
#[command(name = "oom-tui", version, about)]
struct Cli {
    /// Read logs from this file instead of journalctl/dmesg. Use `-` for stdin.
    #[arg(short, long)]
    file: Option<String>,

    /// Boot offset to inspect: 0 is the current boot, -1 the previous one.
    /// Use this to find the OOM kill that caused a reboot.
    #[arg(short, long, allow_hyphen_values = true, conflicts_with = "all_boots")]
    boot: Option<i32>,

    /// Search every boot the journal still retains.
    #[arg(long)]
    all_boots: bool,

    /// Only events after this time; passed to journalctl (e.g. "2 days ago").
    #[arg(long)]
    since: Option<String>,

    /// Only events before this time; passed to journalctl.
    #[arg(long)]
    until: Option<String>,

    /// Output format. `auto` uses the TUI on a terminal and a table when piped.
    #[arg(long, value_enum, default_value = "auto")]
    format: OutputFormat,

    /// Exit with status 1 when any OOM-kill event was found, for use as a check.
    #[arg(long)]
    exit_code: bool,
}

impl Cli {
    fn source_options(&self) -> SourceOptions {
        SourceOptions {
            file: self.file.clone(),
            boot: if self.all_boots {
                BootScope::All
            } else {
                self.boot.map(BootScope::Offset).unwrap_or_default()
            },
            since: self.since.clone(),
            until: self.until.clone(),
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            // `{error:#}` prints the whole anyhow context chain, which is where
            // the actionable part (permissions, missing file) usually lives.
            eprintln!("oom-tui: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let opts = cli.source_options();

    let source = source::load(&opts)?;
    let events = parser::parse_log(&source.text);

    let found = !events.is_empty();
    let format = resolve_format(cli.format);

    if format == OutputFormat::Tui {
        let app = App::new(events, source.description, opts, source.warning);
        run_tui(app)?;
    } else {
        if let Some(warning) = &source.warning {
            eprintln!("oom-tui: warning: {warning}");
        }
        let rendered = match format {
            OutputFormat::Json => report::to_json(&events)?,
            OutputFormat::Jsonl => report::to_jsonl(&events)?,
            _ => report::to_table(&events, &source.description),
        };
        // Downstream may be `head`, which closes the pipe early; that is normal
        // and must not surface as an error.
        if let Err(error) = io::stdout().write_all(rendered.as_bytes()) {
            if error.kind() != io::ErrorKind::BrokenPipe {
                return Err(error.into());
            }
        }
    }

    Ok(if cli.exit_code && found {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

/// Launching an alternate-screen TUI into a pipe produces escape-code garbage,
/// so `auto` only picks the TUI when stdout is really a terminal.
fn resolve_format(requested: OutputFormat) -> OutputFormat {
    match requested {
        OutputFormat::Auto => {
            if io::stdout().is_terminal() {
                OutputFormat::Tui
            } else {
                OutputFormat::Table
            }
        }
        other => other,
    }
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
                    KeyCode::Char('R') => reload(app),
                    _ => {}
                }
            }
        }
    }
}

/// Re-read from whatever source the app was launched with. A failed reload
/// must never take down the TUI: the events already on screen are still the
/// evidence the user came for, so the error becomes a status message.
fn reload(app: &mut App) {
    let previously_selected = app.selected().map(|e| e.victim_pid);

    match source::load(&app.source_options) {
        Ok(source) => {
            app.events = parser::parse_log(&source.text);
            app.source_description = source.description;
            app.warning = source.warning;
            app.status = Some(format!("reloaded — {} events", app.events.len()));

            // Keep the cursor on the same kill if it survived the reload,
            // otherwise fall back to the newest.
            let index = previously_selected
                .and_then(|pid| app.events.iter().position(|e| e.victim_pid == pid))
                .or_else(|| app.events.len().checked_sub(1));
            app.list_state.select(index);
        }
        Err(error) => {
            app.status = Some(format!("reload failed: {error}"));
        }
    }
}
