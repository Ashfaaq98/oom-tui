//! End-to-end test that drives the real binary through a pseudo-terminal.
//!
//! `ui.rs` has no other integration coverage, and the failure that matters most
//! for a TUI is leaving the terminal wrecked - alternate screen not restored,
//! cursor hidden - after a quit or a panic. A plain pipe cannot exercise that,
//! because without a TTY the tool deliberately prints a table instead of the UI.
//! So we allocate a PTY, run `oom-tui --demo` in it, and assert the terminal is
//! handed back cleanly.
//!
//! Runs only on unix (forkpty); a no-op elsewhere.

#![cfg(unix)]

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::time::{Duration, Instant};

// crossterm's control sequences we assert on.
const ALT_ENTER: &[u8] = b"\x1b[?1049h"; // enter alternate screen (TUI started)
const ALT_EXIT: &[u8] = b"\x1b[?1049l"; // leave alternate screen (restored)
const CURSOR_SHOW: &[u8] = b"\x1b[?25h"; // cursor made visible again

struct Session {
    output: Vec<u8>,
    exit_code: i32,
}

/// Launch the binary in a PTY, wait until it has rendered, send `keys`, then
/// collect the rest of the output and the exit status.
fn run_in_pty(bin: &str, args: &[&str], keys: &[u8]) -> Session {
    let mut master: libc::c_int = 0;
    // SAFETY: standard forkpty; the child path only calls async-signal-safe libc
    // functions (execvp/_exit) before replacing the image.
    let pid = unsafe {
        libc::forkpty(
            &mut master,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert!(pid >= 0, "forkpty failed");

    if pid == 0 {
        // Child: exec the binary. Build the C strings here (post-fork) but only
        // from data the parent already owned - no allocation-heavy work.
        let path = CString::new(bin).unwrap();
        let mut argv: Vec<CString> = vec![CString::new(bin).unwrap()];
        argv.extend(args.iter().map(|a| CString::new(*a).unwrap()));
        let mut ptrs: Vec<*const libc::c_char> = argv.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        unsafe {
            libc::execvp(path.as_ptr(), ptrs.as_ptr());
            libc::_exit(127); // only reached if execvp failed
        }
    }

    // Parent: read until the UI has entered the alternate screen, then send keys.
    let mut output: Vec<u8> = Vec::new();
    let started = wait_for(master, &mut output, ALT_ENTER, Duration::from_secs(10));
    assert!(started, "TUI never entered the alternate screen within 10s");

    // SAFETY: master is a valid fd; writing the key bytes.
    unsafe {
        libc::write(master, keys.as_ptr() as *const libc::c_void, keys.len());
    }

    // Drain until the child closes the PTY (read hits EOF/EIO) or we time out.
    drain(master, &mut output, Duration::from_secs(10));
    unsafe { libc::close(master) };

    let mut status: libc::c_int = 0;
    unsafe { libc::waitpid(pid, &mut status, 0) };
    let exit_code = if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        -1
    };

    Session { output, exit_code }
}

/// Read from `fd` into `buf` until `needle` appears or the deadline passes.
fn wait_for(fd: libc::c_int, buf: &mut Vec<u8>, needle: &[u8], timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !poll_readable(fd, 200) {
            continue;
        }
        if read_chunk(fd, buf) == 0 {
            break; // EOF
        }
        if contains(buf, needle) {
            return true;
        }
    }
    contains(buf, needle)
}

fn drain(fd: libc::c_int, buf: &mut Vec<u8>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !poll_readable(fd, 200) {
            continue;
        }
        if read_chunk(fd, buf) == 0 {
            break; // child closed the PTY
        }
    }
}

fn poll_readable(fd: libc::c_int, timeout_ms: libc::c_int) -> bool {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: single valid pollfd.
    let n = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    // POLLHUP too: when the child exits it closes the slave, and we must still
    // attempt the read to observe EOF - otherwise drain spins to its timeout.
    n > 0 && (pfd.revents & (libc::POLLIN | libc::POLLHUP)) != 0
}

fn read_chunk(fd: libc::c_int, buf: &mut Vec<u8>) -> usize {
    let mut tmp = [0u8; 4096];
    // SAFETY: reading into a local buffer of the given length.
    let n = unsafe { libc::read(fd, tmp.as_mut_ptr() as *mut libc::c_void, tmp.len()) };
    if n > 0 {
        buf.extend_from_slice(&tmp[..n as usize]);
        n as usize
    } else {
        0 // 0 = EOF, negative (EIO when slave closed) treated the same
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn bin_path() -> String {
    // cargo builds the bin and exposes its path to integration tests.
    let p = std::path::Path::new(env!("CARGO_BIN_EXE_oom-tui"));
    std::str::from_utf8(p.as_os_str().as_bytes())
        .unwrap()
        .to_string()
}

#[test]
fn tui_starts_and_restores_the_terminal_on_quit() {
    let session = run_in_pty(&bin_path(), &["--demo"], b"q");

    assert!(
        contains(&session.output, ALT_ENTER),
        "expected the TUI to enter the alternate screen"
    );
    assert!(
        contains(&session.output, ALT_EXIT),
        "TUI must leave the alternate screen on quit, or it wrecks the terminal"
    );
    assert!(
        contains(&session.output, CURSOR_SHOW),
        "TUI must restore the cursor on quit"
    );
    assert_eq!(session.exit_code, 0, "quit should exit cleanly");
}

#[test]
fn tui_restores_the_terminal_on_esc() {
    // Esc is the other quit path; it must clean up just like `q`.
    let session = run_in_pty(&bin_path(), &["--demo"], b"\x1b");
    assert!(
        contains(&session.output, ALT_EXIT),
        "Esc must restore the screen"
    );
    assert_eq!(session.exit_code, 0);
}
