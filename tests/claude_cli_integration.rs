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

use common::{
    PROMPT_BYTES, STUB_MID_WRITE_EXIT, StubCli, WriteProbe, complete_against_stub,
    write_blocks_without_a_drainer,
};
use magi_core::error::ProviderError;
use magi_core::test_support::{big_prompt, cannot_test, captured_envelope_with_stop_reason};
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

/// E-10: a child that floods stderr before reading stdin must not hang the parent.
#[tokio::test]
#[serial] // MANDATORY: `complete_against_stub` mutates CLAUDECODE, which is
// process-global state.
async fn a_verbose_child_does_not_hang_the_parent() {
    // The pipe buffer size is the OS's, so this asserts the PROPERTY, not a number
    // -- but the number is picked against the CEILING rather than against "more
    // than a reasonable buffer", which is intuition. Linux lets a pipe grow to
    // /proc/sys/fs/pipe-max-size, default 1 MiB: exactly that would leave ZERO
    // margin on a maximally expanded pipe. 4 MiB is 4x the ceiling. MEASURED:
    // Windows blocks the writer at 8 KiB, so it carries 512x -- it is where the
    // deadlock reproduces most easily, which makes it the PRIMARY verification
    // platform here, not the secondary one.
    const STDERR_BYTES: usize = 4 * 1024 * 1024;

    // PRECONDITION SEEDED BY ANOTHER PATH, and CHECKED rather than believed. If the
    // write does NOT block without a concurrent drainer, this platform absorbs the
    // whole margin, the deadlock was never exercised, and a green here would be
    // green by omission. THREE branches, not two: with a bool, a write ERROR read as
    // "absorbed" and the test excused itself with a false reason -- a harness
    // failure dressed as a platform limit. `Failed` panics WITHOUT the
    // `CANNOT_TEST:` prefix, so it is not adjudicable and blocks the round.
    match write_blocks_without_a_drainer(STDERR_BYTES, PROMPT_BYTES).await {
        WriteProbe::Blocked => {}
        // No `return`: `cannot_test` returns `!`, so it already diverges.
        WriteProbe::Absorbed => {
            cannot_test("this platform absorbed 4 MiB; the deadlock was not reproduced")
        }
        WriteProbe::Failed(e) => panic!("harness failure while seeding the deadlock: {e}"),
    }

    // EXPLICIT TIMEOUT on the call under test: without it, a regression of the
    // deadlock leaves this test HANGING instead of red, and a gate that hangs is
    // worse than one that fails -- it says nothing and burns the clock until
    // somebody kills it. 30 s is two orders of magnitude over what the call takes
    // when it works, and a fraction of any CI timeout.
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        complete_against_stub(
            // `.stdout()` is explicit: the stub has to exit 0 with a valid envelope
            // after the burst, or the `is_ok()` below cannot hold. The burst is the
            // obstacle; the happy ending is what proves it was cleared.
            StubCli::new()
                .writes_stderr_before_reading_stdin(STDERR_BYTES)
                .stdout(captured_envelope_with_stop_reason("end_turn").as_str()),
            // ONE source for the size, shared with the probe above.
            &big_prompt(PROMPT_BYTES),
        ),
    )
    .await
    // Two DIFFERENT failures, hence two assertions: the timeout says "the deadlock
    // is back", the `is_ok()` says "the call failed for something else". Collapsing
    // them would make a deadlock regression read as an ordinary provider error.
    .expect("the deadlock is back: complete() did not return within 30 s");
    assert!(
        outcome.is_ok(),
        "the call must complete instead of deadlocking"
    );
}

/// Case (3) of the precedence table: the child died, so the child's exit is the
/// diagnosis -- not the broken pipe the parent noticed.
#[tokio::test]
#[serial] // MANDATORY: `complete_against_stub` mutates CLAUDECODE.
async fn a_child_that_dies_mid_write_reports_the_process_not_the_broken_pipe() {
    let err = complete_against_stub(StubCli::new().exits_mid_write(), &big_prompt(PROMPT_BYTES))
        .await
        .unwrap_err();
    // THE EXIT CODE, not just the variant. `matches!(Process { .. })` passes for row
    // (2b) too -- which produces `exit_code: None` -- so a bare variant assertion
    // CANNOT FAIL if the precedence inverts, and inverting it is this task's own
    // mutation operator. The stub exits with a KNOWN code precisely so this
    // assertion has a number to check.
    match err {
        ProviderError::Process { exit_code, .. } => assert_eq!(
            exit_code,
            Some(STUB_MID_WRITE_EXIT),
            "row (3) must carry the child's own exit code; `None` is row (2b)"
        ),
        other => panic!("expected Process with the child's exit code, got {other:?}"),
    }
}

/// Row (2b): the write did not complete, so the result is NEVER `Ok` -- not even on
/// exit 0.
///
/// It is the only branch that could return a verdict the model formed over a
/// TRUNCATED prompt, which is exactly what the sentinel work exists to prevent. The
/// table declared it and no test asserted it, so the milestone's worst consequence
/// was the one thing left unpinned.
#[tokio::test]
#[serial] // MANDATORY: `complete_against_stub` mutates CLAUDECODE.
async fn a_failed_prompt_write_is_never_ok_even_on_exit_zero() {
    // The child reads a little, closes stdin, writes a VALID envelope and exits 0.
    // Everything about the process says success; only the write says otherwise.
    let stub = StubCli::new()
        .read_limit(4 * 1024)
        .stdout(captured_envelope_with_stop_reason("end_turn").as_str())
        .exit_code(0);

    let result = complete_against_stub(stub, &big_prompt(PROMPT_BYTES)).await;

    match result {
        Ok(_) => panic!("a truncated prompt must never produce Ok, whatever the exit code"),
        Err(ProviderError::Process {
            exit_code, stderr, ..
        }) => {
            // `None` and not `Some(0)`: the PROCESS succeeded and the WRITE failed, so
            // a `Some(0)` would be a failure variant declaring success.
            assert_eq!(
                exit_code, None,
                "the process did not fail; the write did, so there is no exit code to report"
            );
            assert!(
                stderr.starts_with("prompt write did not complete: "),
                "the field must say what actually failed: {stderr}"
            );
        }
        Err(other) => panic!("expected Process, got {other:?}"),
    }
}

/// The `Flush` half of the write failure: the bytes WERE delivered and only the
/// pipe's teardown failed, so reporting a truncation would invent one.
///
/// With a bare `io::Error` this branch was reported as "prompt write did not
/// complete" while `write_all` had handed over the whole prompt -- the same lie row
/// (2b) exists to prevent, committed inside the mechanism that prevents it.
#[tokio::test]
#[serial] // MANDATORY: `complete_against_stub` mutates CLAUDECODE.
async fn a_teardown_failure_is_not_reported_as_a_truncated_prompt() {
    // The read limit is ABOVE the prompt, so the child takes the whole thing and
    // then closes: the write completes and only the shutdown can fail. Below the
    // prompt it would seed the other branch -- one knob, two branches, by argument.
    let stub = StubCli::new()
        .read_limit(PROMPT_BYTES * 2)
        .stdout(captured_envelope_with_stop_reason("end_turn").as_str())
        .exit_code(0);

    let result = complete_against_stub(stub, &big_prompt(PROMPT_BYTES)).await;

    // The property asserted is the NEGATIVE one, and deliberately so: whether the
    // teardown fails at all is the operating system's call, so demanding that it
    // does would make this test depend on something the product does not control.
    // What the product controls is that a delivered prompt is never reported as a
    // truncated one.
    if let Err(ProviderError::Process { stderr, .. }) = &result {
        assert!(
            !stderr.starts_with("prompt write did not complete: "),
            "the prompt WAS delivered; a teardown failure must not claim otherwise: {stderr}"
        );
    }
    assert!(
        result.is_ok(),
        "a delivered prompt and a clean exit must complete: {result:?}"
    );
}
