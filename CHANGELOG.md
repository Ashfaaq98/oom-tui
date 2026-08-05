# Changelog

All notable changes to oom-tui are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While pre-1.0, the **minor** version (the `y` in `0.y.z`) increments on breaking
or behaviour changes; the **patch** version is for backwards-compatible fixes.

## [Unreleased]

_Nothing yet._

## [0.4.0] - 2026-08-05

### Added

- `--demo` flag, and an "Explore Built-in Sample" action (`4`) on the landing
  dashboard, that open a bundled sample incident. The sample is compiled into
  the binary, so it works on every install method with no log access — you can
  see the tool in action before you have a real OOM kill.

### Changed

- The bundled sample (`examples/sample-oom.log`) is now a realistic host-wide
  OOM on an 8 GiB laptop that demonstrates a _culprit mismatch_: the kernel
  kills a 1.7 GiB Chrome tab while a leaking 4.9 GiB `python3` script was the
  real memory hog. Chrome flags its own tabs as preferred OOM victims, which is
  why the smaller process dies first.

### Fixed

- **No longer panics on hostile input.** Summing three near-`u64::MAX` RSS
  values in `rss_total_kb` overflowed — a panic in debug builds, a silently
  wrapped (tiny) figure in release. It now saturates.
- **rsyslog logs are no longer dropped.** Standard Debian/Ubuntu
  `/var/log/kern.log` lines carry both a syslog prefix and the kernel's own
  embedded `[uptime]` bracket; these were parsed as zero events and are now
  read correctly.
- **A truncated report no longer corrupts the next one.** If a "Killed process"
  line is lost to a dmesg ring-buffer wrap, that report's process table no
  longer bleeds into the following event's top-consumers analysis.
- **Narrow terminals.** Below 90 columns the focus cycle (`Tab`) no longer
  lands on the Evidence pane, which isn't drawn in that layout.
- **The sample is reachable after install.** The landing action and README
  previously pointed at `examples/sample-oom.log`, which does not ship with the
  binary; both now use the embedded demo.
- Additional parsing fixes for process names and syslog dates.

### Removed

- A false claim in the README that cgroup decoding includes the Kubernetes
  _namespace_ — a namespace is not encoded in the cgroup path and was never
  derived. QoS class (which _is_ decoded) is now documented instead.

## [0.3.0] - 2026-07-31

Container-native forensics, timeline polish, and structured output. See the
[release notes](https://github.com/Ashfaaq98/oom-tui/releases/tag/v0.3.0).

## [0.2.0] - 2026-07-29

Multi-boot support, time-range filtering, JSON/table output, and stdin input.
See the [release notes](https://github.com/Ashfaaq98/oom-tui/releases/tag/v0.2.0).

## [0.1.0] - 2026-07-18

First release. See the
[release notes](https://github.com/Ashfaaq98/oom-tui/releases/tag/v0.1.0).

[Unreleased]: https://github.com/Ashfaaq98/oom-tui/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/Ashfaaq98/oom-tui/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/Ashfaaq98/oom-tui/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Ashfaaq98/oom-tui/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Ashfaaq98/oom-tui/releases/tag/v0.1.0
