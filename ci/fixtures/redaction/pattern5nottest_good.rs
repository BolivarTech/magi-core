// TARGET: providers/subject.rs
//! The same production-only module, composing from text the caller already redacted.

#[cfg(not(test))]
mod only_outside_tests {
    pub fn described(redacted: &str) -> String {
        format!("failed: {redacted}")
    }
}
