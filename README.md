<div align="center">

# OOM-TUI

**A terminal forensics console for Linux OOM kills.**  

[![CI](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Ashfaaq98/oom-tui?display_name=tag&sort=semver)](https://github.com/Ashfaaq98/oom-tui/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.75+](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](#development)
[![Platform: Linux](https://img.shields.io/badge/platform-linux-lightgrey.svg)](#requirements)

![oom-tui demo](docs/assets/demo.gif)

</div>

When Linux runs out of memory it kills a process and scatters the evidence
across half a dozen cryptic log lines. `oom-tui` reassembles them into one
browsable incident, and surfaces the detail almost everyone misses:
**the process the kernel killed is often not the one that caused the problem.**

At a glance, it tells you:

- **What died**: the process, its PID, and the memory it held.
- **Who was actually to blame**: the real memory hog, ranked against every
  other task, because the kernel targets the biggest RSS, not the leaker.
- **Why**: host-wide exhaustion vs. a container hitting its cgroup limit
  (very different fixes).
- **Proof**: the untouched kernel lines, one keypress away.

A forensics viewer for logs that already exist, not a monitor, a daemon, or a
root-cause oracle. Missing kernel data stays missing rather than guessed.

## Features

- **Kill types**: global, memory-cgroup, and `oom_kill_allocating_task`, across old and new kernel task-table layouts.
- **Container identity**: decodes cgroup paths into pod, container, QoS class, and runtime (Kubernetes, Docker, Podman, containerd, cri-o, systemd).
- **Sources**: journalctl (any boot), `dmesg`, syslog files, or a piped stream.
- **Structured output**: JSON, JSONL, and table, with `--exit-code` for CI checks.
- **Extras**: copy to clipboard (`c`), three color themes, wall-clock timestamps, and a single static binary with nothing to install to run it.

## Install

On Linux `x86_64` and `aarch64`, install the latest verified release into
`~/.local/bin`:

```bash
curl -fsSL https://github.com/Ashfaaq98/oom-tui/releases/latest/download/install.sh | sh
oom-tui --version
```

If `oom-tui` is not found, add `~/.local/bin` to your `PATH` (or reopen your shell).

For system installs, updates,
uninstalling, a specific version, checksum verification, or building from
source, see the [installation guide](docs/installation.md).

## Quick start

Open OOM incidents from the current boot's kernel journal:

```bash
oom-tui
```

No OOM kills yet? Explore a built-in sample incident. This works no matter how
you installed oom-tui, and needs no log access:

```bash
oom-tui --demo
```

Once inside, press `1` to `4` to scan different log sources directly from the
landing dashboard. Press `h` to toggle between the dashboard and the incident
console, `Tab` to cycle pane focus, arrow keys to select or scroll, `c` to copy
the focused pane, `t` to cycle themes, and `?` for the full keyboard reference.

## Common usage

```bash
# Inspect the boot before a reboot
oom-tui --boot -1

# Read an exported log or piped journal
oom-tui --file customer-dmesg.txt
journalctl -k | oom-tui --file -

# Use structured output in a script
oom-tui --format json | jq -r '.[] | select(.scope == "cgroup") | .victim_name'
```

| Option | Purpose |
| --- | --- |
| `--demo` | Explore a built-in sample incident (no log access needed). |
| `-f`, `--file <FILE>` | Read a file; use `-` for stdin. |
| `-b`, `--boot <N>` | Inspect boot `0` (current), `-1` (previous), and so on. |
| `--all-boots` | Search every retained journal boot. |
| `--since <TIME>` / `--until <TIME>` | Restrict a journal time range. |
| `--format <FMT>` | `auto` (default: TUI on a terminal, table when piped), or force `tui`, `table`, `json`, `jsonl`. |
| `--exit-code` | Exit `1` if one or more OOM events are found. |

Run `oom-tui --help` for the complete CLI reference.

## How it works

```
Kernel log -> Parser (regex state machine) -> OomEvent model -> Analysis & Diagnosis -> TUI or structured output
```

The parser reads kernel log text from any supported source (journalctl, dmesg,
syslog files, or stdin), extracts OOM kill sequences using a regex-driven state
machine, and builds structured `OomEvent` records. Each event captures the
victim process, memory stats, cgroup context, and the raw kernel lines as
evidence. The analysis layer then classifies events as host-wide or
cgroup-scoped, identifies collateral kills, and generates a human-readable
diagnosis, all rendered in an interactive [ratatui](https://ratatui.rs)-powered
terminal UI.

## Requirements

Linux is required. By default, `oom-tui` reads the first usable source in this
order: `journalctl`, `dmesg -T`, `dmesg`, `/var/log/syslog`, then
`/var/log/messages`. A supplied file or stdin takes precedence. You may need
permission to read the kernel journal, such as membership of `systemd-journal`
or `sudo`.

JSON field names are stable within a major version.

## Development

The minimum supported Rust version is 1.75. Built with
[ratatui](https://ratatui.rs) for the terminal UI.

```bash
git clone https://github.com/Ashfaaq98/oom-tui
cd oom-tui
cargo run -- --file examples/sample-oom.log
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for parser fixtures, conventions, and
fuzzing. Misparsed real-world logs are valuable: please [open an
issue](https://github.com/Ashfaaq98/oom-tui/issues/new/choose) with relevant,
redacted kernel lines.

## License

[MIT](LICENSE)
