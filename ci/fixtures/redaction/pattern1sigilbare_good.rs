// TARGET: providers/subject.rs
//! The same log line built from a message the caller already redacted.

pub fn log_failure(redacted: &str) {
    tracing::warn!(detail = redacted, "request failed");
}
