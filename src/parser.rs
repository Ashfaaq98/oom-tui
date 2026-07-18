use crate::model::OomEvent;
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
}

fn regexes() -> &'static Regexes {
    static CELL: OnceLock<Regexes> = OnceLock::new();
    CELL.get_or_init(|| Regexes {
        // `[  767.925606] rest...`  (raw dmesg / kernel uptime)
        dmesg_prefix: Regex::new(r"^\[\s*(?P<uptime>[0-9]+\.[0-9]+)\]\s*(?P<rest>.*)$").unwrap(),
        // `[Sat Jul 18 09:03:34 2026] rest...` (`dmesg -T`)
        dmesg_human_prefix: Regex::new(r"^\[(?P<ts>[^\]]+)\]\s*(?P<rest>.*)$").unwrap(),
        // `Mar 24 18:41:04 host kernel: rest...` (syslog / journalctl -o short)
        syslog_prefix: Regex::new(
            r"^(?P<ts>\w{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2})\s+\S+\s+(?:kernel:\s*)?(?P<rest>.*)$",
        )
        .unwrap(),
        trigger: Regex::new(
            r"^(?P<proc>.+?)\s+invoked oom-killer:\s*gfp_mask=(?P<gfp>\S+),\s*order=(?P<order>\d+)",
        )
        .unwrap(),
        constraint: Regex::new(
            r"^oom-kill:constraint=(?P<constraint>\S+?),.*?task_memcg=(?P<memcg>\S+?),task=(?P<task>.+?),pid=(?P<pid>\d+),uid=(?P<uid>\d+)",
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
    let mut pending_raw: Vec<String> = Vec::new();

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
            pending_raw.push(raw_line.to_string());
            continue;
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
                trigger_process,
                gfp_mask,
                order,
                constraint,
                cgroup,
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
                raw_lines: std::mem::take(&mut pending_raw),
            };
            events.push(event);
            continue;
        }

        // Not a line we care about directly, but if we're mid-way through
        // collecting an event's context, keep it as raw context (bounded so
        // unrelated log spam doesn't pile up forever).
        if pending_trigger.is_some() || pending_constraint.is_some() {
            if pending_raw.len() < 200 {
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

    #[test]
    fn ignores_unrelated_log_noise() {
        let text = "Jan  1 00:00:01 host kernel: Linux version 6.1.0\nJan  1 00:00:02 host systemd[1]: Started foo.service\n";
        let events = parse_log(text);
        assert!(events.is_empty());
    }
}
