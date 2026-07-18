use crate::model::{MemInfo, OomEvent, ProcessEntry};
use regex::Regex;
use std::sync::OnceLock;

// The kernel does NOT print one tidy OOM record - it prints several
// independent printk lines in sequence, e.g:
//
//   [ 767.925606] stress invoked oom-killer: gfp_mask=0x..., order=0, oom_score_adj=0
//   [ 767.925620] oom-kill:constraint=CONSTRAINT_NONE,nodemask=(null),cpuset=/,
//                 mems_allowed=0,global_oom,task_memcg=/user.slice/user-1000.slice/session-1.scope,
//                 task=stress,pid=1433,uid=1000
//   [ 767.925620] Out of memory: Killed process 1433 (stress) total-vm:265804kB,
//                 anon-rss:222856kB, file-rss:0kB, shmem-rss:0kB, UID:1000
//                 pgtables:496kB oom_score_adj:0
//   [ 767.973170] oom_reaper: reaped process 1433 (stress), now anon-rss:0kB, ...
//
// We reconstruct a single OomEvent by remembering the most recent
// "invoked oom-killer" / "oom-kill:constraint=" lines and attaching them
// to the "Killed process" line that follows, since that's the only line
// guaranteed to exist for every real kill.

struct Regexes {
    dmesg_prefix: Regex,
    dmesg_human_prefix: Regex,
    syslog_prefix: Regex,
    trigger: Regex,
    constraint: Regex,
    killed: Regex,
    reaped: Regex,
    proc_row: Regex,
    pages_ram: Regex,
    total_swap: Regex,
    free_swap: Regex,
}

/// The kernel reports task-table memory in pages, not kilobytes.
const PAGE_SIZE_KB: u64 = 4;

fn regexes() -> &'static Regexes {
    static CELL: OnceLock<Regexes> = OnceLock::new();
    CELL.get_or_init(|| Regexes {
        // `[  767.925606] rest...`  (raw dmesg / kernel uptime)
        dmesg_prefix: Regex::new(r"^\[\s*(?P<uptime>[0-9]+\.[0-9]+)\]\s*(?P<rest>.*)$").unwrap(),
        // `[Sat Jul 18 09:03:34 2026] rest...` (`dmesg -T`)
        //
        // The bracket contents must contain a letter or colon. Without that,
        // this also matches a process-table row like `[    408]     0   408 ...`
        // and eats the pid, making the whole dump unparseable.
        dmesg_human_prefix: Regex::new(r"^\[(?P<ts>[^\]]*[A-Za-z:][^\]]*)\]\s*(?P<rest>.*)$")
            .unwrap(),
        // `Mar 24 18:41:04 host kernel: rest...` (syslog / journalctl -o short)
        syslog_prefix: Regex::new(
            r"^(?P<ts>\w{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2})\s+\S+\s+(?:kernel:\s*)?(?P<rest>.*)$",
        )
        .unwrap(),
        trigger: Regex::new(
            r"^(?P<proc>.+?)\s+invoked oom-killer:\s*gfp_mask=(?P<gfp>\S+),\s*order=(?P<order>\d+)",
        )
        .unwrap(),
        // `oom_memcg` is the cgroup whose *limit* was breached; `task_memcg` is
        // merely where the victim lived. They differ when a parent slice's limit
        // kills a child, which is exactly when the distinction matters.
        constraint: Regex::new(
            r"^oom-kill:constraint=(?P<constraint>\S+?),.*?(?:oom_memcg=(?P<oom_memcg>[^,]+),)?task_memcg=(?P<memcg>[^,]+),task=(?P<task>.+?),pid=(?P<pid>\d+),uid=(?P<uid>\d+)",
        )
        .unwrap(),
        // The kernel picks this message prefix per code path, so anchoring on a
        // bare "Out of memory:" silently drops the two most interesting cases:
        //
        //   Out of memory: Killed process ...                            (global)
        //   Memory cgroup out of memory: Killed process ...              (memcg / container)
        //   Out of memory (oom_kill_allocating_task): Killed process ...
        //
        // `msg` is captured rather than skipped because "was this the container's
        // limit or the whole host?" is the first question worth answering.
        // `pgtables:` is absent on kernels older than ~4.19, so it stays optional.
        killed: Regex::new(
            r"^(?P<msg>Memory cgroup out of memory|Out of memory(?:\s*\([^)]*\))?):\s*Killed process\s+(?P<pid>\d+)\s*\((?P<name>[^)]+)\)(?:,\s*UID\s*(?P<uid1>\d+))?[,\s]*total-vm:(?P<vm>\d+)kB,\s*anon-rss:(?P<arss>\d+)kB,\s*file-rss:(?P<frss>\d+)kB,\s*shmem-rss:(?P<srss>\d+)kB(?:,\s*UID:(?P<uid2>\d+))?(?:,?\s*pgtables:(?P<pgt>\d+)kB)?\s*oom_score_adj:(?P<adj>-?\d+)",
        )
        .unwrap(),
        reaped: Regex::new(r"^oom_reaper:\s*reaped process\s+(?P<pid>\d+)\s*\((?P<name>[^)]+)\)")
            .unwrap(),
        // A row of the task dump. The column count varies by kernel version
        // (`nr_ptes`/`nr_pmds` on older kernels, `pgtables_bytes` on newer),
        // so the fields are read positionally from both ends instead of
        // pinning a fixed layout - see `parse_process_row`.
        proc_row: Regex::new(r"^\[\s*(?P<pid>\d+)\s*\]\s+(?P<rest>\S.*)$").unwrap(),
        pages_ram: Regex::new(r"^(?P<pages>\d+)\s+pages RAM").unwrap(),
        total_swap: Regex::new(r"^Total swap\s*=\s*(?P<kb>\d+)\s*kB").unwrap(),
        free_swap: Regex::new(r"^Free swap\s*=\s*(?P<kb>-?\d+)\s*kB").unwrap(),
    })
}

/// Strip a dmesg/syslog prefix off a line, returning (timestamp, rest_of_line).
/// If no known prefix matches, the whole line is returned as `rest` with no timestamp.
fn strip_prefix(line: &str) -> (Option<String>, &str) {
    let re = regexes();
    if let Some(caps) = re.dmesg_prefix.captures(line) {
        let uptime = caps.name("uptime").unwrap().as_str().to_string();
        let rest = caps.name("rest").unwrap();
        return (Some(format!("+{uptime}s")), &line[rest.start()..rest.end()]);
    }
    if let Some(caps) = re.dmesg_human_prefix.captures(line) {
        let ts = caps.name("ts").unwrap().as_str().to_string();
        let rest = caps.name("rest").unwrap();
        return (Some(ts), &line[rest.start()..rest.end()]);
    }
    if let Some(caps) = re.syslog_prefix.captures(line) {
        let ts = caps.name("ts").unwrap().as_str().to_string();
        let rest = caps.name("rest").unwrap();
        return (Some(ts), &line[rest.start()..rest.end()]);
    }
    (None, line)
}

/// Read one task-dump row.
///
/// The kernel has shipped at least two column layouts:
///
///   [ pid ] uid tgid total_vm rss pgtables_bytes swapents oom_score_adj name
///   [ pid ] uid tgid total_vm rss nr_ptes nr_pmds   swapents oom_score_adj name
///
/// Rather than guess which kernel produced the log, read the stable fields
/// from the front and the stable fields from the back, and ignore whatever
/// varies in the middle. That way a third layout doesn't break parsing.
fn parse_process_row(pid: u32, rest: &str) -> Option<ProcessEntry> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    // Front claims 0..=3 and back claims the final three; fewer than 7 columns
    // means they would overlap and we'd invent values.
    if tokens.len() < 7 {
        return None;
    }
    let last = tokens.len() - 1;

    Some(ProcessEntry {
        pid,
        uid: tokens[0].parse().ok()?,
        tgid: tokens[1].parse().ok()?,
        total_vm_kb: tokens[2].parse::<u64>().ok()?.saturating_mul(PAGE_SIZE_KB),
        rss_kb: tokens[3].parse::<u64>().ok()?.saturating_mul(PAGE_SIZE_KB),
        swapents: tokens[last - 2].parse().ok()?,
        oom_score_adj: tokens[last - 1].parse().ok()?,
        name: tokens[last].to_string(),
    })
}

/// Parse a full log (many lines, only some of which are OOM-related) into
/// a chronological list of reconstructed OomEvents.
pub fn parse_log(text: &str) -> Vec<OomEvent> {
    let re = regexes();
    let lines: Vec<&str> = text.lines().collect();

    let mut events: Vec<OomEvent> = Vec::new();

    // "pending" state carried forward from trigger/constraint lines until
    // the next "Killed process" line consumes it.
    let mut pending_trigger: Option<(String, String, u32)> = None; // proc, gfp, order
    let mut pending_constraint: Option<(String, String)> = None; // constraint, cgroup
    let mut pending_limit_cgroup: Option<String> = None; // oom_memcg, when present
    let mut pending_raw: Vec<String> = Vec::new();
    let mut pending_processes: Vec<ProcessEntry> = Vec::new();
    let mut pending_mem = MemInfo::default();

    for (i, raw_line) in lines.iter().enumerate() {
        let (ts, body) = strip_prefix(raw_line);
        let body = body.trim();
        if body.is_empty() {
            continue;
        }

        if let Some(caps) = re.trigger.captures(body) {
            pending_trigger = Some((
                caps.name("proc").unwrap().as_str().trim().to_string(),
                caps.name("gfp").unwrap().as_str().to_string(),
                caps.name("order").unwrap().as_str().parse().unwrap_or(0),
            ));
            pending_raw.push(raw_line.to_string());
            continue;
        }

        if let Some(caps) = re.constraint.captures(body) {
            pending_constraint = Some((
                caps.name("constraint").unwrap().as_str().to_string(),
                caps.name("memcg").unwrap().as_str().to_string(),
            ));
            pending_limit_cgroup = caps.name("oom_memcg").map(|m| m.as_str().to_string());
            pending_raw.push(raw_line.to_string());
            continue;
        }

        // The task dump and Mem-Info block sit between the trigger line and
        // the kill line, so they accumulate the same way the trigger does.
        if let Some(caps) = re.proc_row.captures(body) {
            if let Ok(pid) = caps.name("pid").unwrap().as_str().parse::<u32>() {
                if let Some(entry) = parse_process_row(pid, caps.name("rest").unwrap().as_str()) {
                    pending_processes.push(entry);
                    pending_raw.push(raw_line.to_string());
                    continue;
                }
            }
        }

        if let Some(caps) = re.pages_ram.captures(body) {
            pending_mem.total_ram_kb = caps
                .name("pages")
                .and_then(|m| m.as_str().parse::<u64>().ok())
                .map(|pages| pages.saturating_mul(PAGE_SIZE_KB));
        } else if let Some(caps) = re.total_swap.captures(body) {
            pending_mem.swap_total_kb = caps.name("kb").and_then(|m| m.as_str().parse().ok());
        } else if let Some(caps) = re.free_swap.captures(body) {
            pending_mem.swap_free_kb = caps.name("kb").and_then(|m| m.as_str().parse().ok());
        }

        if let Some(caps) = re.killed.captures(body) {
            let pid: u32 = caps.name("pid").unwrap().as_str().parse().unwrap_or(0);
            let name = caps.name("name").unwrap().as_str().to_string();
            let uid = caps
                .name("uid1")
                .or_else(|| caps.name("uid2"))
                .and_then(|m| m.as_str().parse().ok());

            pending_raw.push(raw_line.to_string());

            // Look ahead a handful of lines for a matching reaper confirmation.
            let mut reaped = false;
            for look in lines.iter().skip(i + 1).take(20) {
                let (_, lbody) = strip_prefix(look);
                if let Some(rc) = re.reaped.captures(lbody.trim()) {
                    if rc.name("pid").unwrap().as_str() == pid.to_string() {
                        reaped = true;
                        pending_raw.push(look.to_string());
                        break;
                    }
                }
                // Stop scanning ahead if we hit the next unrelated OOM trigger.
                if re.trigger.captures(lbody.trim()).is_some() {
                    break;
                }
            }

            let (trigger_process, gfp_mask, order) = match pending_trigger.take() {
                Some((p, g, o)) => (Some(p), Some(g), Some(o)),
                None => (None, None, None),
            };
            let (constraint, cgroup) = match pending_constraint.take() {
                Some((c, m)) => (Some(c), Some(m)),
                None => (None, None),
            };

            // Which kernel code path did the killing. The message prefix is
            // authoritative and is present on every kill line; CONSTRAINT_MEMCG
            // corroborates it for logs where only the constraint line survived.
            let memcg_kill = caps
                .name("msg")
                .is_some_and(|m| m.as_str().starts_with("Memory cgroup"))
                || constraint.as_deref() == Some("CONSTRAINT_MEMCG");

            let event = OomEvent {
                timestamp: ts,
                // Filled in later by `timestamp::resolve_all`, which knows
                // whether this log's boot epoch can be trusted.
                occurred_at: None,
                trigger_process,
                gfp_mask,
                order,
                constraint,
                cgroup,
                limit_cgroup: pending_limit_cgroup.take(),
                memcg_kill,
                victim_name: name,
                victim_pid: pid,
                uid,
                total_vm_kb: caps.name("vm").and_then(|m| m.as_str().parse().ok()),
                anon_rss_kb: caps.name("arss").and_then(|m| m.as_str().parse().ok()),
                file_rss_kb: caps.name("frss").and_then(|m| m.as_str().parse().ok()),
                shmem_rss_kb: caps.name("srss").and_then(|m| m.as_str().parse().ok()),
                pgtables_kb: caps.name("pgt").and_then(|m| m.as_str().parse().ok()),
                oom_score_adj: caps.name("adj").and_then(|m| m.as_str().parse().ok()),
                reaped,
                processes: std::mem::take(&mut pending_processes),
                mem: (pending_mem != MemInfo::default()).then(|| std::mem::take(&mut pending_mem)),
                raw_lines: std::mem::take(&mut pending_raw),
            };
            events.push(event);
            continue;
        }

        // Not a line we care about directly, but if we're mid-way through
        // collecting an event's context, keep it as raw context (bounded so
        // unrelated log spam doesn't pile up forever).
        // A full task dump on a busy host runs to several hundred lines, so the
        // bound has to clear that comfortably or "show me the raw log" silently
        // loses the end of the event. It exists only to stop unbounded growth
        // when a log has no kill line to close the event.
        if pending_trigger.is_some() || pending_constraint.is_some() {
            if pending_raw.len() < 5000 {
                pending_raw.push(raw_line.to_string());
            }
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[ 767.925606] stress invoked oom-killer: gfp_mask=0x100cca(GFP_HIGHUSER_MOVABLE), order=0, oom_score_adj=0
[ 767.925620] oom-kill:constraint=CONSTRAINT_NONE,nodemask=(null),cpuset=/,mems_allowed=0,global_oom,task_memcg=/user.slice/user-1000.slice/session-1.scope,task=stress,pid=1433,uid=1000
[ 767.925620] Out of memory: Killed process 1433 (stress) total-vm:265804kB, anon-rss:222856kB, file-rss:0kB, shmem-rss:0kB, UID:1000 pgtables:496kB oom_score_adj:0
[ 767.973170] oom_reaper: reaped process 1433 (stress), now anon-rss:0kB, file-rss:0kB, shmem-rss:0kB
";

    const SYSLOG_SAMPLE: &str = "Mar 24 18:41:04 PLEDXDBOR0G kernel: Out of memory: Killed process 2475067 (postgres) total-vm:2484556kB, anon-rss:143224kB, file-rss:0kB, shmem-rss:452kB, UID:1011 pgtables:588kB oom_score_adj:900";

    const DMESG_HUMAN_SAMPLE: &str = "[Sat Jul 18 09:03:34 2026] Out of memory: Killed process 99 (worker) total-vm:1024kB, anon-rss:512kB, file-rss:0kB, shmem-rss:0kB, UID:1000 pgtables:16kB oom_score_adj:0";

    const MEMCG_SAMPLE: &str = "Memory cgroup out of memory: Killed process 42 (stress-ng-vm) total-vm:524288kB, anon-rss:262144kB, file-rss:0kB, shmem-rss:0kB, UID:1000 pgtables:512kB oom_score_adj:0";

    #[test]
    fn parses_full_dmesg_sequence() {
        let events = parse_log(SAMPLE);
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.victim_name, "stress");
        assert_eq!(e.victim_pid, 1433);
        assert_eq!(e.trigger_process.as_deref(), Some("stress"));
        assert_eq!(e.constraint.as_deref(), Some("CONSTRAINT_NONE"));
        assert_eq!(
            e.cgroup.as_deref(),
            Some("/user.slice/user-1000.slice/session-1.scope")
        );
        assert_eq!(e.anon_rss_kb, Some(222856));
        assert_eq!(e.uid, Some(1000));
        assert!(e.reaped);
        assert_eq!(e.rss_total_kb(), Some(222856));
    }

    #[test]
    fn parses_bare_syslog_line_with_no_trigger_context() {
        let events = parse_log(SYSLOG_SAMPLE);
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.victim_name, "postgres");
        assert_eq!(e.victim_pid, 2475067);
        assert_eq!(e.uid, Some(1011));
        assert_eq!(e.oom_score_adj, Some(900));
        assert!(e.trigger_process.is_none());
        assert!(!e.reaped);
    }

    #[test]
    fn parses_human_readable_dmesg_timestamp() {
        let events = parse_log(DMESG_HUMAN_SAMPLE);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].timestamp.as_deref(), Some("Sat Jul 18 09:03:34 2026"));
        assert_eq!(events[0].victim_name, "worker");
    }

    /// `Out of memory (oom_kill_allocating_task):` - emitted when the sysctl
    /// of the same name is set, so the allocating task is killed directly.
    const ALLOCATING_TASK_SAMPLE: &str = "[ 900.100000] Out of memory (oom_kill_allocating_task): Killed process 77 (java) total-vm:2000kB, anon-rss:1000kB, file-rss:0kB, shmem-rss:0kB, UID:0 pgtables:16kB oom_score_adj:0";

    /// Kernels older than ~4.19 don't print the `pgtables:` field at all.
    const NO_PGTABLES_SAMPLE: &str = "Mar 24 18:41:04 host kernel: Out of memory: Killed process 1234 (redis-server) total-vm:100000kB, anon-rss:90000kB, file-rss:0kB, shmem-rss:0kB, UID:999 oom_score_adj:0";

    /// A container kill as it actually appears on a Kubernetes node.
    const K8S_MEMCG_SAMPLE: &str = "\
[ 512.100000] node invoked oom-killer: gfp_mask=0xcc0(GFP_KERNEL), order=0, oom_score_adj=968
[ 512.100100] oom-kill:constraint=CONSTRAINT_MEMCG,nodemask=(null),cpuset=cri-containerd-abc123.scope,mems_allowed=0,oom_memcg=/kubepods.slice/kubepods-burstable.slice/kubepods-burstable-pod9f2c.slice,task_memcg=/kubepods.slice/kubepods-burstable.slice/kubepods-burstable-pod9f2c.slice/cri-containerd-abc123.scope,task=node,pid=4242,uid=0
[ 512.100200] Memory cgroup out of memory: Killed process 4242 (node) total-vm:1265804kB, anon-rss:1022856kB, file-rss:4096kB, shmem-rss:0kB, UID:0 pgtables:2496kB oom_score_adj:968
";

    #[test]
    fn parses_memory_cgroup_kill() {
        let events = parse_log(MEMCG_SAMPLE);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].victim_pid, 42);
        assert_eq!(events[0].victim_name, "stress-ng-vm");
        // The whole point: this was a container limit, not host exhaustion.
        assert!(events[0].memcg_kill);
    }

    #[test]
    fn parses_oom_kill_allocating_task_variant() {
        let events = parse_log(ALLOCATING_TASK_SAMPLE);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].victim_name, "java");
        assert_eq!(events[0].victim_pid, 77);
        assert!(!events[0].memcg_kill);
    }

    #[test]
    fn parses_old_kernel_line_without_pgtables() {
        let events = parse_log(NO_PGTABLES_SAMPLE);
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.victim_name, "redis-server");
        assert_eq!(e.anon_rss_kb, Some(90000));
        assert_eq!(e.pgtables_kb, None);
        assert_eq!(e.oom_score_adj, Some(0));
    }

    #[test]
    fn parses_kubernetes_container_kill_with_full_context() {
        let events = parse_log(K8S_MEMCG_SAMPLE);
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.victim_name, "node");
        assert_eq!(e.victim_pid, 4242);
        assert!(e.memcg_kill);
        assert_eq!(e.constraint.as_deref(), Some("CONSTRAINT_MEMCG"));
        assert_eq!(e.oom_score_adj, Some(968));
        assert_eq!(e.rss_total_kb(), Some(1022856 + 4096));
        assert!(e
            .cgroup
            .as_deref()
            .is_some_and(|c| c.contains("cri-containerd-abc123.scope")));
    }

    #[test]
    fn global_kill_is_not_flagged_as_cgroup_kill() {
        let events = parse_log(SAMPLE);
        assert!(!events[0].memcg_kill);
    }

    /// A complete report as the kernel actually emits it: trigger, Mem-Info,
    /// the task dump, then the kill. Note that `leaky-worker` holds far more
    /// memory than the process that actually got killed.
    const FULL_REPORT: &str = "\
[ 512.000000] postgres invoked oom-killer: gfp_mask=0xcc0(GFP_KERNEL), order=0, oom_score_adj=0
[ 512.000100] Mem-Info:
[ 512.000200] active_anon:480000 inactive_anon:12000 isolated_anon:0
[ 512.000300] Total swap = 0kB
[ 512.000310] Free swap  = 0kB
[ 512.000400] 524288 pages RAM
[ 512.000500] 21012 pages reserved
[ 512.000600] Tasks state (memory values in pages):
[ 512.000700] [  pid  ]   uid  tgid total_vm      rss pgtables_bytes swapents oom_score_adj name
[ 512.000800] [    408]     0   408    16853      512       131072        0             0 systemd-journal
[ 512.000900] [   1200]  1000  1200   400000   300000      2097152        0             0 leaky-worker
[ 512.001000] [   1433]   999  1433   100000    80000      1048576        0             0 postgres
[ 512.001100] Out of memory: Killed process 1433 (postgres) total-vm:400000kB, anon-rss:320000kB, file-rss:0kB, shmem-rss:0kB, UID:999 pgtables:1024kB oom_score_adj:0
";

    #[test]
    fn parses_the_process_table_dump() {
        let events = parse_log(FULL_REPORT);
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.processes.len(), 3);

        let worker = e.processes.iter().find(|p| p.name == "leaky-worker").unwrap();
        assert_eq!(worker.pid, 1200);
        assert_eq!(worker.uid, 1000);
        // The kernel prints pages; we report kB.
        assert_eq!(worker.rss_kb, 300000 * 4);
        assert_eq!(worker.total_vm_kb, 400000 * 4);
    }

    #[test]
    fn identifies_when_the_victim_was_not_the_real_culprit() {
        let events = parse_log(FULL_REPORT);
        let e = &events[0];
        // postgres died, but leaky-worker was holding more. This is the whole
        // reason for parsing the task dump.
        assert_eq!(e.victim_name, "postgres");
        assert_eq!(e.victim_was_largest(), Some(false));
        assert_eq!(e.top_consumers(1)[0].name, "leaky-worker");
    }

    #[test]
    fn parses_mem_info_and_derives_share_of_ram() {
        let events = parse_log(FULL_REPORT);
        let mem = events[0].mem.as_ref().unwrap();
        assert_eq!(mem.total_ram_kb, Some(524288 * 4)); // 2 GiB
        assert_eq!(mem.swap_total_kb, Some(0));

        // 320000 kB of a 2 GiB box is a bit over 15%.
        let share = events[0].rss_share_of_ram().unwrap();
        assert!((share - 15.2).abs() < 0.5, "unexpected share: {share}");
    }

    #[test]
    fn handles_the_older_nr_ptes_column_layout() {
        // Pre-4.19 kernels print nr_ptes and nr_pmds instead of pgtables_bytes,
        // giving one extra column. Reading from both ends must absorb that.
        let text = "[ 100.0] oom-kill:constraint=CONSTRAINT_NONE,nodemask=(null),task_memcg=/,task=x,pid=1,uid=0
[ 100.1] [   1234]     0  1234    50000    40000     123     456        0           -100 dockerd
[ 100.2] Out of memory: Killed process 1234 (dockerd) total-vm:200000kB, anon-rss:160000kB, file-rss:0kB, shmem-rss:0kB, UID:0 oom_score_adj:-100";
        let events = parse_log(text);
        assert_eq!(events.len(), 1);
        let p = &events[0].processes[0];
        assert_eq!(p.name, "dockerd");
        assert_eq!(p.rss_kb, 40000 * 4);
        assert_eq!(p.oom_score_adj, -100);
    }

    #[test]
    fn table_header_row_is_not_mistaken_for_a_process() {
        let events = parse_log(FULL_REPORT);
        assert!(events[0].processes.iter().all(|p| p.name != "name"));
    }

    #[test]
    fn truncated_event_without_a_kill_line_yields_nothing_and_does_not_hang() {
        // The dmesg ring buffer wraps mid-event constantly.
        let truncated = "[ 1.0] stress invoked oom-killer: gfp_mask=0x0, order=0, oom_score_adj=0
[ 1.1] [   1234]     0  1234    50000    40000     123     456        0            0 stress";
        assert!(parse_log(truncated).is_empty());
    }

    #[test]
    fn ignores_unrelated_log_noise() {
        let text = "Jan  1 00:00:01 host kernel: Linux version 6.1.0\nJan  1 00:00:02 host systemd[1]: Started foo.service\n";
        let events = parse_log(text);
        assert!(events.is_empty());
    }
}
