// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! Fixture manifest: integrity offline, currency only online.
//!
//! `smoke/fixtures/manifest.toml` declares the fixture corpus the harness
//! replays in later stages. [`Manifest::verify`] crosses it against the LIVE
//! scenario list in BOTH directions: every declared fixture must name a
//! scenario that still exists, AND every file physically present in the
//! fixture directory must be declared by someone — a one-way check would let
//! an unlisted file on disk pass as verified, invisible to the corpus it is
//! silently part of.
//!
//! **INTEGRITY is not CURRENCY.** The sha256 in an entry proves the file on
//! disk is the one this project recorded; it does NOT prove the backend
//! still answers that way. Only re-running the scenario against a live
//! backend proves currency — that is exactly what `currency = "sin-verificar:
//! <reason>"` admits, and what `currency = "verificada-por: <id>"` claims.
//!
//! In E1 the manifest ships with zero `[[fixture]]` entries: no E1 scenario
//! replays a fixture (see `smoke/fixtures/manifest.toml`), so both directions
//! of the cross are satisfied vacuously. That is not a placeholder — a
//! `Manifest::load` that refused to run without a populated file could not
//! run in its own milestone. The data arrives with `F0` of MS1, copied and
//! declared by `smoke/sync-fixtures.sh` (R23).

use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

/// Recognized `sin-verificar: <reason>` reasons. Widening this list on a
/// fixture's behalf must be a deliberate edit here, not a typo that quietly
/// starts being accepted as a currency justification.
const KNOWN_REASONS: [&str; 3] = ["cold-start", "no-reproducible", "deprecado"];

/// Prefix for a currency line whose backend answer has NOT been re-checked.
/// The text after it is the reason, matched against [`KNOWN_REASONS`].
const SIN_VERIFICAR_PREFIX: &str = "sin-verificar: ";

/// Prefix for a currency line whose backend answer WAS re-checked. The text
/// after it identifies what did the checking (e.g. a scenario id); this
/// implementation only requires the prefix itself to be present — the id is
/// for a human reading the manifest, not something this module validates.
const VERIFICADA_POR_PREFIX: &str = "verificada-por: ";

/// The manifest's own filename inside the fixture directory. Exempted from
/// the disk->manifest half of [`Manifest::verify`]: the manifest does not
/// declare itself as a fixture.
const MANIFEST_FILENAME: &str = "manifest.toml";

/// Filenames the disk->manifest direction of [`Manifest::verify`] does not
/// expect a `[[fixture]]` entry for, alongside [`MANIFEST_FILENAME`].
const NON_FIXTURE_FILES: [&str; 2] = [MANIFEST_FILENAME, "README.md"];

/// The fixture corpus, declared. Parsed from `smoke/fixtures/manifest.toml`.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    #[serde(default, rename = "fixture")]
    pub fixtures: Vec<FixtureEntry>,
}

/// One `[[fixture]]` entry: which scenario replays it, where its bytes live,
/// what those bytes must hash to, and whether the BACKEND ANSWER it captured
/// (not its bytes — see the module doc) is still believed current.
#[derive(Debug, Deserialize)]
pub struct FixtureEntry {
    /// The scenario that consumes it. Crossed against the LIVE scenario list
    /// in BOTH directions, so a fixture nobody uses is as detectable as a
    /// scenario whose fixture is missing.
    pub scenario: String,
    /// Path relative to the fixture directory (`smoke/fixtures/`).
    pub path: String,
    /// Lowercase hex sha256 of the file's bytes at `path`.
    pub sha256: String,
    /// `verificada-por: <id>` or `sin-verificar: <reason>`; an unrecognized
    /// form makes the harness DECLINE to run rather than assume the corpus is
    /// current.
    pub currency: String,
}

/// What the bidirectional verification found. Empty vectors mean a clean
/// corpus — and saying so is the point, the same way an empty
/// `extraction_failures` certifies rather than disappears.
#[derive(Debug, Default)]
pub struct FixtureAudit {
    /// Declared in the manifest, missing on disk.
    pub missing: Vec<String>,
    /// On disk, declared by nobody — the half a one-directional check would
    /// skip.
    pub orphans: Vec<String>,
    /// Present and declared, but the hash or the currency reason does not
    /// hold.
    pub corrupt: Vec<String>,
    /// Total `[[fixture]]` entries the manifest declared.
    pub total: usize,
    /// Entries whose currency is `sin-verificar:` — recorded but not
    /// re-checked against a live backend. Feeds the 30% warning of Task 12.
    pub unverified: usize,
}

impl FixtureAudit {
    /// `true` when nothing was found. The preflight fails on `false`.
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.orphans.is_empty() && self.corrupt.is_empty()
    }
}

impl Manifest {
    /// Parses `text` as a `manifest.toml` document. An empty string is a
    /// valid, empty manifest — see the module doc for why E1 relies on that.
    pub fn from_str(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("invalid fixture manifest: {e}"))
    }

    /// Reads `manifest.toml` from `dir`. A missing file is an EMPTY manifest,
    /// not an error: E1 ships no fixtures, and refusing to start without a
    /// file would make the harness unable to run in its own milestone.
    pub fn load(dir: &Path) -> Result<Manifest, String> {
        let path = dir.join(MANIFEST_FILENAME);
        match std::fs::read_to_string(&path) {
            Ok(text) => Manifest::from_str(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest {
                fixtures: Vec::new(),
            }),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Crosses the manifest against `dir` and `live_scenarios` in BOTH
    /// directions: every declared fixture must name a live scenario and
    /// match its recorded hash on disk, and every file physically present in
    /// `dir` must be declared by some entry.
    ///
    /// Accumulates findings into the returned [`FixtureAudit`] instead of
    /// stopping at the first problem: with `?` on the first broken entry, a
    /// corpus with three problems would take three separate runs to see.
    ///
    /// # Complexity
    ///
    /// `O(n + m)` where `n` is the number of manifest entries and `m` the
    /// number of directory entries in `dir`: one pass builds the `declared`
    /// set and checks each manifest entry, a second pass checks the
    /// directory against that set.
    pub fn verify(&self, dir: &Path, live_scenarios: &[&str]) -> Result<FixtureAudit, String> {
        let mut audit = FixtureAudit::default();
        let mut declared: BTreeSet<&str> = BTreeSet::new();

        // (1) Manifest -> disk.
        for entry in &self.fixtures {
            declared.insert(entry.path.as_str());

            if !live_scenarios.contains(&entry.scenario.as_str()) {
                // A fixture nobody replays was verified and copied for
                // nothing — cheap to detect here, before spending a read and
                // a hash on a file no scenario will ever open.
                audit.orphans.push(format!(
                    "{}: names scenario {}, which no live scenario implements",
                    entry.path, entry.scenario
                ));
                continue;
            }

            match std::fs::read(dir.join(&entry.path)) {
                Err(e) => audit.missing.push(format!("{}: {e}", entry.path)),
                Ok(bytes) => {
                    if crate::proxy::sha256_hex(&bytes) != entry.sha256 {
                        audit.corrupt.push(format!("{}: hash mismatch", entry.path));
                    }
                }
            }

            Self::check_currency(entry, &mut audit);
        }

        // (2) Disk -> manifest. The half a one-directional check would skip.
        if let Ok(read_dir) = std::fs::read_dir(dir) {
            for dir_entry in read_dir.flatten() {
                let file_name = dir_entry.file_name();
                let Some(name) = file_name.to_str() else {
                    continue;
                };
                if NON_FIXTURE_FILES.contains(&name) {
                    continue;
                }
                if !declared.contains(name) {
                    audit.orphans.push(format!(
                        "{name}: on disk and declared by nobody — copied for nothing, and its \
                         currency is unknowable"
                    ));
                }
            }
        }

        audit.total = self.fixtures.len();
        Ok(audit)
    }

    /// Validates one entry's `currency` line, appending to `audit.corrupt`
    /// (and counting `audit.unverified`) as needed.
    ///
    /// **Extends the brief's reference implementation**: it only validated
    /// the `sin-verificar:` branch. `FixtureEntry::currency`'s own contract
    /// says a form that is neither `verificada-por:` nor `sin-verificar:`
    /// must make the harness decline to run — silently accepting it would be
    /// exactly the "assume the corpus is current" failure that doc comment
    /// exists to name.
    fn check_currency(entry: &FixtureEntry, audit: &mut FixtureAudit) {
        if let Some(reason) = entry.currency.strip_prefix(SIN_VERIFICAR_PREFIX) {
            if !KNOWN_REASONS.contains(&reason.trim()) {
                audit.corrupt.push(format!(
                    "{}: unrecognized currency reason '{}'. Accepted: {KNOWN_REASONS:?}. \
                     Widening the list must be deliberate.",
                    entry.path,
                    reason.trim()
                ));
            }
            audit.unverified += 1;
        } else if entry.currency.strip_prefix(VERIFICADA_POR_PREFIX).is_none() {
            audit.corrupt.push(format!(
                "{}: unrecognized currency form '{}'. Expected 'verificada-por: <id>' or \
                 'sin-verificar: <reason>'.",
                entry.path, entry.currency
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::fixture_dir;
    use crate::testkit::tempdir_with;

    #[test]
    fn a_fixture_whose_hash_changed_is_rejected() {
        // The real `fixture_dir()` must stay data-file-free in E1 (see the
        // module doc), so this constructs its own on-disk fixture rather
        // than depending on the shared production directory — see the Task 7
        // report for why the brief's literal `fixture_dir()` call here would
        // contradict `an_empty_manifest_verifies_clean_which_is_exactly_e1`
        // below.
        let dir = tempdir_with(&[("native-E-malformed.json", "{\"done_reason\":\"load\"}")]);
        let m = Manifest::from_str(
            r#"
            [[fixture]]
            scenario = "S9"
            path     = "native-E-malformed.json"
            sha256   = "0000000000000000000000000000000000000000000000000000000000000000"
            currency = "verificada-por: S9b"
        "#,
        )
        .unwrap();
        // `verify` returns `Ok(audit)` with the findings INSIDE — it
        // accumulates instead of stopping at the first failure — so
        // `unwrap_err()` here would panic on an `Ok`.
        let audit = m.verify(dir.path(), &["S9", "S9b"]).unwrap();
        assert!(!audit.is_clean());
        assert!(audit
            .corrupt
            .iter()
            .any(|s| s.contains("native-E-malformed.json")));
    }

    #[test]
    fn an_empty_manifest_verifies_clean_which_is_exactly_e1() {
        // E1 ships an empty manifest on purpose: no E1 scenario replays
        // fixtures. Both directions of the cross must be vacuously
        // satisfied — if an empty manifest errored, the harness could not
        // run in its own milestone.
        let m = Manifest::from_str("").unwrap();
        // `is_ok()` is not enough: `verify` returns `Ok` even when it FOUND
        // problems. What must be asserted is that the audit is CLEAN — and a
        // directory with undeclared files would report them as orphans.
        assert!(m.verify(&fixture_dir(), &["S1", "S2"]).unwrap().is_clean());
        assert!(m.verify(&fixture_dir(), &[]).unwrap().is_clean());
    }

    #[test]
    fn an_orphaned_entry_is_detected() {
        // A fixture no live scenario uses was verified and copied for
        // NOTHING — cheap to detect, invisible if nobody looks.
        let m = Manifest::from_str(
            r#"
            [[fixture]]
            scenario = "S99"
            path     = "native-N1.json"
            sha256   = "0000000000000000000000000000000000000000000000000000000000000000"
            currency = "sin-verificar: no-reproducible"
        "#,
        )
        .unwrap();
        let audit = m.verify(&fixture_dir(), &["S9"]).unwrap();
        assert!(
            audit.orphans.iter().any(|s| s.contains("S99")),
            "the manifest must be crossed in BOTH directions"
        );
    }

    #[test]
    fn a_file_on_disk_that_nobody_declares_is_an_orphan_too() {
        // The other half of the cross: until now every test entered through
        // the manifest, so the disk->manifest direction — the one the
        // implementation was NOT exercising — stayed untested.
        let dir = tempdir_with(&[("undeclared.json", "{}")]);
        let audit = Manifest::from_str("")
            .unwrap()
            .verify(dir.path(), &["S9"])
            .unwrap();
        assert!(audit.orphans.iter().any(|s| s.contains("undeclared.json")));
    }

    #[test]
    fn an_unrecognized_currency_reason_makes_the_harness_decline_to_run() {
        let m = Manifest::from_str(
            r#"
            [[fixture]]
            scenario = "S9"
            path     = "native-N1.json"
            sha256   = "0000000000000000000000000000000000000000000000000000000000000000"
            currency = "sin-verificar: porque si"
        "#,
        )
        .unwrap();
        let audit = m.verify(&fixture_dir(), &["S9"]).unwrap();
        assert!(audit.corrupt.iter().any(|s| s.contains("porque si")));
    }

    #[test]
    fn an_unrecognized_currency_form_is_rejected() {
        // Neither `verificada-por:` nor `sin-verificar:` — the
        // `FixtureEntry::currency` contract says this must make the harness
        // decline to run, not pass silently. The brief's reference
        // implementation only validated the `sin-verificar:` branch; this
        // pins the other half of that contract (see
        // `Manifest::check_currency`'s doc).
        let m = Manifest::from_str(
            r#"
            [[fixture]]
            scenario = "S9"
            path     = "native-N1.json"
            sha256   = "0000000000000000000000000000000000000000000000000000000000000000"
            currency = "yolo"
        "#,
        )
        .unwrap();
        let audit = m.verify(&fixture_dir(), &["S9"]).unwrap();
        assert!(audit
            .corrupt
            .iter()
            .any(|s| s.contains("unrecognized currency form")));
    }

    #[test]
    fn manifest_load_of_a_missing_file_is_an_empty_manifest_not_an_error() {
        // `Manifest::load`'s documented contract: a missing manifest.toml is
        // an EMPTY manifest, not an error. E1 needs this to be true, or the
        // harness could not run in its own milestone the day someone clones
        // the repo before F0 of MS1 adds real fixtures.
        let dir = tempdir_with(&[]);
        let m = Manifest::load(dir.path()).expect("a missing manifest.toml must not error");
        assert!(m.fixtures.is_empty());
    }
}
