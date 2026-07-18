//! Non-interactive output: what `oom-tui` prints when it isn't a TUI.
//!
//! The JSON shape here is a deliberate, documented view rather than a
//! `Serialize` derive on `OomEvent`. Scripts depend on this schema, so it must
//! be free to stay stable while the internal representation changes.

use crate::model::OomEvent;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    /// TUI when stdout is a terminal, table when piped.
    Auto,
    /// Force the interactive dashboard.
    Tui,
    /// Human-readable columns.
    Table,
    /// A single pretty-printed JSON array.
    Json,
    /// One compact JSON object per line, for streaming consumers.
    Jsonl,
}

/// Stable public schema. Field names are part of the tool's contract; changing
/// one is a breaking change.
#[derive(Serialize)]
struct EventJson<'a> {
    timestamp: Option<&'a str>,
    /// RFC 3339 wall-clock time, when the log's epoch could be trusted.
    occurred_at: Option<String>,
    victim_name: &'a str,
    victim_pid: u32,
    uid: Option<u32>,
    /// "cgroup" when a container/cgroup limit was hit, "host" for global exhaustion.
    scope: &'static str,
    cgroup: Option<&'a str>,
    /// The cgroup whose limit was breached, when it differs from `cgroup`.
    limit_cgroup: Option<&'a str>,
    /// Workload identity decoded from the cgroup path: runtime, pod, container.
    workload: Option<WorkloadJson>,
    constraint: Option<&'a str>,
    trigger_process: Option<&'a str>,
    gfp_mask: Option<&'a str>,
    alloc_order: Option<u32>,
    oom_score_adj: Option<i32>,
    total_vm_kb: Option<u64>,
    anon_rss_kb: Option<u64>,
    file_rss_kb: Option<u64>,
    shmem_rss_kb: Option<u64>,
    pgtables_kb: Option<u64>,
    /// anon + file + shmem, precomputed so consumers don't have to.
    rss_total_kb: Option<u64>,
    reaped: bool,

    // --- system state at the moment of the kill ---
    total_ram_kb: Option<u64>,
    swap_total_kb: Option<u64>,
    /// The victim's RSS as a percentage of total RAM. Far more meaningful than
    /// the absolute figure, which says nothing without knowing the machine.
    rss_percent_of_ram: Option<f64>,
    /// False means the kernel killed something that was *not* the biggest
    /// memory user, so the process to investigate is in `top_consumers`.
    victim_was_largest: Option<bool>,
    /// Every task the kernel listed, largest resident set first.
    top_consumers: Vec<ProcessJson<'a>>,
}

#[derive(Serialize)]
struct WorkloadJson {
    runtime: &'static str,
    container_id: Option<String>,
    pod_uid: Option<String>,
    qos_class: Option<String>,
    unit: Option<String>,
}

impl From<crate::container::Identity> for WorkloadJson {
    fn from(id: crate::container::Identity) -> Self {
        WorkloadJson {
            runtime: id.runtime.label(),
            container_id: id.container_id,
            pod_uid: id.pod_uid,
            qos_class: id.qos_class,
            unit: id.unit,
        }
    }
}

#[derive(Serialize)]
struct ProcessJson<'a> {
    pid: u32,
    uid: u32,
    name: &'a str,
    rss_kb: u64,
    total_vm_kb: u64,
    oom_score_adj: i32,
}

impl<'a> From<&'a crate::model::ProcessEntry> for ProcessJson<'a> {
    fn from(p: &'a crate::model::ProcessEntry) -> Self {
        ProcessJson {
            pid: p.pid,
            uid: p.uid,
            name: &p.name,
            rss_kb: p.rss_kb,
            total_vm_kb: p.total_vm_kb,
            oom_score_adj: p.oom_score_adj,
        }
    }
}

impl<'a> From<&'a OomEvent> for EventJson<'a> {
    fn from(e: &'a OomEvent) -> Self {
        EventJson {
            timestamp: e.timestamp.as_deref(),
            occurred_at: e.occurred_at.map(|t| t.to_rfc3339()),
            victim_name: &e.victim_name,
            victim_pid: e.victim_pid,
            uid: e.uid,
            scope: if e.memcg_kill { "cgroup" } else { "host" },
            cgroup: e.cgroup.as_deref(),
            limit_cgroup: e.limit_cgroup.as_deref(),
            workload: e
                .cgroup
                .as_deref()
                .and_then(crate::container::identify)
                .map(WorkloadJson::from),
            constraint: e.constraint.as_deref(),
            trigger_process: e.trigger_process.as_deref(),
            gfp_mask: e.gfp_mask.as_deref(),
            alloc_order: e.order,
            oom_score_adj: e.oom_score_adj,
            total_vm_kb: e.total_vm_kb,
            anon_rss_kb: e.anon_rss_kb,
            file_rss_kb: e.file_rss_kb,
            shmem_rss_kb: e.shmem_rss_kb,
            pgtables_kb: e.pgtables_kb,
            rss_total_kb: e.rss_total_kb(),
            reaped: e.reaped,
            total_ram_kb: e.mem.as_ref().and_then(|m| m.total_ram_kb),
            swap_total_kb: e.mem.as_ref().and_then(|m| m.swap_total_kb),
            rss_percent_of_ram: e.rss_share_of_ram().map(|p| (p * 10.0).round() / 10.0),
            victim_was_largest: e.victim_was_largest(),
            top_consumers: e
                .top_consumers(usize::MAX)
                .into_iter()
                .map(ProcessJson::from)
                .collect(),
        }
    }
}

pub fn to_json(events: &[OomEvent]) -> serde_json::Result<String> {
    let view: Vec<EventJson> = events.iter().map(EventJson::from).collect();
    serde_json::to_string_pretty(&view)
}

pub fn to_jsonl(events: &[OomEvent]) -> serde_json::Result<String> {
    let mut out = String::new();
    for event in events {
        out.push_str(&serde_json::to_string(&EventJson::from(event))?);
        out.push('\n');
    }
    Ok(out)
}

/// Fixed-width columns, greppable and awk-friendly.
pub fn to_table(events: &[OomEvent], source: &str) -> String {
    let mut out = format!(
        "# {} OOM-kill event{} from {source}\n",
        events.len(),
        if events.len() == 1 { "" } else { "s" }
    );
    out.push_str(&format!(
        "{:<24} {:<24} {:>9} {:>12}  {:<7} {}\n",
        "TIMESTAMP", "VICTIM", "PID", "RSS", "SCOPE", "CGROUP"
    ));
    for e in events {
        out.push_str(&format!(
            "{:<24} {:<24} {:>9} {:>12}  {:<7} {}\n",
            e.timestamp.as_deref().unwrap_or("-"),
            truncate(&e.victim_name, 24),
            e.victim_pid,
            e.rss_total_kb()
                .map(|kb| format!("{:.1} MiB", kb as f64 / 1024.0))
                .unwrap_or_else(|| "-".to_string()),
            if e.memcg_kill { "cgroup" } else { "host" },
            e.cgroup.as_deref().unwrap_or("-"),
        ));
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<OomEvent> {
        crate::parser::parse_log(
            "Memory cgroup out of memory: Killed process 42 (node) total-vm:100kB, anon-rss:50kB, file-rss:0kB, shmem-rss:0kB, UID:0 pgtables:4kB oom_score_adj:0",
        )
    }

    #[test]
    fn json_exposes_scope_and_precomputed_rss() {
        let json = to_json(&sample()).unwrap();
        assert!(json.contains("\"scope\": \"cgroup\""));
        assert!(json.contains("\"rss_total_kb\": 50"));
    }

    #[test]
    fn jsonl_emits_one_line_per_event() {
        let out = to_jsonl(&sample()).unwrap();
        assert_eq!(out.lines().count(), 1);
        // Must be compact: streaming consumers read line-by-line.
        assert!(!out.trim().contains('\n'));
    }

    #[test]
    fn empty_input_still_produces_valid_json() {
        assert_eq!(to_json(&[]).unwrap(), "[]");
        assert_eq!(to_jsonl(&[]).unwrap(), "");
    }

    #[test]
    fn table_has_a_header_and_one_row_per_event() {
        let table = to_table(&sample(), "test");
        assert!(table.contains("VICTIM"));
        assert!(table.contains("node"));
        assert!(table.contains("cgroup"));
    }
}
