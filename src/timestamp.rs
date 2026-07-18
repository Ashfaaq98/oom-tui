//! Turn the log's assorted timestamp spellings into real wall-clock times.
//!
//! Kernel logs carry three different formats, none directly comparable:
//!
//!   `+767.925606s`              seconds since boot (raw dmesg)
//!   `Sat Jul 18 09:03:34 2026`  wall clock (`dmesg -T`)
//!   `Mar 24 18:41:04`           wall clock, no year (syslog / journalctl)
//!
//! Resolving them matters because "was this five minutes ago or three months
//! ago" is the first question anyone asks during an incident.

use crate::model::OomEvent;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDateTime, TimeZone};

/// When this machine booted, from `/proc/stat`'s `btime` field (epoch seconds).
///
/// Only meaningful for logs produced by *this* machine. Resolving an uptime
/// stamp from a log copied off another host against the local boot time would
/// produce a confidently wrong answer, so callers must not supply it there.
pub fn local_boot_time() -> Option<DateTime<Local>> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let btime: i64 = stat
        .lines()
        .find_map(|line| line.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()?;
    Local.timestamp_opt(btime, 0).single()
}

/// Fill in `occurred_at` for every event we can date.
///
/// `boot_time` should be `None` whenever the log did not come from this
/// machine's current boot; uptime-based stamps are then left unresolved rather
/// than being anchored to the wrong epoch.
pub fn resolve_all(events: &mut [OomEvent], boot_time: Option<DateTime<Local>>, now: DateTime<Local>) {
    for event in events.iter_mut() {
        event.occurred_at = event
            .timestamp
            .as_deref()
            .and_then(|raw| resolve(raw, boot_time, now));
    }
}

fn resolve(
    raw: &str,
    boot_time: Option<DateTime<Local>>,
    now: DateTime<Local>,
) -> Option<DateTime<Local>> {
    if let Some(seconds) = raw.strip_prefix('+').and_then(|s| s.strip_suffix('s')) {
        let offset: f64 = seconds.parse().ok()?;
        let boot = boot_time?;
        return Some(boot + Duration::milliseconds((offset * 1000.0) as i64));
    }

    // `dmesg -T` style, which already carries a year.
    for format in ["%a %b %e %H:%M:%S %Y", "%a %b %d %H:%M:%S %Y"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(raw, format) {
            return Local.from_local_datetime(&parsed).single();
        }
    }

    // Syslog style: no year at all, so infer one.
    for format in ["%b %e %H:%M:%S", "%b %d %H:%M:%S"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(
            &format!("{raw} {}", now.year()),
            &format!("{format} %Y"),
        ) {
            let candidate = Local.from_local_datetime(&parsed).single()?;
            // A "future" syslog date really means it is from last year - the
            // classic New Year's Eve log-reading bug.
            return Some(if candidate > now + Duration::days(1) {
                Local
                    .from_local_datetime(&parsed.with_year(now.year() - 1)?)
                    .single()?
            } else {
                candidate
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap()
    }

    #[test]
    fn uptime_is_anchored_to_boot_time() {
        let boot = at(2026, 7, 18, 9, 0);
        let resolved = resolve("+120.5s", Some(boot), at(2026, 7, 18, 10, 0)).unwrap();
        assert_eq!(resolved, boot + Duration::milliseconds(120_500));
    }

    #[test]
    fn uptime_is_left_unresolved_without_a_trustworthy_boot_time() {
        // A log from another machine must not be dated against our own boot.
        assert!(resolve("+120.5s", None, at(2026, 7, 18, 10, 0)).is_none());
    }

    #[test]
    fn dmesg_human_format_is_parsed() {
        let resolved = resolve(
            "Sat Jul 18 09:03:34 2026",
            None,
            at(2026, 7, 18, 10, 0),
        )
        .unwrap();
        assert_eq!(resolved.year(), 2026);
        assert_eq!(resolved.month(), 7);
        assert_eq!(resolved.day(), 18);
    }

    #[test]
    fn syslog_without_a_year_assumes_the_current_one() {
        let resolved = resolve("Mar 24 18:41:04", None, at(2026, 7, 18, 10, 0)).unwrap();
        assert_eq!(resolved.year(), 2026);
        assert_eq!(resolved.month(), 3);
    }

    #[test]
    fn a_future_syslog_date_is_treated_as_last_year() {
        // Reading a December log on New Year's Day.
        let resolved = resolve("Dec 24 18:41:04", None, at(2026, 1, 2, 10, 0)).unwrap();
        assert_eq!(resolved.year(), 2025);
        assert_eq!(resolved.month(), 12);
    }

    #[test]
    fn unrecognised_text_is_not_invented_into_a_date() {
        assert!(resolve("not a timestamp", None, at(2026, 7, 18, 10, 0)).is_none());
    }
}
