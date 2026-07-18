use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Where the raw log text ultimately came from - shown in the UI so the
/// user knows what they're looking at (and why it might be empty/stale).
#[derive(Debug, Clone)]
pub struct LogSource {
    pub description: String,
    pub text: String,
}

/// Try, in order: an explicit file path, `journalctl -k`, `dmesg -T`,
/// `dmesg`, and finally `/var/log/syslog`. Returns the first one that
/// produces readable output.
pub fn load(explicit_file: Option<&str>) -> Result<LogSource> {
    if let Some(path) = explicit_file {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read log file '{path}'"))?;
        return Ok(LogSource {
            description: format!("file: {path}"),
            text,
        });
    }

    if let Some(text) = run_ok("journalctl", &["-k", "-o", "short-iso", "--no-pager"]) {
        return Ok(LogSource {
            description: "journalctl -k".to_string(),
            text,
        });
    }

    if let Some(text) = run_ok("dmesg", &["-T"]) {
        return Ok(LogSource {
            description: "dmesg -T".to_string(),
            text,
        });
    }

    if let Some(text) = run_ok("dmesg", &[]) {
        return Ok(LogSource {
            description: "dmesg".to_string(),
            text,
        });
    }

    for candidate in ["/var/log/syslog", "/var/log/messages"] {
        if Path::new(candidate).exists() {
            if let Ok(text) = std::fs::read_to_string(candidate) {
                return Ok(LogSource {
                    description: format!("file: {candidate}"),
                    text,
                });
            }
        }
    }

    anyhow::bail!(
        "couldn't read logs from journalctl, dmesg, or /var/log/{{syslog,messages}}.\n\
         Try running with --file <path>, or as a user with permission to read kernel logs\n\
         (member of the 'systemd-journal' group, or root for dmesg)."
    )
}

fn run_ok(cmd: &str, args: &[&str]) -> Option<String> {
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
