# oom-tui

A single-dashboard terminal UI for **OOM-killer forensics** on Linux.

## The problem

When Linux runs out of memory, the kernel's OOM killer abruptly `SIGKILL`s a
process to save the system. The evidence for *why* is scattered across
several separate, differently-formatted `printk` lines in `dmesg` /
`journalctl -k` / `/var/log/syslog` — a trigger line, a constraint/cgroup
line, the actual kill line, and sometimes a reaper confirmation line —
printed milliseconds apart with no shared structure. Reconstructing "what
died, how much RAM did it hold, and what was going on at the time" today
means manually reading raw kernel log dumps.

`oom-tui` does that reconstruction for you and shows it as one browsable
dashboard.

## What it does

- Pulls kernel logs from (in order of preference): a file you pass with
  `--file`, `journalctl -k`, `dmesg -T`, `dmesg`, or `/var/log/syslog`.
- Parses the scattered log lines with regex + a small state machine and
  groups them back into one `OomEvent` per kill (see `src/parser.rs`).
- Renders a single dashboard: a timeline of every kill event on top, and a
  clean "autopsy" table for the selected event below — victim process, PID,
  UID, RSS breakdown, oom_score_adj, the cgroup it belonged to, what
  triggered the allocation, and whether the reaper confirmed cleanup.
- `l` toggles down to the original raw log lines for that event, so you can
  always sanity-check the parse.

## Running it

```bash
cargo run -- --file path/to/some.log   # analyze a saved log
cargo run                              # live: journalctl -k / dmesg
```

Keys: `↑/k` `↓/j` navigate, `l` raw log view, `R` reload from the live
source, `q`/`Esc` quit.

## Trying it without a real OOM event

```bash
# careful: this will actually trigger your system's OOM killer
stress-ng --vm 2 --vm-bytes 90% --timeout 10s
```
or just point `--file` at the sample log used in the test suite
(`src/parser.rs` has one inline).

## Why this project

Built to learn Rust's ecosystem end to end on one deliberately small, real
problem: `regex` + `Option`/`Result`-heavy parsing of messy unstructured
text, a `clap`-driven CLI, and a `ratatui`/`crossterm` TUI — while avoiding
scope creep into "yet another top/htop clone" (the Rust TUI space is
already saturated there). See `src/parser.rs` for the parsing core and its
unit tests, which double as parser documentation via real dmesg/syslog
samples.

## Notes on MSRV

Pinned to dependency versions compatible with older toolchains (this was
built/tested against rustc 1.75); bump `ratatui`/`clap`/`crossterm` freely
if your toolchain is newer.
