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
//! backend proves currency — that is exactly what `currency = "unverified:
//! <reason>"` admits, and what `currency = "verified-by: <id>"` claims.
//!
//! In E1 the manifest ships with zero `[[fixture]]` entries: no E1 scenario
//! replays a fixture (see `smoke/fixtures/manifest.toml`), so both directions
//! of the cross are satisfied vacuously. That is not a placeholder — a
//! `Manifest::load` that refused to run without a populated file could not
//! run in its own milestone. The data arrives with the scenarios that replay
//! it, copied and declared by `smoke/sync-fixtures.sh` (R23).

use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

/// Recognized `unverified: <reason>` reasons. Widening this list on a
/// fixture's behalf must be a deliberate edit here, not a typo that quietly
/// starts being accepted as a currency justification.
const KNOWN_REASONS: [&str; 3] = ["cold-start", "no-reproducible", "deprecated"];

/// Prefix for a currency line whose backend answer has NOT been re-checked.
/// The text after it is the reason, matched against [`KNOWN_REASONS`].
const UNVERIFIED_PREFIX: &str = "unverified: ";

/// Prefix for a currency line whose backend answer WAS re-checked. The text
/// after it identifies what did the checking: a scenario id, crossed
/// against the LIVE scenario list by [`Manifest::check_currency`], so a claim
/// cannot outlive the scenario that used to make it.
const VERIFIED_BY_PREFIX: &str = "verified-by: ";

/// The manifest's own filename inside the fixture directory. Exempted from
/// the disk->manifest half of [`Manifest::verify`]: the manifest does not
/// declare itself as a fixture.
const MANIFEST_FILENAME: &str = "manifest.toml";

/// Filenames the disk->manifest direction of [`Manifest::verify`] does not
/// expect a `[[fixture]]` entry for, alongside [`MANIFEST_FILENAME`].
///
/// `wanted.txt` is the sync script's INPUT — the tracked list of what the
/// corpus should contain — and it lives beside the fixtures it describes. It is
/// declared by no `[[fixture]]` entry and never will be, so without this entry
/// the first legitimate sync turns the audit's orphan check into a permanent
/// refusal to start. The pairing is pinned by a test that reads the name out of
/// `sync-fixtures.sh` rather than repeating it.
const NON_FIXTURE_FILES: [&str; 3] = [MANIFEST_FILENAME, "README.md", "wanted.txt"];

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
    /// `verified-by: <id>` or `unverified: <reason>`; an unrecognized
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
    /// hold — or the corpus could not be fully READ, at either level: the
    /// fixture directory itself (see `Manifest::verify`'s direction-2 comment)
    /// or one entry inside it (see
    /// [`Manifest::cross_disk_against_manifest`]). What could not be read
    /// belongs here rather than nowhere: an audit that silently omits it is
    /// clean about a corpus it did not see.
    pub corrupt: Vec<String>,
    /// Total `[[fixture]]` entries the manifest declared.
    pub total: usize,
    /// Entries whose currency is `unverified:` — recorded but not
    /// re-checked against a live backend. Feeds the 30% warning.
    pub unverified: usize,
}

impl FixtureAudit {
    /// `true` when nothing was found. The preflight fails on `false`.
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.orphans.is_empty() && self.corrupt.is_empty()
    }

    /// The two counts that outlive the preflight, for the end-of-run report and
    /// the certificate.
    ///
    /// The audit's own vectors do not travel: they exist to FAIL the preflight
    /// and are already rendered into that failure's message. What the rest of
    /// the run needs is the pair R23 asks to be surfaced.
    pub fn summary(&self) -> FixtureSummary {
        FixtureSummary {
            total: self.total,
            unverified: self.unverified,
        }
    }
}

/// Above this percentage of `unverified:` entries, the certificate carries a
/// visible warning (R23). **Strictly above**: at exactly the threshold nothing
/// fires, which is what [`FixtureSummary::unverified_warning`]'s own test pins
/// from both sides.
const UNVERIFIED_WARNING_PERCENT: usize = 30;

/// What the fixture audit found, in the two numbers that have to leave the
/// preflight alive.
///
/// # Why this exists at all
///
/// [`FixtureAudit::unverified`] was computed and dropped: nothing outside this
/// module ever read it, so neither R23's end-of-run count nor its 30 % warning
/// existed. That is precisely the failure R23 was written to prevent — *"a
/// field nobody counts fills up with `unverified` without the figure ever
/// appearing"* — reproduced in the code meant to implement it. Latent while
/// the manifest is empty; live from the first fixture anyone adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FixtureSummary {
    /// Every `[[fixture]]` entry the manifest declared.
    pub total: usize,
    /// Those whose currency is `unverified:` — recorded, but never re-checked
    /// against a live backend.
    pub unverified: usize,
}

impl FixtureSummary {
    /// The end-of-run count: how many fixtures of each class this run verified.
    ///
    /// R23 asks for it explicitly, and for the same reason the warning exists:
    /// a distinction that is never counted lives in the schema and not in
    /// practice.
    pub fn report_line(&self) -> String {
        format!(
            "fixtures: {} declared, {} verified by a live scenario, {} unverified",
            self.total,
            self.total.saturating_sub(self.unverified),
            self.unverified
        )
    }

    /// The certificate's warning, or `None` when the corpus is within R23's
    /// threshold.
    ///
    /// # Why the CERTIFICATE and not stdout
    ///
    /// R23 is explicit: *"a count that only lives in stdout is lost; one in the
    /// certificate stays in git history"*. It is advisory and blocks nothing —
    /// some fixtures cannot have their currency verified at all — but it
    /// appears where somebody decides a release.
    pub fn unverified_warning(&self) -> Option<String> {
        // Cross-multiplied rather than dividing: integer division would floor
        // the share and let a corpus sit just above the threshold reporting
        // itself just below it. No floats either — a percentage compared with
        // `>` is the one place where a rounding rule nobody chose would decide
        // whether a release ships with a warning.
        if self.total == 0 || self.unverified * 100 <= self.total * UNVERIFIED_WARNING_PERCENT {
            return None;
        }
        Some(format!(
            "WARNING: {} of {} fixtures are unverified (over {UNVERIFIED_WARNING_PERCENT}%). \
             Their bytes are the ones recorded, but nothing re-checked that the backend still \
             answers that way. This does not block the release; it is here because a count \
             that lives only in a run's output is lost by the time anyone decides.",
            self.unverified, self.total
        ))
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
    /// `live_scenarios` is crossed TWICE per entry, against two different
    /// fields: the `scenario` that replays the fixture, and the id a
    /// `verified-by:` currency line names as having re-checked it. The second
    /// cross used not to happen, so a currency claim survived the scenario that
    /// made it (see [`check_currency`](Manifest::check_currency)).
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
    pub fn verify(&self, dir: &Path, live_scenarios: &[&str]) -> FixtureAudit {
        let mut audit = FixtureAudit::default();
        let mut declared: BTreeSet<&str> = BTreeSet::new();

        // (1) Manifest -> disk.
        for entry in &self.fixtures {
            declared.insert(entry.path.as_str());

            // BEFORE the read, and that order is the point: a path carrying a
            // separator is joined onto `dir` and read from wherever it lands,
            // so `../` walks straight out of the corpus. Refusing afterwards
            // would mean the escape had already happened.
            //
            // This is the manifest side of the flatness the disk walk enforces
            // (see `cross_disk_against_manifest`), and half the check was worse
            // than none: the disk walk reads ONE level, so a nested or escaping
            // entry is hashed by direction (1) and then never crossed by
            // direction (2) — the audit reports clean over a file that is not
            // in the scope it speaks for.
            if let Some(bad) = Self::offending_component(&entry.path) {
                audit.corrupt.push(format!(
                    "{}: the fixture corpus is FLAT, and this path is not a member of it \
                     ({bad}). It would be read from outside the one directory level this \
                     audit crosses, so the audit could not be clean.",
                    entry.path
                ));
                continue;
            }

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

            Self::check_currency(entry, live_scenarios, &mut audit);
        }

        // (2) Disk -> manifest. The half a one-directional check would skip.
        //
        // An unreadable `dir` — missing, wrong path, sparse checkout — is a
        // FINDING, not a reason to skip this direction in silence: with an
        // empty manifest (E1's own state) direction (1) above has nothing to
        // report either, so swallowing the `Err` here would make `verify`
        // return a CLEAN audit over a corpus that is not there at all —
        // exactly the "guard reports success while guarding nothing" defect
        // this module exists to catch. A directory that EXISTS and is empty
        // is E1's legitimate state and takes the `Ok` arm below, untouched.
        match std::fs::read_dir(dir) {
            Err(e) => {
                audit.corrupt.push(format!(
                    "{}: fixture directory could not be read: {e}",
                    dir.display()
                ));
            }
            Ok(read_dir) => Self::cross_disk_against_manifest(read_dir, dir, &declared, &mut audit),
        }

        audit.total = self.fixtures.len();
        audit
    }

    /// Why a declared path is not a plain filename, or `None` when it is.
    ///
    /// # Both separators, on every platform
    ///
    /// `/` and `\` are BOTH rejected everywhere, rather than deferring to
    /// `std::path`'s per-platform rules. A manifest is copied between machines,
    /// and a rule that accepts `a\b.json` on Linux — where it is one filename —
    /// and splits it on Windows would make the same corpus audit two different
    /// ways. The corpus is flat, so neither character has any business in an
    /// entry regardless of who reads it.
    ///
    /// `..` is rejected as a WHOLE component and not as a substring: a fixture
    /// legitimately named `native-E..malformed.json` is a filename, and refusing
    /// it would be a guard inventing a finding. With separators already gone the
    /// only way `..` can mean the parent directory is by being the entire path.
    ///
    /// An EMPTY path is refused too. It joins to `dir` itself, so the read would
    /// succeed or fail on the directory rather than on a fixture.
    ///
    /// # Parameters
    ///
    /// * `path` — the `path` field of a `[[fixture]]` entry.
    ///
    /// # Complexity
    ///
    /// `O(n)` in the length of `path`.
    fn offending_component(path: &str) -> Option<&'static str> {
        if path.is_empty() {
            return Some("it is empty");
        }
        if path.contains('/') || path.contains('\\') {
            return Some("it contains a path separator");
        }
        if path == ".." || path == "." {
            return Some("it names a directory rather than a file");
        }
        None
    }

    /// Direction (2) of the cross, over an ITERATOR of entries rather than
    /// over a path.
    ///
    /// # Why it takes the iterator instead of reading the directory itself
    ///
    /// So that the failure branch can be reached from a test. There is no
    /// portable way to make a real `ReadDir` yield an `Err` on demand, and a
    /// branch that cannot be reached is a branch nothing pins: reverting it to
    /// a silent skip would leave the suite green. `std::fs::ReadDir` already
    /// **is** an `Iterator<Item = io::Result<DirEntry>>`, so the production
    /// call site hands over its own iterator unchanged and no seam is
    /// simulated.
    ///
    /// # Nothing is skipped in silence, at EITHER level
    ///
    /// The directory-level read is guarded one level up; this is the ENTRY
    /// level, which used to flatten. An entry that cannot be read, or
    /// whose name is not valid UTF-8 and therefore cannot be compared with a
    /// manifest path at all, is a finding — never a row that quietly does not
    /// appear. Dropping either one lets `verify` return a CLEAN audit over a
    /// corpus it did not fully see, which is the same "reports success while
    /// guarding nothing" defect the directory-level fix closed one level up.
    ///
    /// # The corpus is FLAT, and that is enforced rather than assumed
    ///
    /// This walk reads ONE directory level. A `[[fixture]]` path containing a
    /// separator would therefore be hashed correctly by direction (1) and never
    /// crossed by direction (2), and any undeclared file beside it would be
    /// invisible — half the stated scope, reported on as though it were all of
    /// it. A subdirectory is consequently a finding of its own, and NOT an
    /// orphan: an orphan says a file was copied for nothing, while this says a
    /// region of the corpus was not looked at, and the two send the reader to
    /// different places.
    ///
    /// A file type that cannot be read takes the same branch as a directory,
    /// deliberately: "it might be a directory" is not enough to claim the walk
    /// saw everything, and the safe direction here is the one that refuses.
    ///
    /// # Parameters
    ///
    /// * `entries` — the directory's entries, each possibly a read failure.
    /// * `dir` — the fixture directory, for naming what could not be read.
    /// * `declared` — every path the manifest declared, from direction (1).
    /// * `audit` — accumulator; findings are appended, nothing is returned.
    ///
    /// # Complexity
    ///
    /// `O(m log n)` for `m` entries against `n` declared paths: one `BTreeSet`
    /// lookup per entry.
    fn cross_disk_against_manifest<I>(
        entries: I,
        dir: &Path,
        declared: &BTreeSet<&str>,
        audit: &mut FixtureAudit,
    ) where
        I: IntoIterator<Item = std::io::Result<std::fs::DirEntry>>,
    {
        for entry in entries {
            let dir_entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    audit.corrupt.push(format!(
                        "{}: an entry of the fixture directory could not be read: {e}. The \
                         corpus was NOT fully seen, so this audit cannot be clean.",
                        dir.display()
                    ));
                    continue;
                }
            };
            let file_name = dir_entry.file_name();
            let Some(name) = file_name.to_str() else {
                audit.corrupt.push(format!(
                    "{}: entry named {:?} is not valid UTF-8, so it cannot be compared with \
                     any manifest path — whether it is declared is unknowable, not clean.",
                    dir.display(),
                    file_name.to_string_lossy()
                ));
                continue;
            };
            if NON_FIXTURE_FILES.contains(&name) {
                continue;
            }
            // The corpus is FLAT, and this is where that is enforced. This walk
            // reads one directory level, so anything below a subdirectory is
            // never compared with the manifest at all — a check covering half
            // its stated scope while reporting on the whole of it. Reporting
            // the directory as an orphan FILE would be the wrong finding: it
            // says something was copied for nothing, when what happened is that
            // a region of the corpus was not looked at.
            //
            // Flatness rather than recursion because no fixture nests, and a
            // recursive walk would have to build relative paths and reconcile
            // two separator conventions to compare them with manifest entries
            // that have never carried either. If a corpus ever needs nesting,
            // this refusal is where it announces itself.
            if dir_entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
                audit.corrupt.push(format!(
                    "{}: {name} is a directory, and this walk does not recurse — the fixture \
                     corpus is FLAT. Nothing inside it was compared with the manifest, so \
                     this audit cannot be clean.",
                    dir.display()
                ));
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

    /// Validates one entry's `currency` line, appending to `audit.corrupt`
    /// (and counting `audit.unverified`) as needed.
    ///
    /// **Extends the brief's reference implementation**: it only validated
    /// the `unverified:` branch. `FixtureEntry::currency`'s own contract
    /// says a form that is neither `verified-by:` nor `unverified:`
    /// must make the harness decline to run — silently accepting it would be
    /// exactly the "assume the corpus is current" failure that doc comment
    /// exists to name.
    ///
    /// # BOTH branches are validated, and the second one used not to be
    ///
    /// `verified-by:` was accepted on the strength of its prefix alone, so any
    /// text behind it passed: an empty id, or an id naming a scenario that does
    /// not exist. That is not a cosmetic gap. The claim such a line makes is
    /// that a LIVE scenario re-checks this fixture's answer against a backend,
    /// and once the named scenario is renamed or removed the manifest goes on
    /// making it forever while the preflight reports the corpus clean — an
    /// audit certifying a currency nothing establishes.
    ///
    /// The id is therefore crossed against `live_scenarios`, exactly as
    /// [`verify`](Manifest::verify) already crosses the `scenario` field, and
    /// for the same reason: the machinery existed, this branch simply was not
    /// given the list.
    ///
    /// # Parameters
    ///
    /// * `entry` — the manifest entry whose `currency` line is being read.
    /// * `live_scenarios` — the scenarios this build actually implements.
    /// * `audit` — accumulator; findings are appended, nothing is returned.
    fn check_currency(entry: &FixtureEntry, live_scenarios: &[&str], audit: &mut FixtureAudit) {
        if let Some(reason) = entry.currency.strip_prefix(UNVERIFIED_PREFIX) {
            if !KNOWN_REASONS.contains(&reason.trim()) {
                audit.corrupt.push(format!(
                    "{}: unrecognized currency reason '{}'. Accepted: {KNOWN_REASONS:?}. \
                     Widening the list must be deliberate.",
                    entry.path,
                    reason.trim()
                ));
            }
            audit.unverified += 1;
        } else if let Some(id) = entry.currency.strip_prefix(VERIFIED_BY_PREFIX) {
            let id = id.trim();
            if id.is_empty() {
                audit.corrupt.push(format!(
                    "{}: currency claims 'verified-by' and names nobody. A re-check by an \
                     unnamed party cannot be found, repeated or retired.",
                    entry.path
                ));
            } else if !live_scenarios.contains(&id) {
                audit.corrupt.push(format!(
                    "{}: currency claims to be verified by '{id}', which no live scenario \
                     implements. The claim outlived whatever used to make it, so this \
                     fixture's answer is of unknown currency rather than current.",
                    entry.path
                ));
            }
        } else {
            audit.corrupt.push(format!(
                "{}: unrecognized currency form '{}'. Expected 'verified-by: <id>' or \
                 'unverified: <reason>'.",
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

    /// Lays out a throw-away replica of the repository — `<root>/smoke/` with a
    /// copy of the real `sync-fixtures.sh` in it, and an empty
    /// `<root>/sbtdd/ec-evidence/` for the script's source check — and returns
    /// the replica's root.
    ///
    /// A replica rather than the real checkout because the script WRITES, and a
    /// test that writes into the tree is the very thing the guard below exists
    /// to forbid.
    #[cfg(test)]
    fn sync_script_replica() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "magi-smoke-sync-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("smoke")).expect("replica smoke dir");
        std::fs::create_dir_all(root.join("sbtdd").join("ec-evidence")).expect("replica source");
        std::fs::copy(
            crate::paths::smoke_dir().join("sync-fixtures.sh"),
            root.join("smoke").join("sync-fixtures.sh"),
        )
        .expect("the script under test must be copyable");
        root
    }

    /// Runs the replica's script with `sh`, from `cwd`, and returns its status.
    #[cfg(test)]
    fn run_sync_from(root: &std::path::Path, cwd: &std::path::Path) -> std::process::Output {
        std::process::Command::new("sh")
            .arg(root.join("smoke").join("sync-fixtures.sh"))
            .current_dir(cwd)
            .output()
            .expect(
                "this project's gate is itself a `sh` script, so `sh` is a hard dependency; \
                 skipping here instead would be green by omission",
            )
    }

    #[test]
    fn sync_fixtures_writes_the_same_place_from_any_directory() {
        // Its two paths used to be resolved against DIFFERENT bases: the source
        // against the caller's working directory, the destination against the
        // repository root. Run from the repository root the source pointed
        // outside the repository; run from `smoke/` the destination landed on
        // `smoke/smoke/fixtures` — a directory INSIDE the checkout that nothing
        // is supposed to create. One base makes the script answer the same way
        // wherever it is invoked from, which is the only contract a script with
        // two paths can honestly offer.
        for cwd in ["", "smoke"] {
            let root = sync_script_replica();
            let from = root.join(cwd);
            let out = run_sync_from(&root, &from);
            assert!(
                out.status.success(),
                "running from {from:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                root.join("smoke")
                    .join("fixtures")
                    .join("manifest.toml")
                    .is_file(),
                "running from {from:?} did not regenerate smoke/fixtures/manifest.toml"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn a_sync_that_cannot_finish_leaves_the_tracked_manifest_alone() {
        // The rewrite truncated the tracked manifest to its header BEFORE the
        // first copy, so any failure part-way through — a missing source file,
        // an unreadable one — left every previously-synced fixture declared by
        // nobody. The next audit calls the whole corpus orphaned, and the
        // recovery is a second successful sync that the failure just proved
        // impossible.
        let root = sync_script_replica();
        let fixtures = root.join("smoke").join("fixtures");
        std::fs::create_dir_all(&fixtures).expect("replica fixture dir");
        let manifest = fixtures.join("manifest.toml");
        let before = "# a manifest that already declares a corpus\n";
        std::fs::write(&manifest, before).expect("seed the tracked manifest");
        // Names a file the origin does not have: the copy fails, and `set -e`
        // stops the script right there.
        std::fs::write(
            fixtures.join("wanted.txt"),
            "absent.json S9 unverified: cold-start\n",
        )
        .expect("seed the input list");

        let out = run_sync_from(&root, &root);
        assert!(
            !out.status.success(),
            "a copy that could not happen must not report success"
        );
        assert_eq!(
            std::fs::read_to_string(&manifest).expect("the manifest must still be there"),
            before,
            "the tracked manifest was rewritten by a sync that never finished"
        );
        // The other half of the same fix. Building the manifest aside solves
        // nothing if the aside file survives inside the audited directory: the
        // orphan check would find it and refuse to start, which is the failure
        // that was being moved rather than removed.
        let leftovers: Vec<String> = std::fs::read_dir(&fixtures)
            .expect("the replica fixture dir must be readable")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|n| !NON_FIXTURE_FILES.contains(&n.as_str()))
            .collect();
        assert!(
            leftovers.is_empty(),
            "an aborted sync left {leftovers:?} in the fixture directory, and the orphan audit \
             will report every one of them"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_input_list_the_sync_script_reads_is_exempt_from_the_orphan_audit() {
        // `sync-fixtures.sh` keeps its input list INSIDE the directory the
        // audit walks, so the audit sees a file no `[[fixture]]` entry
        // declares and reports an orphan — a permanent exit 2, arriving the
        // first time somebody declares a fixture.
        //
        // Read out of the script rather than written twice: the two names have
        // to agree, and the only way to be sure they do is to let one of them
        // BE the other. A literal here would pass while the script wrote
        // somewhere else, which is the shape of failure this pins.
        let script = std::fs::read_to_string(crate::paths::smoke_dir().join("sync-fixtures.sh"))
            .expect("sync-fixtures.sh must be readable");
        let line = script
            .lines()
            .find_map(|l| l.trim().strip_prefix("WANTED="))
            .expect("sync-fixtures.sh must still name its input list");
        let name = line
            .trim_matches('"')
            .rsplit(['/', '}'])
            .next()
            .expect("a non-empty path");
        assert!(
            NON_FIXTURE_FILES.contains(&name),
            "{name} lives in the fixture directory and is declared by no [[fixture]] entry, so \
             the audit will call it an orphan and refuse to start"
        );
    }

    #[test]
    fn sync_fixtures_creates_nothing_beside_the_fixture_directory() {
        // The other half, and the one nothing else enforces: `writable_locations`
        // covers the BINARY's write sites, and this script is not the binary. A
        // stray directory here is invisible to that guard and to `git status`
        // alike once somebody ignores it.
        for cwd in ["", "smoke"] {
            let root = sync_script_replica();
            let out = run_sync_from(&root, &root.join(cwd));
            assert!(out.status.success(), "the script must run from {cwd:?}");
            let mut created: Vec<String> = std::fs::read_dir(root.join("smoke"))
                .expect("the replica's smoke directory must be readable")
                .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                .collect();
            created.sort();
            assert_eq!(
                created,
                vec!["fixtures".to_string(), "sync-fixtures.sh".to_string()],
                "run from {cwd:?} the script created something beside smoke/fixtures"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn the_tracked_manifest_is_what_the_script_generates() {
        // The manifest's own first line says it is generated and must not be
        // hand-edited, and the tracked file did not match what the generator
        // writes: its header had been written by hand. A file that claims to be
        // generated while diverging from its generator is the same defect class
        // this module exists to catch one level down — a check that reports on
        // something other than what it looked at. The first regeneration would
        // have silently rewritten prose nobody meant to lose.
        let root = sync_script_replica();
        let out = run_sync_from(&root, &root);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let generated = std::fs::read_to_string(root.join("smoke/fixtures/manifest.toml"))
            .expect("the script must have written a manifest");
        let tracked = std::fs::read_to_string(fixture_dir().join(MANIFEST_FILENAME))
            .expect("the tracked manifest must be readable");
        // Line-wise, so the comparison is not decided by the checkout's line
        // endings: the script writes LF and git may hand back CRLF.
        let generated: Vec<&str> = generated.lines().collect();
        let tracked: Vec<&str> = tracked.lines().collect();
        assert_eq!(
            tracked, generated,
            "the tracked manifest differs from what sync-fixtures.sh generates"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_fixture_whose_hash_changed_is_rejected() {
        // The real `fixture_dir()` must stay data-file-free in E1 (see the
        // module doc), so this constructs its own on-disk fixture rather
        // than depending on the shared production directory: a literal
        // `fixture_dir()` call here would contradict
        // `an_empty_manifest_verifies_clean_which_is_exactly_e1` below.
        let dir = tempdir_with(&[("native-E-malformed.json", "{\"done_reason\":\"load\"}")]);
        let m = Manifest::from_str(
            r#"
            [[fixture]]
            scenario = "S9"
            path     = "native-E-malformed.json"
            sha256   = "0000000000000000000000000000000000000000000000000000000000000000"
            currency = "verified-by: S9b"
        "#,
        )
        .unwrap();
        // `verify` returns `Ok(audit)` with the findings INSIDE — it
        // accumulates instead of stopping at the first failure — so
        // `unwrap_err()` here would panic on an `Ok`.
        let audit = m.verify(dir.path(), &["S9", "S9b"]);
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
        assert!(m.verify(&fixture_dir(), &["S1", "S2"]).is_clean());
        assert!(m.verify(&fixture_dir(), &[]).is_clean());
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
            currency = "unverified: no-reproducible"
        "#,
        )
        .unwrap();
        let audit = m.verify(&fixture_dir(), &["S9"]);
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
        let audit = Manifest::from_str("").unwrap().verify(dir.path(), &["S9"]);
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
            currency = "unverified: just because"
        "#,
        )
        .unwrap();
        let audit = m.verify(&fixture_dir(), &["S9"]);
        assert!(audit.corrupt.iter().any(|s| s.contains("just because")));
    }

    #[test]
    fn an_unrecognized_currency_form_is_rejected() {
        // Neither `verified-by:` nor `unverified:` — the
        // `FixtureEntry::currency` contract says this must make the harness
        // decline to run, not pass silently. The brief's reference
        // implementation only validated the `unverified:` branch; this
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
        let audit = m.verify(&fixture_dir(), &["S9"]);
        assert!(audit
            .corrupt
            .iter()
            .any(|s| s.contains("unrecognized currency form")));
    }

    #[test]
    fn a_verified_by_naming_no_live_scenario_is_rejected() {
        // The `verified-by:` branch was accepted on the strength of its prefix
        // alone, so any text behind it passed — including an id that names
        // nothing. A fixture declaring `verified-by: S9b` keeps asserting that
        // a live scenario re-checks its currency after S9b is renamed or
        // deleted, and the preflight goes on reporting the corpus clean.
        //
        // The machinery to close it already existed: `verify` crosses the
        // `scenario` field against `live_scenarios` for exactly this reason.
        // `check_currency` simply was not given the list.
        let m = Manifest::from_str(
            r#"
            [[fixture]]
            scenario = "S9"
            path     = "native-N1.json"
            sha256   = "0000000000000000000000000000000000000000000000000000000000000000"
            currency = "verified-by: S9b"
        "#,
        )
        .unwrap();
        let audit = m.verify(&fixture_dir(), &["S9"]);
        assert!(
            audit.corrupt.iter().any(|s| s.contains("S9b")),
            "an id naming no live scenario must be a finding, not a prefix that passed: {:?}",
            audit.corrupt
        );
    }

    #[test]
    fn a_verified_by_with_an_empty_id_is_rejected() {
        // The degenerate end of the same hole: `verified-by: ` carries the
        // prefix and nothing else, so it claimed a re-check by nobody.
        let m = Manifest::from_str(
            r#"
            [[fixture]]
            scenario = "S9"
            path     = "native-N1.json"
            sha256   = "0000000000000000000000000000000000000000000000000000000000000000"
            currency = "verified-by:   "
        "#,
        )
        .unwrap();
        let audit = m.verify(&fixture_dir(), &["S9"]);
        assert!(
            audit.corrupt.iter().any(|s| s.contains("names nobody")),
            "an empty id claims a re-check by nobody and must say so: {:?}",
            audit.corrupt
        );
    }

    #[test]
    fn a_verified_by_naming_a_live_scenario_is_accepted() {
        // The other direction, without which the two above are satisfied by a
        // check that rejects every `verified-by:` line there is.
        let m = Manifest::from_str(
            r#"
            [[fixture]]
            scenario = "S9"
            path     = "native-N1.json"
            sha256   = "0000000000000000000000000000000000000000000000000000000000000000"
            currency = "verified-by: S9b"
        "#,
        )
        .unwrap();
        let audit = m.verify(&fixture_dir(), &["S9", "S9b"]);
        assert!(
            !audit.corrupt.iter().any(|s| s.contains("currency")),
            "an id that DOES name a live scenario is a valid currency claim: {:?}",
            audit.corrupt
        );
    }

    #[test]
    fn a_nested_corpus_is_refused_because_the_walk_does_not_recurse() {
        // The disk->manifest half reads ONE directory level, so anything under
        // a subdirectory is never compared with the manifest at all. That gives
        // a check which covers half its stated scope and says nothing about the
        // other half — the corpus is reported on as though it had been fully
        // seen.
        //
        // Closed by declaring the corpus FLAT and enforcing that, rather than
        // by recursing: no fixture nests today, and a recursive walk would have
        // to build and normalise relative paths across two separator
        // conventions to compare them with a manifest that has never carried
        // one. Enforcing flatness closes exactly the gap, and a corpus that
        // ever needs nesting gets a loud refusal instead of a silent omission.
        let dir = tempdir_with(&[("sub/nested.json", "{}")]);
        let audit = Manifest { fixtures: vec![] }.verify(dir.path(), &["S9"]);
        assert!(
            audit.corrupt.iter().any(|s| s.contains("does not recurse")),
            "a directory inside the corpus must be refused as unreadable scope, not reported \
             as an orphan FILE nobody declared — the distinction is whether anything looked \
             inside it: {audit:?}"
        );
    }

    #[test]
    fn a_manifest_path_that_escapes_the_corpus_is_refused() {
        // Flatness was enforced on the DISK side only, which is half a check.
        // A `[[fixture]]` path carrying a separator is joined onto the corpus
        // directory and READ from wherever it lands — `../` walks straight out
        // of the corpus — and the disk walk never crosses it, because that walk
        // reads one level and the file is not in it. So the entry is hashed
        // against a file outside the scope the audit reports on, and the audit
        // comes back clean.
        //
        // The escaping entry is the interesting half, but a plain subdirectory
        // reference is refused for the same reason: neither is a member of the
        // one directory level this audit can speak for.
        let outside = tempdir_with(&[("secret.json", "{}")]);
        let dir = tempdir_with(&[("sub", "not a directory, just a file")]);
        for escaping in ["../secret.json", "sub/nested.json", "a\\b.json"] {
            // A TOML LITERAL string (single quotes) for the path: a basic
            // string processes escapes, so `a\b.json` arrives as `a<BS>.json`
            // and the case under test never reaches the code. The first draft
            // of this test did exactly that and passed its third case for a
            // reason that had nothing to do with the guard.
            let m = Manifest::from_str(&format!(
                r#"
                [[fixture]]
                scenario = "S9"
                path     = '{escaping}'
                sha256   = "0000000000000000000000000000000000000000000000000000000000000000"
                currency = "unverified: cold-start"
                "#
            ))
            .expect("the manifest itself is well-formed; the PATH is what must be refused");
            let audit = m.verify(dir.path(), &["S9"]);
            assert!(
                audit.corrupt.iter().any(|s| s.contains(escaping)),
                "{escaping:?} must be refused before the read, as corrupt: {audit:?}"
            );
        }
        drop(outside);
    }

    #[test]
    fn a_missing_fixtures_directory_is_reported_not_silently_clean() {
        // The disk->manifest scan used to swallow the `Err` from
        // `std::fs::read_dir`, so with an empty
        // manifest (E1's own state, which has nothing for direction 1 to
        // report either) a directory that does not exist at all verified as
        // CLEAN. A directory that exists and is empty must stay clean — see
        // `an_empty_manifest_verifies_clean_which_is_exactly_e1` — but one
        // that cannot be read must not.
        let dir = tempdir_with(&[]);
        let missing = dir.path().join("does-not-exist");
        let audit = Manifest::from_str("").unwrap().verify(&missing, &[]);
        assert!(!audit.is_clean());
        assert!(audit
            .corrupt
            .iter()
            .any(|s| s.contains(&missing.display().to_string())));
    }

    #[test]
    fn an_entry_that_cannot_be_read_is_reported_not_silently_dropped() {
        // The directory-level read is guarded, and the ENTRY-level iteration
        // kept `.flatten()` — so an entry that cannot be read vanished and
        // `verify` returned a CLEAN audit over a corpus it had not fully seen.
        // Same defect, one level down: fixing one site of a class does not fix
        // the class.
        //
        // The error is INJECTED rather than forced out of the filesystem, and
        // that limitation is stated instead of hidden: there is no portable way
        // to make a real `ReadDir` fail on an entry (deleting the directory
        // mid-iteration is racy and platform-specific, and Windows does not
        // surface `FindNextFile` failures through `std` at all). What this pins
        // is the code's own decision — given an unreadable entry, it records a
        // finding — which is exactly what a revert to `.flatten()` would undo.
        let dir = tempdir_with(&[]);
        let declared = BTreeSet::new();
        let mut audit = FixtureAudit::default();
        Manifest::cross_disk_against_manifest(
            vec![Err(std::io::Error::other("simulated entry read failure"))],
            dir.path(),
            &declared,
            &mut audit,
        );
        assert!(
            !audit.is_clean(),
            "an entry the scan could not read must not leave the audit clean"
        );
        assert!(
            audit
                .corrupt
                .iter()
                .any(|s| s.contains("simulated entry read failure")),
            "the finding must name what could not be read: {:?}",
            audit.corrupt
        );
        assert!(
            audit
                .corrupt
                .iter()
                .any(|s| s.contains(&dir.path().display().to_string())),
            "the finding must name the directory it came from: {:?}",
            audit.corrupt
        );
    }

    #[test]
    fn a_readable_entry_is_still_crossed_against_the_manifest() {
        // The companion: making the entry level report failures must not turn
        // the direction-2 cross into dead code. Entered through `verify` over a
        // real directory, because `std::fs::DirEntry` has no public
        // constructor — the Ok arm can only be exercised for real.
        let dir = tempdir_with(&[("undeclared-after-the-fix.json", "{}")]);
        let audit = Manifest::from_str("").unwrap().verify(dir.path(), &["S9"]);
        assert!(audit
            .orphans
            .iter()
            .any(|s| s.contains("undeclared-after-the-fix.json")));
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

    #[test]
    fn the_thirty_percent_warning_fires_only_strictly_above_the_threshold() {
        // R23 says "if the unverified EXCEED 30% of the fixtures". Both sides
        // of the boundary, because a threshold test that only checks the far
        // side passes just as well with the comparison written backwards.
        let s = |total, unverified| FixtureSummary { total, unverified };
        assert_eq!(
            s(10, 3).unverified_warning(),
            None,
            "exactly 30% is not more than 30%"
        );
        let warned = s(10, 4)
            .unverified_warning()
            .expect("4 of 10 is above the threshold");
        assert!(
            warned.contains("4") && warned.contains("10"),
            "the warning must carry the counts a reader would act on: {warned}"
        );
        assert_eq!(
            s(0, 0).unverified_warning(),
            None,
            "an empty corpus has no unverified share, and 0 of 0 must not read as a problem"
        );
        assert!(
            s(3, 3).unverified_warning().is_some(),
            "a corpus that is entirely unverified is the case the warning exists for"
        );
    }

    #[test]
    fn the_end_of_run_count_names_both_classes() {
        // R23 asks the harness to report how many there are of each class when
        // it finishes. `unverified` was computed and read by nobody: grep for
        // it outside this module returned nothing, so neither this count nor
        // the certificate warning existed. That is the exact failure R23 was
        // written to prevent, reproduced in the code meant to implement it.
        let line = FixtureSummary {
            total: 7,
            unverified: 2,
        }
        .report_line();
        assert!(
            line.contains("7") && line.contains("2"),
            "the count must name the total and the unverified share: {line}"
        );
    }

    #[test]
    fn the_audit_hands_on_the_counts_it_computed() {
        // The connection that did not exist: the audit knew both numbers and
        // nothing carried them out of the preflight.
        let audit = FixtureAudit {
            total: 5,
            unverified: 2,
            ..FixtureAudit::default()
        };
        assert_eq!(
            audit.summary(),
            FixtureSummary {
                total: 5,
                unverified: 2
            }
        );
    }
}
