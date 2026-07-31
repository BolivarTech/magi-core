// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
//! `not(test)` is PRODUCTION code. Treating it as a test module hides the leak below.

#[cfg(not(test))]
mod only_outside_tests {
    pub fn leaky(e: &dyn std::error::Error) -> String {
        format!("failed: {e}")
    }
}
