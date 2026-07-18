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

    // --- who triggered the allocation that failed ---
    pub trigger_process: Option<String>,
    pub gfp_mask: Option<String>,
    pub order: Option<u32>,

    // --- where the kernel decided to kill ---
    pub constraint: Option<String>,
    pub cgroup: Option<String>,

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
}
