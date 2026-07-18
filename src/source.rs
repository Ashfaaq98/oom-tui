use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::Command;

/// Which slice of history to ask the log backend for.
///
/// This matters more than it looks. `journalctl -k` implies `-b`, i.e. the
/// current boot only - so a kill that *caused* a reboot is invisible by
/// default, which is exactly the incident people most want to investigate.
/// `-k` is really just `_TRANSPORT=kernel` plus that implicit `-b`, so asking
/// for the transport directly is what unlocks previous boots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum BootScope {
    /// Current boot only (the `journalctl -k` default).
    #[default]
    Current,
    /// A specific boot offset: 0 is current, -1 the previous one, and so on.
    Offset(i32),
    /// Every boot the journal still retains.
    All,
}

/// How the caller wants logs located.
#[derive(Debug, Clone, Default)]
pub struct SourceOptions {
    /// Explicit file path, or `-` for stdin.
    pub file: Option<String>,
    pub boot: BootScope,
    /// Passed through to `journalctl --since` (e.g. "2 days ago", "2026-07-01").
    pub since: Option<String>,
    /// Passed through to `journalctl --until`.
    pub until: Option<String>,
}

impl SourceOptions {
    /// True when the caller asked for something only journalctl can answer.
    /// Used to explain why a fallback source may be missing events.
    fn needs_journal(&self) -> bool {
        self.boot != BootScope::Current || self.since.is_some() || self.until.is_some()
    }
}

/// Where the raw log text ultimately came from - shown in the UI so the
/// user knows what they're looking at (and why it might be empty/stale).
#[derive(Debug, Clone)]
pub struct LogSource {
    pub description: String,
    pub text: String,
    /// True when this text came from the running machine's current boot, which
    /// is the only case where kernel uptime stamps can be anchored to wall time.
    pub is_live_local: bool,
    /// Set when we had to fall back to a source that cannot honour the
    /// requested boot/time range, so the UI can say so rather than quietly
    /// showing the wrong window of history.
    pub warning: Option<String>,
}

/// Try, in order: an explicit file (or stdin), `journalctl`, `dmesg -T`,
/// `dmesg`, and finally `/var/log/{syslog,messages}`. Returns the first one
/// that produces readable output.
pub fn load(opts: &SourceOptions) -> Result<LogSource> {
    if let Some(path) = &opts.file {
        return load_file(path);
    }

    if let Some(text) = run_ok("journalctl", &journalctl_args(opts)) {
        return Ok(LogSource {
            description: describe_journal(opts),
            text,
            is_live_local: opts.boot == BootScope::Current,
            warning: None,
        });
    }

    // Everything below reads whatever the backend happens to hold; none of it
    // can honour --boot/--since, so say so instead of implying we did.
    let warning = if opts.needs_journal() {
        Some("journalctl unavailable - boot/time filters were ignored by this source".to_string())
    } else {
        None
    };

    for (cmd, args, desc) in [
        ("dmesg", &["-T"][..], "dmesg -T"),
        ("dmesg", &[][..], "dmesg"),
    ] {
        if let Some(text) = run_ok(cmd, args) {
            return Ok(LogSource {
                description: desc.to_string(),
                text,
                is_live_local: true,
                warning,
            });
        }
    }

    for candidate in ["/var/log/syslog", "/var/log/messages"] {
        if Path::new(candidate).exists() {
            if let Ok(text) = std::fs::read_to_string(candidate) {
                return Ok(LogSource {
                    description: format!("file: {candidate}"),
                    text,
                    is_live_local: false,
                    warning,
                });
            }
        }
    }

    anyhow::bail!(
        "couldn't read logs from journalctl, dmesg, or /var/log/{{syslog,messages}}.\n\
         Try running with --file <path>, piping a log in with --file -, or as a user\n\
         with permission to read kernel logs (member of the 'systemd-journal' group,\n\
         or root for dmesg)."
    )
}

fn load_file(path: &str) -> Result<LogSource> {
    if path == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .context("failed to read log from stdin")?;
        return Ok(LogSource {
            description: "stdin".to_string(),
            text,
            is_live_local: false,
            warning: None,
        });
    }

    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read log file '{path}'"))?;
    Ok(LogSource {
        description: format!("file: {path}"),
        text,
        is_live_local: false,
        warning: None,
    })
}

/// Build the journalctl invocation for the requested scope.
///
/// `-o short` is deliberate: it emits the traditional syslog-style timestamp
/// the parser understands. `short-iso` would need separate handling.
fn journalctl_args(opts: &SourceOptions) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    match opts.boot {
        // `-k` is the well-trodden path for the common case.
        BootScope::Current => args.push("-k".into()),
        BootScope::Offset(n) => {
            args.push("-k".into());
            args.push("-b".into());
            args.push(n.to_string());
        }
        // `-k` would re-impose the current boot, so match the transport directly.
        BootScope::All => args.push("_TRANSPORT=kernel".into()),
    }

    if let Some(since) = &opts.since {
        args.push("--since".into());
        args.push(since.clone());
    }
    if let Some(until) = &opts.until {
        args.push("--until".into());
        args.push(until.clone());
    }

    args.extend(["-o".into(), "short".into(), "--no-pager".into()]);
    args
}

fn describe_journal(opts: &SourceOptions) -> String {
    let scope = match opts.boot {
        BootScope::Current => "current boot".to_string(),
        BootScope::Offset(n) => format!("boot {n}"),
        BootScope::All => "all boots".to_string(),
    };
    let mut desc = format!("journalctl ({scope})");
    if let Some(since) = &opts.since {
        desc.push_str(&format!(" since {since}"));
    }
    if let Some(until) = &opts.until {
        desc.push_str(&format!(" until {until}"));
    }
    desc
}

fn run_ok<S: AsRef<std::ffi::OsStr>>(cmd: &str, args: &[S]) -> Option<String> {
    let output = Command::new(cmd).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_boot_uses_the_dmesg_shorthand() {
        let args = journalctl_args(&SourceOptions::default());
        assert!(args.contains(&"-k".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("_TRANSPORT")));
    }

    #[test]
    fn all_boots_avoids_dash_k_because_it_implies_current_boot() {
        let opts = SourceOptions {
            boot: BootScope::All,
            ..Default::default()
        };
        let args = journalctl_args(&opts);
        assert!(args.contains(&"_TRANSPORT=kernel".to_string()));
        // -k would silently re-restrict us to the current boot.
        assert!(!args.contains(&"-k".to_string()));
    }

    #[test]
    fn boot_offset_is_passed_through() {
        let opts = SourceOptions {
            boot: BootScope::Offset(-1),
            ..Default::default()
        };
        let args = journalctl_args(&opts);
        assert!(args.contains(&"-b".to_string()));
        assert!(args.contains(&"-1".to_string()));
    }

    #[test]
    fn time_range_is_passed_through() {
        let opts = SourceOptions {
            since: Some("2 days ago".into()),
            until: Some("1 hour ago".into()),
            ..Default::default()
        };
        let args = journalctl_args(&opts);
        assert!(args.windows(2).any(|w| w == ["--since", "2 days ago"]));
        assert!(args.windows(2).any(|w| w == ["--until", "1 hour ago"]));
    }

    #[test]
    fn fallback_sources_are_flagged_when_filters_were_requested() {
        assert!(SourceOptions {
            boot: BootScope::All,
            ..Default::default()
        }
        .needs_journal());
        assert!(!SourceOptions::default().needs_journal());
    }
}
