// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-09-08

//! Integration tests for `ClaudeCliProvider`.
//!
//! The file exists from the first commit of MS2 and not from the task that fills
//! it, because the milestone's per-commit gate names this binary in its filter:
//! `binary(/cli/)` is matched against binary NAMES, and nextest refuses to parse a
//! filterset whose `binary(...)` matches none -- measured in this tree at exit 94,
//! not the silent zero a missing filter usually gives. Without the file the gate
//! the plan mandates for every commit of this milestone cannot run at all.
//!
//! Renaming this file breaks that filter. If it is renamed, the filter in
//! `scripts/ms2_commit_gate.sh` moves in the same change.
