# oom-tui

**Why did the kernel kill my process?** A terminal UI for OOM-killer forensics
on Linux.

[![CI](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](#license)

---

## The problem

Something got `SIGKILL`ed and you don't know why. `kubectl describe pod` says
`OOMKilled`, exit code 137, and stops there. Your metrics dashboard samples
every 30 seconds and missed the spike entirely.

The kernel *does* know exactly what happened — but it reports it as several
unrelated `printk` lines, in different formats, milliseconds apart, buried in
`dmesg`:

```
[  767.925606] stress invoked oom-killer: gfp_mask=0x100cca(GFP_HIGHUSER_MOVABLE), order=0, oom_score_adj=0
[  767.925620] oom-kill:constraint=CONSTRAINT_NONE,nodemask=(null),cpuset=/,mems_allowed=0,global_oom,
               task_memcg=/user.slice/user-1000.slice/session-1.scope,task=stress,pid=1433,uid=1000
[  767.925620] Out of memory: Killed process 1433 (stress) total-vm:265804kB, anon-rss:222856kB,
               file-rss:0kB, shmem-rss:0kB, UID:1000 pgtables:496kB oom_score_adj:0
[  767.973170] oom_reaper: reaped process 1433 (stress), now anon-rss:0kB, file-rss:0kB, shmem-rss:0kB
```

Four lines, no shared structure, nothing tying them together. `oom-tui`
reassembles them into one browsable incident.

## What it gives you

- **A timeline** of every OOM kill in the log, colour-coded by how much memory
  the victim was holding.
- **An autopsy** for the selected kill: victim process, PID, UID, RSS
  breakdown, `oom_score_adj`, the cgroup it belonged to, what triggered the
  failing allocation, and whether the reaper confirmed cleanup.
- **Container vs. host scope** — the most useful single field for containerised
  workloads. Did this process blow through *its own cgroup limit*, or did the
  *whole machine* run out? Those have completely different fixes: raise the
  limit / fix the leak, versus stop oversubscribing the node.
- **The raw lines, always one keypress away.** Press `l` to drop to the
  original kernel output for the selected event. A parser you can't check isn't
  one you should trust.

## Install

```bash
cargo install oom-tui
```

Or build from source:

```bash
git clone https://github.com/Ashfaaq98/oom-tui
cd oom-tui
cargo build --release   # binary at target/release/oom-tui
```

## Usage

```bash
oom-tui                              # live: journalctl -k, falling back to dmesg
oom-tui --file /path/to/kernel.log   # analyse a log copied off another machine
```

| Key | Action |
| --- | --- |
| `↑` / `k` | previous event |
| `↓` / `j` | next event |
| `l` | toggle raw kernel log for this event |
| `R` | reload from the live source |
| `q` / `Esc` | quit |

### Where it reads from

Tried in order; the first source that yields output wins:

1. `--file <path>`, if given
2. `journalctl -k -o short --no-pager`
3. `dmesg -T`
4. `dmesg`
5. `/var/log/syslog`, then `/var/log/messages`

Reading kernel logs needs permission: membership of the `systemd-journal` group
is enough for `journalctl`, otherwise `dmesg` typically wants root (depending on
`kernel.dmesg_restrict`). `oom-tui` shells out to those standard tools and never
touches kernel memory itself.

### Kill formats understood

The kernel picks its message prefix based on which code path did the killing,
so all three are handled:

| Log line | Meaning |
| --- | --- |
| `Out of memory: Killed process …` | global — the host ran out |
| `Memory cgroup out of memory: Killed process …` | a cgroup/container hit its limit |
| `Out of memory (oom_kill_allocating_task): Killed process …` | the `oom_kill_allocating_task` sysctl is set |

Modern and pre-4.19 kernels are both supported (older ones omit the `pgtables:`
field entirely).

## Trying it without wrecking your machine

Generate a real event inside a tight cgroup, so the damage is contained instead
of taking down your desktop:

```bash
systemd-run --user --scope -p MemoryMax=100M \
  stress-ng --vm 1 --vm-bytes 200M --timeout 10s
oom-tui
```

## Current limitations

Being upfront about what it does **not** do yet:

- Only reads the **current boot** via `journalctl -k` (which implies `-b`), so
  an OOM kill that *caused* a reboot won't show up. Export the log and use
  `--file` as a workaround.
- Does not yet parse the kernel's **full process table dump** or the `Mem-Info`
  block, so it tells you who died but not yet who else was hogging memory. (The
  OOM killer targets the largest RSS, which is frequently *not* the process
  that actually leaked.)
- Timestamps are shown exactly as the log records them and are not normalised
  across sources, so events can't yet be sorted or filtered by time.
- No JSON output or stdin support yet.
- The raw-log pane doesn't scroll, so very long events are cut off on screen.

## Contributing

**The most valuable contribution is a weird `dmesg`.** This is a parser for
unstructured kernel output whose shape has changed across kernel versions,
distributions, and container runtimes. If `oom-tui` misparses or misses an
event on your system, please open an issue with the raw lines (redact hostnames
and process names as needed) — those become test fixtures, which is the only
real defence against format drift.

Bug reports, kernel-version reports, and PRs all welcome.

## Non-goals

This is a forensics viewer for logs that already exist. It is deliberately
**not** a memory monitor, a `top`/`htop` clone, a daemon, an alerting system, or
an eBPF tracer. That space is well served already; this tool does one thing.

## Minimum supported Rust version

1.75, enforced in CI. Dependency versions are pinned to stay compatible with it.

## License

MIT — see [LICENSE](LICENSE).

Unless you state otherwise, any contribution you intentionally submit for
inclusion in this work shall be licensed as above, without any additional terms
or conditions.
