// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-09-08

//! A stand-in for the `claude` CLI, driven entirely by a `stub.json` that sits
//! next to this executable.
//!
//! # Why a real binary and not a mock
//!
//! The defect it exists to reproduce is a **deadlock between two processes**: the
//! parent writes a large prompt to the child's stdin while the child fills its own
//! stderr pipe and nobody drains it. Both block. A mock has one process and cannot
//! have that shape at all, so it would certify the one thing that never breaks.
//!
//! # Why `stub.json` and not environment variables
//!
//! Under `edition = "2024"` `std::env::set_var` is `unsafe`, because the
//! environment is process-global and not thread-safe. Configuring the child that
//! way would have put an `unsafe` mutation in the path of every single stub run.
//! A file next to the executable is read by exactly one process and mutates
//! nothing.
//!
//! The file lives beside the executable rather than at a fixed path because the
//! test harness copies this binary into its own temporary directory per instance:
//! nextest runs tests in parallel, and a shared configuration file would let one
//! test rewrite another's behaviour mid-run.
//!
//! # The keys
//!
//! | Key | Meaning |
//! |---|---|
//! | `exit_code` | what to exit with, once everything else is done |
//! | `stdout` | written to stdout at the end |
//! | `stderr` | written to stderr at the end |
//! | `stderr_before_stdin_bytes` | a burst of this many bytes to stderr BEFORE reading stdin -- this is what fills the pipe and blocks an undrained parent |
//! | `exits_mid_write` | exit right after that burst, without reading stdin at all |
//! | `read_limit` | read exactly this many bytes of stdin and stop, letting the process exit close the pipe -- `null` means drain to EOF |
//!
//! `read_limit` is the ONLY way this stub stops draining stdin. The default drains
//! to EOF, and that default is load-bearing: if it did not, every test sending a
//! large prompt would seed a broken pipe without asking for one, and the deadlock
//! test would stop measuring the deadlock.

use std::fs;
use std::io::{Read, Write};
use std::process::ExitCode;

/// The exit code `exits_mid_write` uses.
///
/// Kept in step with `STUB_MID_WRITE_EXIT` in the integration tests' `common`
/// module, which is where the assertion that reads it lives. It is deliberately
/// neither `0` nor `1`: the provider's precedence table splits two of its rows by
/// exit code, so a `0` would turn one case into the other and a `1` collides with
/// the ordinary failure path.
const MID_WRITE_EXIT: u8 = 42;

fn main() -> ExitCode {
    let config = load_config();

    let burst = config
        .get("stderr_before_stdin_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if burst > 0 {
        // BEFORE stdin, and that order is the whole point: it is what fills the
        // child's stderr pipe while the parent is still writing.
        let chunk = vec![b'E'; 8192];
        let mut written = 0u64;
        let mut err = std::io::stderr();
        while written < burst {
            let take = std::cmp::min(chunk.len() as u64, burst - written) as usize;
            if err.write_all(&chunk[..take]).is_err() {
                break;
            }
            written += take as u64;
        }
        let _ = err.flush();
    }

    if config
        .get("exits_mid_write")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return ExitCode::from(MID_WRITE_EXIT);
    }

    match config.get("read_limit").and_then(|v| v.as_u64()) {
        // Read exactly this much and stop. Below the prompt's length the parent's
        // write fails; at or above it the write completes and the teardown is what
        // fails. One knob seeds both, by argument.
        Some(limit) => {
            // What closes the pipe is this process EXITING a few lines below, not
            // anything done to the handle: `std::io::Stdin` does not implement
            // `Drop`, so dropping it closes nothing and would have left this branch
            // behaving exactly like the draining default while reading as though it
            // did something.
            let mut buf = vec![0u8; limit as usize];
            let _ = read_exactly(&mut std::io::stdin(), &mut buf);
        }
        // The default DRAINS. See the module docs: anything else would seed a
        // broken pipe in every test that sends a large prompt.
        None => {
            let mut sink = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut sink);
        }
    }

    if let Some(out) = config.get("stdout").and_then(|v| v.as_str()) {
        let _ = std::io::stdout().write_all(out.as_bytes());
        let _ = std::io::stdout().flush();
    }
    if let Some(err) = config.get("stderr").and_then(|v| v.as_str()) {
        let _ = std::io::stderr().write_all(err.as_bytes());
        let _ = std::io::stderr().flush();
    }

    let code = config
        .get("exit_code")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    ExitCode::from(code as u8)
}

/// Reads `stub.json` from beside this executable.
///
/// A missing or unreadable file yields an empty object, so every key falls back to
/// its default and the stub behaves like a silent, successful `claude`. That is the
/// safe direction: the alternative is a stub that panics during harness setup and
/// reports a test failure for a configuration problem.
fn load_config() -> serde_json::Value {
    let path = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("stub.json")));
    let raw = match path {
        Some(p) => fs::read_to_string(p).unwrap_or_default(),
        None => String::new(),
    };
    serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}))
}

/// Reads until `buf` is full or the stream ends, whichever comes first.
///
/// `read_exact` is the obvious call and the wrong one: it treats a short stream as
/// an error and would make "the parent sent less than the limit" indistinguishable
/// from a real failure, which is a distinction the tests using this depend on.
fn read_exactly<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}
