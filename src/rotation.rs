// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-26

//! Per-agent lineage rotation for MS2.
//!
//! (Module documentation is expanded in Task 15; this file starts with the
//! [`Lineage`] newtype — a declared, never-inferred model-family label.)

use std::borrow::Cow;
use std::fmt;

/// A declared model-family lineage label (e.g. `"alibaba"`, `"deepseek"`).
///
/// A `Lineage` is a **declared** identity — never inferred from a model name or
/// response. It is the primary diversity key for rotation: two live mages never
/// share a lineage. Backed by `Cow<'static, str>` so a `&'static str` literal is
/// zero-alloc (`Borrowed`) while a runtime `String` is `Owned`.
///
/// # Normalization (R3.2)
///
/// All constructors **trim** leading/trailing whitespace. Construction is
/// infallible: an empty result (`""`, or all-whitespace) is a valid `Lineage`
/// *value*; it is rejected as invalid input later, at `MagiBuilder::build()`.
///
/// ```
/// use magi_core::rotation::Lineage;
/// assert_eq!(Lineage::new(" alibaba ").as_str(), "alibaba");
/// assert_eq!(Lineage::new("deepseek"), Lineage::from("deepseek"));
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Lineage(Cow<'static, str>);

impl Lineage {
    /// Creates a `Lineage`, trimming surrounding whitespace (R3.2).
    ///
    /// Accepts anything convertible into `Cow<'static, str>` — a `&'static str`
    /// stays `Borrowed` (zero-alloc), a `String` is `Owned`.
    pub fn new(s: impl Into<Cow<'static, str>>) -> Self {
        Self(trim_cow(s.into()))
    }

    /// Returns the lineage label as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Trims a `Cow<'static, str>` while preserving its variant: a `Borrowed`
/// `&'static str` stays borrowed (a trimmed `&'static str` is still `'static`),
/// and an already-trimmed `Owned` string is not re-allocated.
fn trim_cow(c: Cow<'static, str>) -> Cow<'static, str> {
    match c {
        Cow::Borrowed(s) => Cow::Borrowed(s.trim()),
        Cow::Owned(s) => {
            let trimmed = s.trim();
            if trimmed.len() == s.len() {
                Cow::Owned(s)
            } else {
                Cow::Owned(trimmed.to_owned())
            }
        }
    }
}

impl From<&'static str> for Lineage {
    fn from(s: &'static str) -> Self {
        Self(Cow::Borrowed(s.trim()))
    }
}

impl From<String> for Lineage {
    fn from(s: String) -> Self {
        Self(trim_cow(Cow::Owned(s)))
    }
}

impl fmt::Display for Lineage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lineage_equality_and_display() {
        assert_eq!(Lineage::new("alibaba"), Lineage::from("alibaba"));
        assert_ne!(Lineage::new("alibaba"), Lineage::new("moonshot"));
        assert_eq!(format!("{}", Lineage::new("glm")), "glm");
        assert_eq!(Lineage::new("glm").as_str(), "glm");
    }

    #[test]
    fn test_lineage_from_static_is_borrowed_zero_alloc() {
        // A &'static str constructs a Borrowed Cow (no heap alloc).
        let l = Lineage::from("alibaba");
        assert!(matches!(l.0, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn test_lineage_from_owned_string_is_owned() {
        let s = String::from("run-time");
        assert!(matches!(Lineage::from(s).0, std::borrow::Cow::Owned(_)));
    }

    #[test]
    fn test_lineage_ord_for_btree_keys() {
        use std::collections::BTreeSet;
        let mut s = BTreeSet::new();
        s.insert(Lineage::new("b"));
        s.insert(Lineage::new("a"));
        assert_eq!(s.iter().next().unwrap().as_str(), "a"); // Ord works
    }

    #[test]
    fn test_lineage_new_trims_whitespace() {
        // R3.2 — constructors normalize.
        assert_eq!(Lineage::new(" alibaba ").as_str(), "alibaba");
        assert_eq!(Lineage::from("  glm\t").as_str(), "glm");
        assert_eq!(
            Lineage::from(String::from("\n moonshot ")).as_str(),
            "moonshot"
        );
        // A trimmed &'static str stays Borrowed (zero-alloc preserved).
        assert!(matches!(
            Lineage::new(" alibaba ").0,
            std::borrow::Cow::Borrowed(_)
        ));
        // Empty / all-whitespace trims to "" — a VALID Lineage value here; rejected at build (Task 7).
        assert_eq!(Lineage::new("   ").as_str(), "");
    }
}
