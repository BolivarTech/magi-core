// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-09-08

//! Integration tests for `ClaudeCliProvider`, driven against a real child process.
//!
//! Gated on both features: everything here needs the provider (`claude-cli`) and
//! the stub binary plus the environment helpers (`test-utils`). Without the gate
//! the file would fail to compile under default features, because
//! `CARGO_BIN_EXE_stub_cli` only exists when the stub is built.
//!
//! The file exists from the first commit of this milestone and not from the task
//! that fills it, because the per-commit gate names this binary in its filter:
//! `binary(/cli/)` matches binary NAMES, and nextest refuses to parse a filterset
//! whose `binary(...)` matches none -- measured in this tree at exit 94, not the
//! silent zero a missing filter usually gives.
//!
//! Renaming this file breaks that filter. If it is renamed, the filter in
//! `scripts/ms2_commit_gate.sh` moves in the same change.

#![cfg(all(feature = "claude-cli", feature = "test-utils"))]

mod common;

use common::StubCli;
use serial_test::serial;

/// The scaffolding's own test.
///
/// A stub the milestone's central assertion depends on cannot be the one thing
/// nobody verifies: if it silently emitted nothing, the precondition probe would
/// report "this platform absorbed the burst" and the deadlock scenario would
/// excuse itself with a reason that never happened.
///
/// It lives in this file rather than in `common/mod.rs` because a `#[test]` there
/// is compiled into every binary that declares `mod common;` -- it would run once
/// per binary and report under a name that appears more than once.
#[tokio::test]
#[serial]
async fn the_stub_emits_the_burst_it_was_configured_with() {
    use tokio::io::AsyncReadExt;

    const BURST: usize = 64 * 1024;

    let mut stub = StubCli::new()
        .writes_stderr_before_reading_stdin(BURST)
        .exits_mid_write();
    let exe = stub.path().to_path_buf();

    let mut child = tokio::process::Command::new(&exe)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the stub spawns");

    // Drained HERE on purpose, unlike the precondition probe: this test measures
    // what the stub emits, not whether an undrained parent blocks.
    let mut err = child.stderr.take().expect("stderr is piped");
    let mut sink = Vec::new();
    err.read_to_end(&mut sink).await.expect("draining stderr");

    let status = child.wait().await.expect("reaping the stub");

    assert_eq!(
        sink.len(),
        BURST,
        "the stub must emit exactly the burst it was configured with"
    );
    assert_eq!(
        status.code(),
        Some(common::STUB_MID_WRITE_EXIT),
        "exits_mid_write must use the pinned exit code, which the precedence table \
         splits two of its rows by"
    );
}
