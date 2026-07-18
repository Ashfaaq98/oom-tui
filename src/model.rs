/// One row of the kernel's process table dump.
///
/// When the OOM killer fires it prints *every* eligible task with its memory
/// footprint, which is the only record of what the machine actually looked
/// like at that instant. It matters because the OOM killer picks the largest
/// RSS, which is frequently not the process that actually leaked - the real
/// culprit is often sitting a few rows down this table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub uid: u32,
    pub tgid: u32,
    /// Virtual size, converted from the pages the kernel prints.
    pub total_vm_kb: u64,
    /// Resident size, converted from the pages the kernel prints.
    pub rss_kb: u64,
    pub swapents: u64,
    pub oom_score_adj: i32,
    pub name: String,
}

/// The `Mem-Info` block: what the machine as a whole had left.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemInfo {
    pub total_ram_kb: Option<u64>,
    pub swap_total_kb: Option<u64>,
    pub swap_free_kb: Option<u64>,
}

/// A single OOM-kill event reconstructed from kernel log lines.
///
/// The kernel actually logs an OOM event as *several* separate log lines
/// spread across a few milliseconds (trigger -> constraint/cgroup info ->
/// the actual kill -> optional reaper confirmation). `OomEvent` is the
/// clean, joined-up view of all of that.
#[derive(Debug, Clone, Default)]
pub struct OomEvent {
    /// Raw timestamp string as found in the log (syslog time or kernel
    /// uptime in brackets). Kept as a string because the two source
    /// formats aren't directly comparable without extra context (boot time).
    pub timestamp: Option<String>,

    /// The timestamp resolved to real wall-clock time, when that could be done
    /// safely. `None` for an uptime-based stamp from a log this machine did not
    /// produce, where anchoring it to the local boot time would be a confident
    /// lie rather than a missing value.
    pub occurred_at: Option<chrono::DateTime<chrono::Local>>,

    // --- who triggered the allocation that failed ---
    pub trigger_process: Option<String>,
    pub gfp_mask: Option<String>,
    pub order: Option<u32>,

    // --- where the kernel decided to kill ---
    pub constraint: Option<String>,
    pub cgroup: Option<String>,

    /// The cgroup whose *limit* was breached (`oom_memcg`), which is not
    /// always the cgroup the victim lived in. When a parent slice's limit
    /// kills a child, this is the one to go and raise.
    pub limit_cgroup: Option<String>,

    /// True when the kill satisfied a *cgroup memory limit* rather than
    /// global host exhaustion. This is the first thing worth knowing about
    /// any containerised kill: hitting your own limit means "raise the limit
    /// or fix the leak", whereas global exhaustion means the host is
    /// oversubscribed and this container may just have been the biggest
    /// target rather than the actual cause.
    pub memcg_kill: bool,

    // --- the victim ---
    pub victim_name: String,
    pub victim_pid: u32,
    pub uid: Option<u32>,
    pub total_vm_kb: Option<u64>,
    pub anon_rss_kb: Option<u64>,
    pub file_rss_kb: Option<u64>,
    pub shmem_rss_kb: Option<u64>,
    pub pgtables_kb: Option<u64>,
    pub oom_score_adj: Option<i32>,

    /// Whether the oom_reaper subsequently confirmed memory was reclaimed.
    pub reaped: bool,

    /// Every task the kernel listed at the moment of the kill, if it printed
    /// a process table. Empty when the log was truncated or the dump was
    /// suppressed (`oom_dump_tasks=0`).
    pub processes: Vec<ProcessEntry>,

    /// System-wide memory state, if a `Mem-Info` block accompanied the kill.
    pub mem: Option<MemInfo>,

    /// The original, unmodified log lines that make up this event -
    /// kept so the user can always drop down to "just show me dmesg".
    pub raw_lines: Vec<String>,
}

impl OomEvent {
    /// Resident memory actually held by the victim at time of death
    /// (anon + file + shmem), in KB. This is usually the number people
    /// actually care about, more than total-vm (which includes unmapped
    /// virtual address space).
    pub fn rss_total_kb(&self) -> Option<u64> {
        match (self.anon_rss_kb, self.file_rss_kb, self.shmem_rss_kb) {
            (Some(a), Some(f), Some(s)) => Some(a + f + s),
            _ => None,
        }
    }

    /// Tasks from the process dump, largest resident set first.
    pub fn top_consumers(&self, limit: usize) -> Vec<&ProcessEntry> {
        let mut sorted: Vec<&ProcessEntry> = self.processes.iter().collect();
        sorted.sort_by(|a, b| b.rss_kb.cmp(&a.rss_kb));
        sorted.truncate(limit);
        sorted
    }

    /// The victim's share of total RAM, as a percentage.
    ///
    /// Absolute byte thresholds are meaningless without this: 400 MB is
    /// unremarkable on a 64 GB host and fatal on a 512 MB VM.
    pub fn rss_share_of_ram(&self) -> Option<f64> {
        let total = self.mem.as_ref()?.total_ram_kb?;
        if total == 0 {
            return None;
        }
        Some(self.rss_total_kb()? as f64 / total as f64 * 100.0)
    }

    /// Whether the process the kernel killed was actually the biggest memory
    /// user. When it wasn't, the victim is collateral damage and the name to
    /// investigate is the one above it in the table.
    pub fn victim_was_largest(&self) -> Option<bool> {
        let largest = self.processes.iter().max_by_key(|p| p.rss_kb)?;
        Some(largest.pid == self.victim_pid)
    }
}
