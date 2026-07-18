# Contributing to oom-tui

## The most valuable contribution is a weird `dmesg`

`oom-tui` parses unstructured kernel output whose shape has changed across
kernel versions, distributions, and container runtimes. No amount of careful
coding substitutes for real logs from real machines.

If `oom-tui` misparses an event, or shows nothing when you know a kill
happened, please [open an issue][issues] with the raw lines. Redact hostnames
and anything sensitive first — the structure is what matters. Every such report
becomes a permanent test fixture.

[issues]: https://github.com/Ashfaaq98/oom-tui/issues/new/choose

## Getting set up

```bash
git clone https://github.com/Ashfaaq98/oom-tui
cd oom-tui
cargo test
cargo run -- --file <some-log>
```

There is nothing else to install. The tool shells out to `journalctl`/`dmesg`
at runtime but needs neither to run its tests.

## Before opening a pull request

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs exactly these, plus a build against the minimum supported Rust version
(1.75) and a cross-compile to static musl targets.

## How this codebase is organised

| File | Responsibility |
| --- | --- |
| `src/parser.rs` | Reassembles scattered `printk` lines into events. The core. |
| `src/model.rs` | `OomEvent` and its derived questions (`victim_was_largest`, …). |
| `src/source.rs` | Locating logs: journalctl, dmesg, files, stdin. |
| `src/container.rs` | Decoding container/pod identity from cgroup paths. |
| `src/timestamp.rs` | Resolving the three timestamp formats to wall-clock time. |
| `src/report.rs` | Non-interactive output (JSON, JSONL, table). |
| `src/ui.rs`, `src/app.rs` | The `ratatui` dashboard. |

## Conventions worth knowing

**Every parser change needs a fixture.** Add the real log line to the tests in
`src/parser.rs`. Those tests double as documentation of what the kernel
actually emits — keep them realistic rather than minimal.

**Never invent data.** If a field is absent from the log, it is `None`. A
plausible-looking guess is worse than a gap, because someone is making an
operational decision from this output. The same rule is why an uptime timestamp
from another machine's log is left unresolved instead of being anchored to the
local boot time.

**The parser must never panic.** It reads input from machines nobody controls,
usually mid-incident. See `parser::robustness` for the pinned cases, and
`fuzz/` for the wider exploration (`cargo fuzz run parse_log`, needs nightly).

**The JSON schema is a contract.** `report::EventJson` is a deliberate view
struct, not a `Serialize` derive on the internal type, so internals can change
freely. Renaming or removing a field there is a breaking change.

## Scope

This is a forensics viewer for logs that already exist. It is deliberately not
a memory monitor, a `top`/`htop` clone, a daemon, an alerting system, or an
eBPF tracer. Features that pull in that direction will likely be declined —
please open an issue to discuss before building something large.

## License

By contributing, you agree that your contributions will be licensed under the
MIT License, matching the rest of the project.
