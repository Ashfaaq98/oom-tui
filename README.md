# OOM-TUI

**Interactive Linux OOM incident investigation.** `oom-tui` reconstructs
scattered kernel log lines into incidents you can investigate without losing
the original evidence.

[![CI](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/Ashfaaq98/oom-tui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Ashfaaq98/oom-tui?display_name=tag&sort=semver)](https://github.com/Ashfaaq98/oom-tui/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.75+](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](#development)
[![Platform: Linux](https://img.shields.io/badge/platform-linux-lightgrey.svg)](#requirements)

It helps answer three questions quickly:

1. What happened: which process was killed and how much memory it used.
2. Why: whether the kernel reported host-wide pressure or a cgroup limit.
3. What proves it: the untouched kernel evidence beside the analysis.

It is a forensics viewer for existing logs, not a memory monitor, daemon, or
root-cause oracle. Missing kernel data stays missing rather than guessed.

## Install

On Linux `x86_64` and `aarch64`, install the latest verified release into
`~/.local/bin`:

```bash
curl -fsSL https://github.com/Ashfaaq98/oom-tui/releases/latest/download/install.sh | sh
oom-tui --version
```

Ensure `~/.local/bin` is on your `PATH`. For system installs, updates,
uninstalling, a specific version, checksum verification, or building from
source, see the [installation guide](docs/installation.md).

## Quick start

Open OOM incidents from the current boot's kernel journal:

```bash
oom-tui
```

Try the bundled example:

```bash
oom-tui --file examples/sample-oom.log
```

The TUI leads from the incident list to a concise summary, diagnosis, and raw
kernel evidence. `Tab` changes panes; arrow keys select or scroll; `?` opens
the complete keyboard reference.

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
| `-f`, `--file <FILE>` | Read a file; use `-` for stdin. |
| `-b`, `--boot <N>` | Inspect boot `0` (current), `-1` (previous), and so on. |
| `--all-boots` | Search every retained journal boot. |
| `--since <TIME>` / `--until <TIME>` | Restrict a journal time range. |
| `--format <FMT>` | Choose `tui`, `table`, `json`, or `jsonl`. |
| `--exit-code` | Exit `1` if one or more OOM events are found. |

Run `oom-tui --help` for the complete CLI reference.

## Requirements

Linux is required. By default, `oom-tui` reads the first usable source in this
order: `journalctl`, `dmesg -T`, `dmesg`, `/var/log/syslog`, then
`/var/log/messages`. A supplied file or stdin takes precedence. You may need
permission to read the kernel journal, such as membership of `systemd-journal`
or `sudo`.

The parser supports global OOM kills, memory-cgroup OOM kills, and
`oom_kill_allocating_task` reports. JSON field names are stable within a major
version.

## Development

The minimum supported Rust version is 1.75.

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
