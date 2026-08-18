//! Evidence-backed investigation text for the interactive console.
//!
//! This module deliberately turns only recorded fields into short conclusions.
//! It never parses raw text again and never fills gaps with guesses.

use crate::model::OomEvent;

/// The concise, first-screen explanation of an OOM incident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Investigation {
    pub summary: Vec<String>,
    pub diagnosis: Vec<String>,
}

pub fn investigate(event: &OomEvent) -> Investigation {
    let scope = if event.memcg_kill {
        "Memory cgroup exhaustion"
    } else {
        "Host-wide memory exhaustion"
    };

    let mut summary = vec![format!(
        "{scope} caused the kernel to terminate {} (PID {}).",
        event.victim_name, event.victim_pid
    )];
    if let Some(rss) = event.rss_total_kb() {
        summary.push(format!("Victim RSS: {}.", memory(rss)));
    } else {
        summary.push("Victim RSS: not reported by kernel.".to_string());
    }
    if let Some(anon) = event.anon_rss_kb {
        if event.file_rss_kb == Some(0) && event.shmem_rss_kb == Some(0) {
            summary.push(format!("Anonymous RSS accounts for {}.", memory(anon)));
        }
    }
    if let Some(mask) = &event.gfp_mask {
        summary.push(match event.order {
            Some(order) => format!("Allocation failed: order {order} using {mask}."),
            None => format!("Allocation mask: {mask}."),
        });
    }
    summary.truncate(5);

    let mut diagnosis = vec![if event.memcg_kill {
        "Memory cgroup limit triggered the OOM killer.".to_string()
    } else {
        "Kernel invoked the host-wide OOM killer.".to_string()
    }];
    diagnosis.push(format!(
        "{} (PID {}) was selected as the victim.",
        event.victim_name, event.victim_pid
    ));
    if let Some(limit) = &event.limit_cgroup {
        diagnosis.push(format!("Reported limit cgroup: {limit}."));
    } else if event.memcg_kill {
        diagnosis.push("Cgroup limit path: not reported by kernel.".to_string());
    } else {
        diagnosis.push("No cgroup limit was reported.".to_string());
    }
    if let Some(trigger) = &event.trigger_process {
        diagnosis.push(format!("Failed allocation was triggered by {trigger}."));
    } else {
        diagnosis.push("Allocation trigger: not reported by kernel.".to_string());
    }
    match event.victim_was_largest() {
        // The victim is often collateral damage; name the real memory hog.
        Some(false) => {
            if let Some(largest) = event.top_consumers(1).first() {
                diagnosis.push(format!(
                    "Largest recorded task: {} (PID {}).",
                    largest.name, largest.pid
                ));
            }
        }
        // Say so explicitly - confirmation that the killed process really was
        // the heaviest is exactly what the reader is checking for.
        Some(true) => {
            diagnosis.push(format!(
                "Victim was the largest of {} tasks examined.",
                event.processes.len()
            ));
        }
        None => {}
    }
    diagnosis.truncate(5);

    Investigation { summary, diagnosis }
}

fn memory(kb: u64) -> String {
    format!("{:.1} MiB", kb as f64 / 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_diagnosis_only_uses_recorded_facts() {
        let event = OomEvent {
            victim_name: "postgres".to_string(),
            victim_pid: 1433,
            anon_rss_kb: Some(320_000),
            file_rss_kb: Some(0),
            shmem_rss_kb: Some(0),
            gfp_mask: Some("GFP_KERNEL".to_string()),
            order: Some(0),
            ..Default::default()
        };
        let report = investigate(&event);
        assert!(report.summary[0].contains("Host-wide"));
        assert!(report
            .summary
            .iter()
            .any(|line| line.contains("Anonymous RSS")));
        assert!(report
            .diagnosis
            .iter()
            .any(|line| line.contains("No cgroup limit")));
        assert!(report
            .diagnosis
            .iter()
            .any(|line| line.contains("not reported")));
        assert!(report.summary.len() <= 5);
        assert!(report.diagnosis.len() <= 5);
    }

    #[test]
    fn confirms_when_the_victim_was_the_largest_task() {
        use crate::model::ProcessEntry;
        let event = OomEvent {
            victim_name: "claude".to_string(),
            victim_pid: 449088,
            processes: vec![
                ProcessEntry {
                    pid: 449088,
                    rss_kb: 14_000_000,
                    name: "claude".to_string(),
                    ..Default::default()
                },
                ProcessEntry {
                    pid: 399347,
                    rss_kb: 700_000,
                    name: "chrome".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let report = investigate(&event);
        assert!(report
            .diagnosis
            .iter()
            .any(|line| line.contains("largest of 2 tasks")));
    }

    #[test]
    fn cgroup_diagnosis_keeps_missing_limit_explicit() {
        let event = OomEvent {
            victim_name: "redis".to_string(),
            victim_pid: 9,
            memcg_kill: true,
            ..Default::default()
        };
        let report = investigate(&event);
        assert!(report.diagnosis[0].contains("Memory cgroup"));
        assert!(report
            .diagnosis
            .iter()
            .any(|line| line.contains("not reported by kernel")));
    }
}
