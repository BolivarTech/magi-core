// TARGET: providers/claude_cli.rs
//! A subprocess provider as it actually is: it spawns a program, so the errors it composes cannot
//! carry a URL, and interpolating them is safe. This file would fail rule 1 if it were scanned.

pub fn spawn_failure(e: &std::io::Error) -> String {
    format!("failed to spawn claude process: {e}")
}
