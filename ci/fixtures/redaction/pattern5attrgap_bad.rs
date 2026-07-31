// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
//! An attribute between the cfg and the `mod` must not cancel the skip — but the leak below is
//! in PRODUCTION code, so it must still be caught.

pub fn leaky(e: &dyn std::error::Error) -> String {
    format!("failed: {e}")
}

#[cfg(test)]
#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    #[test]
    fn t() {}
}
