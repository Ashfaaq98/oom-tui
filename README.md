<div align="center">

# oom-tui

**OOM-killer forensics for Linux.** Reconstructs scattered kernel log lines into readable OOM incidents, so you can see what was killed, why, and what else was using memory.

[![CI](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.75+](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](#development)
[![Platform: Linux](https://img.shields.io/badge/platform-linux-lightgrey.svg)](#requirements)

</div>

`oom-tui` turns kernel OOM output into a browsable timeline and incident view. It distinguishes host-wide exhaustion from cgroup limits, reports the victim's memory breakdown, ranks the kernel's task dump, and decodes common container and Kubernetes cgroup paths.

The process that was killed is not necessarily the process that caused the pressure. `oom-tui` makes that evidence visible.

## Install

Download a static Linux binary from [GitHub Releases](https://github.com/Ashfaaq98/oom-tui/releases):

```bash
curl -L https://github.com/Ashfaaq98/oom-tui/releases/latest/download/oom-tui-x86_64-unknown-linux-musl.tar.gz | tar xz
sudo install oom-tui-*/oom-tui /usr/local/bin/oom-tui
```

Both `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` archives are published.

Or build from source (Rust 1.75+):

```bash
git clone https://github.com/Ashfaaq98/oom-tui
cd oom-tui
cargo build --release
./target/release/oom-tui --help
```

## Quick start

```bash
oom-tui
```

By default, it reads the current boot's kernel journal and opens an interactive TUI. When stdout is piped, it emits a plain table instead.

To inspect the bundled example:

```bash
oom-tui --file examples/sample-oom.log
```

## Usage

```text
oom-tui [OPTIONS]
```

| Option | Description |
| --- | --- |
| `-f`, `--file <FILE>` | Read a log file; use `-` for stdin. |
| `-b`, `--boot <N>` | Inspect boot `N`: `0` is current and `-1` is previous. |
| `--all-boots` | Search every boot retained by the journal. |
| `--since <TIME>` / `--until <TIME>` | Restrict the journal time range. |
| `--format <FMT>` | `auto` (default), `tui`, `table`, `json`, or `jsonl`. |
| `--exit-code` | Exit `1` when one or more OOM events are found. |

### TUI keys

| Key | Action |
| --- | --- |
| `↑`/`k`, `↓`/`j` | Select an incident; scroll raw evidence when it is open. |
| `l` | Toggle raw kernel lines. |
| `PgUp`/`PgDn`, `g`/`G` | Scroll raw evidence. |
| `R` | Reload the selected source. |
| `q`/`Esc` | Quit. |

## Examples

Inspect an OOM kill from the boot before a reboot:

```bash
oom-tui --boot -1
```

Analyse an exported kernel log or a pipe:

```bash
oom-tui --file customer-dmesg.txt
journalctl -k | oom-tui --file -
```

Find cgroup-limit failures with `jq`:

```bash
oom-tui --format json | jq -r '.[] | select(.scope == "cgroup") | .victim_name'
```

Use it in a check; a found event produces exit status `1`:

```bash
oom-tui --since "1 hour ago" --exit-code >/dev/null || echo "OOM event detected"
```

## Log sources and requirements

Linux is required. `oom-tui` reads the first available source in this order:

1. `--file <path>` or stdin
2. `journalctl`
3. `dmesg -T`, then `dmesg`
4. `/var/log/syslog`, then `/var/log/messages`

`--boot`, `--all-boots`, `--since`, and `--until` require `journalctl`; a fallback source is explicitly flagged when it cannot honour those filters. You may need permission to read the kernel journal (for example, membership of `systemd-journal`) or to run the command with `sudo`.

## Output and compatibility

`--format json` returns one JSON array; `--format jsonl` returns one object per line. The JSON field names are a stable public contract within a major version.

The parser handles global OOM kills, memory-cgroup OOM kills, and `oom_kill_allocating_task` logs, including older task-table layouts. It only reports evidence present in the log; a truncated task dump or an unavailable timestamp remains incomplete rather than guessed.

This is a viewer for existing OOM evidence, not a memory monitor, daemon, alerting system, or eBPF tracer.

## Development

The minimum supported Rust version is 1.75.

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for parser fixtures, development conventions, and fuzzing guidance. If a real-world log is misparsed, please [open an issue](https://github.com/Ashfaaq98/oom-tui/issues/new/choose) with the relevant redacted kernel lines.

## License

Licensed under the [MIT License](LICENSE).
