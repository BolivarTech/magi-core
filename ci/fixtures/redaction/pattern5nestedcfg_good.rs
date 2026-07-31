// TARGET: providers/subject.rs
//! A test module behind a cfg with a NESTED paren before the token. Everything inside violates a
//! rule on purpose: if the nesting defeats the detection, this file is rejected.

pub fn clean() -> &'static str {
    "ok"
}

#[cfg(all(not(feature = "unlikely"), test))]
mod tests {
    #[test]
    fn interpolates_freely() {
        let e = std::io::Error::other("boom");
        assert!(!format!("{e}").is_empty());
    }
}
