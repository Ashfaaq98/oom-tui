<div align="center">

# oom-tui

**Why did the kernel kill my process?**

A terminal UI for OOM-killer forensics on Linux. It reassembles the kernel's
scattered log lines into one readable incident — and tells you whether the
process it killed was actually the one to blame.

[![CI](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.75+](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](#minimum-supported-rust-version)
[![Platform: Linux](https://img.shields.io/badge/platform-linux-lightgrey.svg)](#requirements)

<!-- Add once published to crates.io:
[![crates.io](https://img.shields.io/crates/v/oom-tui.svg)](https://crates.io/crates/oom-tui)
[![downloads](https://img.shields.io/crates/d/oom-tui.svg)](https://crates.io/crates/oom-tui)
-->

</div>

---

```
 OOM // INCIDENT CONSOLE
 1 incident •  journalctl (current boot)

┌ INCIDENT TIMELINE  ·  newest last ───────────────────────────────────────────────────────────┐
│▌ ● postgres  PID 1433  312.5 MiB  ·  320000 kB                                               │
│    2026-07-18 09:12:44  (2h ago)  ·  unconfirmed  ·  host / unspecified cgroup               │
│                                                                                              │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
┌ INCIDENT CONTEXT ───────────────┐┌ MEMORY AUTOPSY ────────┐┌ TOP CONSUMERS  ·  victim was NOT┐
│VICTIM           postgres  (PID 1││RSS TOTAL       312.5 Mi││  leaky-worker         1172 MiB  │
│SCOPE            host-wide exhaus││ANONYMOUS RSS   312.5 Mi││  chrome               371 MiB   │
│WHEN             2026-07-18 09:12││FILE RSS        0.0 MiB ││▶ postgres             312 MiB   │
│UID              999             ││SHMEM RSS       0.0 MiB ││  systemd-journal      2 MiB     │
│OOM SCORE ADJ    0               ││PAGE TABLES     1.0 MiB ││                                 │
│WORKLOAD         —               ││SHARE OF RAM    15.3% of││                                 │
│CGROUP           —               ││MACHINE RAM     2048.0 M││                                 │
│TRIGGER          postgres        ││RAW LINES       9 captur││                                 │
└─────────────────────────────────┘└────────────────────────┘└─────────────────────────────────┘
                        ↑/k navigate   l raw evidence   R reload   q quit
```

Note the panel on the right: `postgres` was killed, but `leaky-worker` was
holding nearly four times as much memory. That distinction is the whole point.

## Table of contents

- [The problem](#the-problem)
- [Features](#features)
- [Installation](#installation)
- [Quick start](#quick-start)
- [Usage](#usage)
- [Examples](#examples)
- [JSON output](#json-output)
- [What the kernel actually logs](#what-the-kernel-actually-logs)
- [Troubleshooting](#troubleshooting)
- [Limitations](#limitations)
- [Contributing](#contributing)
- [License](#license)

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

Four lines, no shared structure, nothing tying them together — plus a task
table hundreds of rows long that almost nobody reads. `oom-tui` reassembles all
of it into one browsable incident.

## Features

- **Timeline** of every OOM kill in the log, colour-coded by severity.
- **Autopsy** per kill: victim, PID, UID, RSS breakdown, `oom_score_adj`,
  cgroup, triggering allocation, and reaper confirmation.
- **Container vs. host scope.** Did the process blow through *its own cgroup
  limit*, or did the *whole machine* run out? Completely different fixes:
  raise the limit / fix the leak, versus stop oversubscribing the node.
- **Who else was holding memory.** The kernel dumps every task when it fires.
  `oom-tui` parses and ranks that table, which regularly shows the victim was
  *not* the leaker — the OOM killer targets the largest resident set, not the
  culprit.
- **Workload identity.** Kubernetes and Docker cgroup paths decode to pod UIDs,
  container IDs, QoS class and systemd units. No cluster access required; the
  identity is already in the path.
- **Real timestamps.** Kernel uptime, `dmesg -T` and syslog formats all resolve
  to wall-clock time with a relative hint.
- **Severity as share of RAM**, not absolute bytes — 400 MB is noise on a 64 GB
  host and fatal on a 512 MB VM.
- **JSON output** for scripting, dashboards and CI checks.
- **Raw lines one keypress away.** A parser you can't check isn't one you
  should trust.

## Requirements

Linux, with kernel logs readable via `journalctl`, `dmesg`, or a log file.
`oom-tui` shells out to those standard tools and never touches kernel memory
itself.

## Installation

### Prebuilt binary (recommended)

Statically linked against musl, so it needs no toolchain and no matching
glibc — which matters when the machine that just OOMed is the one you're
debugging.

```bash
curl -L https://github.com/Ashfaaq98/oom-tui/releases/latest/download/oom-tui-x86_64-unknown-linux-musl.tar.gz | tar xz
sudo install oom-tui /usr/local/bin/
```

`aarch64-unknown-linux-musl` builds are published alongside.

### From source

```bash
git clone https://github.com/Ashfaaq98/oom-tui
cd oom-tui
cargo build --release   # binary at target/release/oom-tui
```

## Quick start

```bash
oom-tui
```

That's it. It finds the logs, parses them, and opens the dashboard.

**Seeing an empty timeline?** That usually means good news: nothing has been
OOM-killed on this boot. To see the tool working with real data, either open
the bundled sample:

```bash
oom-tui --file examples/sample-oom.log
```

or generate a genuine kill, safely contained to a 100 MB cgroup so nothing
else on the machine is at risk:

```bash
systemd-run --user --scope -p MemoryMax=100M -p MemorySwapMax=0 \
  python3 -c 'b=bytearray()
while True: b.extend(bytearray(5*1024*1024))'
oom-tui
```

## Usage

```
oom-tui [OPTIONS]
```

| Option | Description |
| --- | --- |
| `-f`, `--file <FILE>` | Read from a file instead of the live log. `-` means stdin. |
| `-b`, `--boot <N>` | Boot offset: `0` current, `-1` previous. Finds the kill that *caused* a reboot. |
| `--all-boots` | Search every boot the journal still retains. |
| `--since <TIME>` | Only events after this time (e.g. `"2 days ago"`). |
| `--until <TIME>` | Only events before this time. |
| `--format <FMT>` | `auto` (default), `tui`, `table`, `json`, `jsonl`. |
| `--exit-code` | Exit `1` when any event was found — for CI and monitoring. |
| `-h`, `--help` / `-V`, `--version` | Help and version. |

`auto` renders the dashboard on a terminal and a plain table when piped, so
`oom-tui | grep postgres` does the obvious thing.

### Keyboard shortcuts

| Key | Action |
| --- | --- |
| `↑` / `k`, `↓` / `j` | Move between incidents |
| `l` | Toggle the raw kernel log for this incident |
| `PgUp` / `PgDn`, `g` / `G` | Scroll the raw log |
| `R` | Reload from the source |
| `q` / `Esc` | Quit |

### Where it reads from

Tried in order; the first source that yields output wins:

1. `--file <path>` (or stdin), if given
2. `journalctl -k -o short --no-pager`
3. `dmesg -T`
4. `dmesg`
5. `/var/log/syslog`, then `/var/log/messages`

If a fallback source can't honour `--boot` or `--since`, `oom-tui` says so
rather than quietly showing you the wrong window of history.

## Examples

**Find the kill that caused last night's reboot**

```bash
oom-tui --boot -1
```

**Investigate a log someone sent you**

```bash
oom-tui --file ./customer-dmesg.txt
journalctl -k | oom-tui --file -          # or straight from a pipe
```

**List every case where the kernel killed the wrong process**

```bash
oom-tui --format json | jq -r '
  .[] | select(.victim_was_largest == false)
  | "\(.victim_name) died but \(.top_consumers[0].name) held \(.top_consumers[0].rss_kb/1024|floor)MiB"'
```

**Find which container hit its limit**

```bash
oom-tui --format json | jq -r '
  .[] | select(.scope == "cgroup")
  | "\(.workload.runtime) pod=\(.workload.pod_uid) killed \(.victim_name)"'
```

**Use it as a health check**

```bash
oom-tui --since "1 hour ago" --exit-code >/dev/null || echo "OOM kill in the last hour!"
```

**Append to a log for later analysis**

```bash
oom-tui --format jsonl >> /var/log/oom-incidents.jsonl
```

## JSON output

The JSON schema is a **stable contract** — field names won't change without a
major version bump. Selected fields:

| Field | Meaning |
| --- | --- |
| `occurred_at` | RFC 3339 wall-clock time, when the log's epoch could be trusted |
| `victim_name`, `victim_pid`, `uid` | Who died |
| `scope` | `"cgroup"` (container limit) or `"host"` (machine exhausted) |
| `workload` | Decoded `runtime`, `pod_uid`, `container_id`, `qos_class`, `unit` |
| `cgroup`, `limit_cgroup` | Where the task lived; whose limit was breached |
| `rss_total_kb`, `rss_percent_of_ram` | Victim's memory, absolute and relative |
| `total_ram_kb`, `swap_total_kb` | Machine state at the moment of the kill |
| `victim_was_largest` | `false` means the kernel killed collateral damage |
| `top_consumers[]` | Every task the kernel listed, largest RSS first |
| `reaped` | Whether the reaper confirmed the memory came back |

## What the kernel actually logs

The kernel picks its message prefix based on which code path did the killing.
All three are handled:

| Log line | Meaning |
| --- | --- |
| `Out of memory: Killed process …` | Global — the host ran out |
| `Memory cgroup out of memory: Killed process …` | A cgroup/container hit its limit |
| `Out of memory (oom_kill_allocating_task): Killed process …` | The `oom_kill_allocating_task` sysctl is set |

Both modern and pre-4.19 kernels are supported, including the older task-table
layout (`nr_ptes`/`nr_pmds`) and lines with no `pgtables:` field.

## Troubleshooting

**"couldn't read logs from journalctl, dmesg, …"**

You lack permission to read kernel logs. Either join the `systemd-journal`
group (`sudo usermod -aG systemd-journal $USER`, then log out and back in), or
run under `sudo`. Some distributions also restrict `dmesg` via
`kernel.dmesg_restrict=1`.

**The timeline is empty but I know a process was killed**

- The kill may predate the current boot. Try `--all-boots` or `--boot -1`.
- The dmesg ring buffer may have wrapped and dropped it. Try the journal
  instead, which persists across boots when storage is enabled.
- The process may have been killed by something other than the OOM killer —
  a liveness probe, systemd's `OOMPolicy`, or a plain `SIGKILL` won't appear
  here, because the kernel never logged an OOM event.

**Times show as `+767.9s` instead of a date**

That's a kernel uptime stamp from a log this machine didn't produce. Anchoring
it to *your* boot time would give a confidently wrong date, so the raw value is
shown instead. Logs read live from this machine resolve normally.

**No `TOP CONSUMERS` panel**

The log has no task dump — either it was truncated, or the kernel was
configured with `oom_dump_tasks=0`.

## Limitations

- No search or filtering inside the TUI, and no grouping by victim — if a
  process died fourteen times you get fourteen rows. Use `--format json` and
  `jq` for now.
- `--all-boots` reads the entire kernel journal with no upper bound, which is
  slow on machines with a long history — a 3.7 GB journal spanning 58 boots
  takes minutes. Pair it with `--since` to keep it quick.
- Page size is assumed to be 4 KiB when converting the task table, which is
  wrong on architectures configured with 16K/64K pages.
- Not yet published to crates.io.

## Contributing

**The most valuable contribution is a weird `dmesg`.** This is a parser for
unstructured kernel output whose shape has changed across kernel versions,
distributions, and container runtimes. If `oom-tui` misparses or misses an
event on your system, please [open an issue][new-issue] with the raw lines —
those become test fixtures, which is the only real defence against format
drift.

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and conventions.

[new-issue]: https://github.com/Ashfaaq98/oom-tui/issues/new/choose

## Non-goals

This is a forensics viewer for logs that already exist. It is deliberately
**not** a memory monitor, a `top`/`htop` clone, a daemon, an alerting system,
or an eBPF tracer. That space is well served already; this tool does one thing.

## Minimum supported Rust version

1.75, enforced in CI against the committed lockfile.

## License

MIT — see [LICENSE](LICENSE).

Unless you state otherwise, any contribution you intentionally submit for
inclusion in this work shall be licensed as above, without any additional terms
or conditions.
