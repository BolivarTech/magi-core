// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-05-24

//! Stable, structured identity for MAGI findings.
//!
//! A finding's identity is `SHA-256(normalized_path:line:category)[..16]` —
//! never its (LLM-generated, run-unstable) title. Port of panóptico's finding
//! id (Python parity: `MAGI@62cf5801` `finding_id.py`, verified by golden
//! vectors in this module's tests).

use crate::schema::Category;
use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};

const FINDING_ID_HEX_LEN: usize = 16;

/// Canonicalize a path for stable identity: `\` → `/`, strip leading `./`,
/// collapse `//`. Pure string transform (OS-separator independent).
pub fn normalize_path(path: &str) -> String {
    let mut p = path.replace('\\', "/");
    while let Some(stripped) = p.strip_prefix("./") {
        p = stripped.to_string();
    }
    while p.contains("//") {
        p = p.replace("//", "/");
    }
    p
}

/// Map an arbitrary string to a known [`Category`], else [`Category::Other`].
/// Normalizes: trim, lowercase, `_`/space → `-` (parity with Python
/// `normalize_category`).
pub fn normalize_category(value: Option<&str>) -> Category {
    let Some(raw) = value else {
        return Category::Other;
    };
    let slug = raw.trim().to_lowercase().replace(['_', ' '], "-");
    serde_json::from_value::<Category>(serde_json::Value::String(slug)).unwrap_or(Category::Other)
}

/// `SHA-256(normalize_path(file):line:category-slug)[..16]` (hex, 16 chars).
/// Title-independent → stable across runs even when the LLM rewords the title.
pub fn generate_finding_id(file: &str, line: u32, category: Category) -> String {
    let cat_slug = serde_json::to_value(category)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "other".to_string());
    let payload = format!("{}:{}:{}", normalize_path(file), line, cat_slug);
    let digest = Sha256::digest(payload.as_bytes());
    let mut hex = String::with_capacity(FINDING_ID_HEX_LEN);
    for byte in digest.iter().take(FINDING_ID_HEX_LEN / 2) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Fail-soft deserializer for `Finding::file`: JSON string → `Some`; else `None`.
pub(crate) fn de_opt_file<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    })
}

/// Fail-soft deserializer for `Finding::line`: positive int or whole float → `Some`;
/// non-positive, bool, non-whole float, string, null, or out-of-u32 → `None`.
pub(crate) fn de_opt_line<'de, D>(d: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                u32::try_from(u).ok().filter(|&x| x > 0)
            } else if let Some(i) = n.as_i64() {
                u32::try_from(i).ok().filter(|&x| x > 0)
            } else if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f > 0.0 && f <= u32::MAX as f64 {
                    Some(f as u32)
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    })
}

/// Fail-soft deserializer for `Finding::category`: JSON string → normalized; else `Other`.
pub(crate) fn de_category<'de, D>(d: D) -> Result<Category, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => normalize_category(Some(&s)),
        _ => Category::Other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Category;

    #[test]
    fn test_normalize_path_canonicalizes_separator_and_prefix() {
        assert_eq!(normalize_path("src\\x.rs"), "src/x.rs");
        assert_eq!(normalize_path("./src/x.rs"), "src/x.rs");
        assert_eq!(normalize_path("src//x.rs"), "src/x.rs");
    }

    #[test]
    fn test_normalize_category_parity_with_python() {
        assert_eq!(
            normalize_category(Some("logic_error")),
            Category::LogicError
        );
        assert_eq!(normalize_category(Some(" Injection ")), Category::Injection);
        assert_eq!(normalize_category(Some("INJECTION")), Category::Injection);
        assert_eq!(normalize_category(Some("nope")), Category::Other);
        assert_eq!(normalize_category(None), Category::Other);
    }

    #[test]
    fn test_generate_finding_id_shape() {
        let a = generate_finding_id("src/x.rs", 42, Category::LogicError);
        assert_eq!(a, generate_finding_id("src/x.rs", 42, Category::LogicError));
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_finding_id_golden_vectors_match_python() {
        // Pinned to Python finding_id.py @ MAGI 62cf5801 (cross-language parity).
        assert_eq!(
            generate_finding_id("src/x.rs", 42, Category::LogicError),
            "7fb2a28931164f30"
        );
        assert_eq!(
            generate_finding_id("./src/x.rs", 42, Category::LogicError),
            "7fb2a28931164f30"
        );
        assert_eq!(
            generate_finding_id("src\\x.rs", 42, Category::LogicError),
            "7fb2a28931164f30"
        );
        assert_eq!(
            generate_finding_id("src/x.rs", 42, Category::Injection),
            "0f8a878b777ce419"
        );
        assert_eq!(
            generate_finding_id("src/main.rs", 1, Category::Other),
            "74f9783a13d7fc23"
        );
    }
}
