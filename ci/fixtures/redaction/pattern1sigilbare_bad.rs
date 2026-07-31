// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
//! The terse tracing sigil: the sigil names the field, so there is no `=` to anchor on.

pub fn log_failure(e: &dyn std::error::Error) {
    tracing::warn!(%e, "request failed");
}
