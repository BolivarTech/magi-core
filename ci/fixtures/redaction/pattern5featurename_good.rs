// TARGET: providers/subject.rs
//! The same feature-gated production module, composing from already-redacted text.

#[cfg(feature = "a-test")]
mod gated {
    pub fn described(redacted: &str) -> String {
        format!("failed: {redacted}")
    }
}
