// TARGET: providers/subject.rs
//! A test module reached through an intervening attribute and a blank line. Everything inside it
//! is a rule violation on purpose: if the skip fails, this file is rejected.

pub fn clean() -> &'static str {
    "ok"
}

#[cfg(test)]

#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    #[test]
    fn interpolates_freely() {
        let e = std::io::Error::other("boom");
        assert!(format!("{e}").contains("boom"));
        assert!(!e.to_string().is_empty());
    }
}
