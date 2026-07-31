// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
//! A feature NAME ending in the token. The module is production code; treating the attribute as a
//! test gate skips it and hides the leak inside.

#[cfg(feature = "a-test")]
mod gated {
    pub fn leaky(e: &dyn std::error::Error) -> String {
        format!("failed: {e}")
    }
}
