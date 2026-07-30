// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-29

//! The verdict sentinel: delimits an agent's verdict inside its raw output.
//!
//! # The no-search rule — it carries all of this module's security
//!
//! **NORMALIZING inside an already-delimited region is PERMITTED.
//! SEARCHING outside the markers is FORBIDDEN, always.**
//!
//! Any change that adds a search outside the markers **undoes this module**, however
//! useful it looks: the parser would go back to *guessing* which of several JSON objects
//! was the verdict, which is exactly the defect this module exists to erase.
//!
//! ## What the no-search rule does NOT forbid
//!
//! "Searching" means **choosing between candidates** — probing positions until
//! something decodes, or preferring one object over another. It does not forbid:
//!
//! - **Scanning lines to LOCATE the markers.** The text must be walked to learn
//!   where the block starts and ends; that is locating, not choosing, and the
//!   result is either deterministic or an error.
//! - **Normalizing a line during that scan** in order to compare it against a
//!   marker. That is how *"is this line the marker?"* gets decided.
//! - **Normalizing INSIDE the already-delimited block** (fence stripping, trim).
//!
//! The line is this: **before** the block is in hand, the only admissible
//! question is *"is this line a marker?"* — never *"does this object look like a
//! verdict?"*. Zero heuristics, zero tie-breaks, zero fallbacks.
//!
//! # Scope (SRP)
//!
//! This module **delimits and nothing else**: it does not validate the 7-key
//! schema (that is `validate.rs`), does not speak HTTP, and does not launch
//! agents. It is pure and **never panics** — every failure is a typed `Err`.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

/// Opening marker of the verdict block. ASCII by construction — and that is load-bearing,
/// not incidental: because the case fold is ASCII-only, no non-ASCII scalar can fold into
/// a marker character, so the guarantee is structural rather than a property of this
/// particular string. (The reasoning lives on the crate-internal `normalize_line`, which
/// cannot be linked from public docs.)
pub const VERDICT_OPEN: &str = "<MAGI_VERDICT>";

/// Closing marker of the verdict block. ASCII by construction.
pub const VERDICT_CLOSE: &str = "</MAGI_VERDICT>";

/// Unicode categories stripped from a line before comparing it to a marker.
///
/// **By CATEGORY, not by list:** `Cf` covers *every* format/invisible code point
/// — including `U+00AD` (SOFT HYPHEN) and `U+180E`, which a hardcoded list left
/// out in the reference implementation. The category is exhaustive and does not
/// age. `Mn` covers nonspacing marks (variation selectors and friends).
static STRIPPED_CATEGORIES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{Cf}\p{Mn}]").expect("valid STRIPPED_CATEGORIES_RE"));

/// Steps **1 and 2** of the canonical form of a line of *model* output: drop
/// `Cf`/`Mn`, then trim. One pass, `O(c)`.
///
/// # ⚠ Step 3 (the ASCII case fold) is NOT here — deliberately
///
/// The fold is defined as `eq_ignore_ascii_case` **against the marker, without
/// allocating**: that is a **comparison**, not a transformation. Folding the line
/// into a canonical form would allocate a second `String` per line *and* require
/// lowercasing the constant too, adding a fresh place to get it wrong (forget to
/// lower the marker and nothing ever matches).
///
/// **BINDING CONSEQUENCE:** the result of this function is **never compared with
/// `==`**. Doing so yields case-SENSITIVE comparison — a silent divergence from
/// the predicate. The only legitimate consumer is [`is_marker_line`]; everything
/// else goes through it.
///
/// [`is_exact_marker_line`] is **not** a consumer and must not become one — it is the
/// strict half of the two-predicate asymmetry, so it compares the raw trimmed line, and
/// normalizing for it would erase the very difference that makes it strict.
///
/// # Why the order of the steps carries weight
///
/// The witness is `İ` (U+0130), in `<MAGİ_VERDICT>`:
///
/// | Order | Trace | Result |
/// |---|---|---|
/// | **Current** (strip → trim → ASCII-fold) | `İ` is category `Lu`, so it survives the strip; the ASCII fold cannot touch it because it is not ASCII | **no match — SAFE** |
/// | **Inverted** (Unicode case-fold first) | the fold decomposes `İ` into `i` + `U+0307`; the later strip removes `U+0307` because it is `Mn` | **matches — UNSAFE** |
///
/// The order and the ASCII fold are **independent** defences: the order protects
/// against a future return to Unicode case-folding, the ASCII fold protects even
/// if someone inverts the order. Neither makes the other redundant.
///
/// # Do NOT reuse `consensus::dedup_key`
///
/// It applies NFKC, and **NFKC maps `＜` (U+FF1C) to `<`** — a homoglyph would
/// become a marker: a fabricated fail-open. Marker normalization carries no NFKC.
///
/// # Parameters / Returns
///
/// * `line` — one line of model output, already split by the caller.
///
/// Returns the line with `Cf`/`Mn` removed and surrounding whitespace trimmed.
///
/// # Zero allocation in the common case
///
/// This runs for **every line of every agent response**, so it returns a `Cow`: a line
/// containing no `Cf`/`Mn` — which is nearly all of them — borrows straight from the input
/// and allocates nothing. Only a line that actually carries an invisible takes the owned
/// branch. `strip_invisibles` in `user_prompt` uses the same shape for the same reason.
pub(crate) fn normalize_line(line: &str) -> Cow<'_, str> {
    match STRIPPED_CATEGORIES_RE.replace_all(line, "") {
        Cow::Borrowed(s) => Cow::Borrowed(s.trim()),
        // The rare branch. Trim IN PLACE rather than `s.trim().to_string()`, which would
        // allocate a second buffer for a string `replace_all` just allocated. Both offsets
        // are whitespace-boundary byte counts, so neither can split a codepoint.
        //
        // NOT `replace_all(line.trim(), "")` — that inverts the step order R4 fixes, and
        // the inversion is observable: for `" \u{200b} <MAGI_VERDICT>"`, trimming first
        // leaves the inner space stranded after the strip (`" <MAGI_VERDICT>"`, no match),
        // while strip-then-trim collapses it correctly.
        Cow::Owned(mut s) => {
            let end = s.trim_end().len();
            s.truncate(end);
            let start = s.len() - s.trim_start().len();
            s.drain(..start);
            Cow::Owned(s)
        }
    }
}

/// PERMISSIVE — for the output of the **MODEL** (untrusted, outside our control).
///
/// | Predicate | Applied to | Criterion | Why |
/// |---|---|---|---|
/// | `is_marker_line` | the **MODEL's** output | **PERMISSIVE**: strips invisibles, trims, ignores case | the model is **untrusted** and we do not control its output; killing it over a zero-width character is **giving away a retry** |
/// | [`is_exact_marker_line`] | **OUR** `.md` files | **STRICT**: the line, trimmed, **IS** the ASCII marker | these are files we **ship**; an invisible there is **corruption**, and it has to be visible |
///
/// **Their asymmetry IS the invariant**, which is why the two live side by side
/// under one shared doc table. A single shared predicate has already failed twice
/// in this design: shared-and-permissive let a corrupted `.md` through;
/// shared-and-strict aborted a run with a false FATAL over a BOM. The fix was
/// neither predicate — it was moving the BOM to the **encoding layer**, where it
/// is resolved before anything is compared.
///
/// # Parameters / Returns
///
/// * `line` — the candidate line. **First** argument.
/// * `marker` — [`VERDICT_OPEN`] or [`VERDICT_CLOSE`]. **Second** argument.
///
/// The argument order is **not symmetric**: both are `&str`, so swapping them
/// compiles and silently never matches.
///
/// Returns `true` when `line` is that marker under permissive normalization.
pub(crate) fn is_marker_line(line: &str, marker: &str) -> bool {
    normalize_line(line).eq_ignore_ascii_case(marker)
}

/// STRICT — for **OUR** shipped `.md` files. See the table on [`is_marker_line`]:
/// the asymmetry between the two is deliberate and load-bearing.
///
/// Trims surrounding whitespace and requires byte equality with the ASCII marker.
/// No invisible stripping, no case folding: in a file we ship, an invisible
/// character inside a marker line is corruption, not tolerance owed.
///
/// # Parameters / Returns
///
/// * `line` — the candidate line. **First** argument.
/// * `marker` — [`VERDICT_OPEN`] or [`VERDICT_CLOSE`]. **Second** argument.
///
/// Returns `true` only when the trimmed line is exactly that marker.
pub(crate) fn is_exact_marker_line(line: &str, marker: &str) -> bool {
    line.trim() == marker
}

/// The ONLY separators that count as a line ending: the ones JSON escapes inside a
/// string. **`\r\n` comes FIRST** — the crate's alternation is leftmost-first, so with
/// `\r` ahead a CRLF would split into two lines (an empty one in between) and a marker
/// followed by CRLF would stop anchoring.
///
/// # This is a real bug, with a casualty
///
/// Python's `str.splitlines()` also splits on `\v`, `\f`, `\x1c-\x1e`, `U+0085`,
/// `U+2028` and `U+2029` — and the last three are **legal raw inside a JSON string**.
/// A finding *about the sentinel* that quoted the marker behind a `U+2028` left it alone
/// on its own line → two closes → the mage died. It failed closed, yes, but in a case
/// the guarantee called impossible: *the guarantee was wider than the code*.
///
/// Narrowing the set **cannot** open a fail-open: a line containing `U+2028` does not
/// normalize to a marker, so it can only fail closed.
///
/// **Do NOT use `str::lines()`** (does not split on a lone `\r`) and **do NOT reuse
/// `user_prompt::normalize_newlines`** (it includes `U+2028`/`U+2029`/`U+000B`/`U+000C`/
/// `U+0085` on purpose, to sanitize user input — applying it here would MUTATE the JSON
/// payload).
static LINE_BREAK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\r\n|\r|\n").expect("valid LINE_BREAK_RE"));

/// A fence opener or closer: ``` or ~~~, with a **permissive** info string on both ends.
///
/// Group 1 is the fence type, and it is what gets compared between the two ends.
///
/// # Why the info string is permissive
///
/// A whitelist would enumerate what is allowed, and then every language with an odd
/// character (`c#`) and every model that writes two words becomes a future failure —
/// **each one costing a retry on a verdict that was never wrong**. The closer tolerates
/// an info string because models echo the opener's when closing.
///
/// # Accepted limitation
///
/// Does **not** match fences of 4+ characters (` ```` `), with or without an info
/// string, because `[^`~]*` cannot consume the fourth marker character. It fails in the
/// safe direction: the fence is not stripped, `serde_json` rejects the content, and the
/// mage gets a retry with `InvalidJson`. One extra retry on a case no model produces,
/// versus widening the pattern and adding surface to get wrong.
static FENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(```|~~~)[^`~]*$").expect("valid FENCE_RE"));

/// Why extraction or validation of an agent's verdict failed.
///
/// The orchestrator selects the retry instruction by **this type**, never by the text of
/// an error message: matching strings is brittle (a rewording silently breaks the
/// feedback) and is a second-order injection surface.
#[non_exhaustive]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum ExtractionFailureCause {
    /// No markers at all — the model did not emit the block.
    MissingMarkers,
    /// One of the two markers is missing: the signature of a TRUNCATED output.
    Unterminated,
    /// More than one block, or a close before its open. No tie-break is applied.
    Ambiguous,
    /// The delimited block is not valid JSON.
    InvalidJson,
    /// The object parsed but the validator rejected it.
    Schema,
    /// The output reproduces the worked example from the instructions.
    EchoedExample,
    /// The verdict claims to come from a different mage than the one dispatched.
    AgentIdentity,
    /// Catch-all for deserializing a cause a newer version produced.
    ///
    /// **Never constructed by this crate** — it exists so a consumer on an older
    /// version can still read a report written by a newer one instead of failing to
    /// parse the whole document over one telemetry field. Same shape and same reason as
    /// [`crate::schema::Category`]'s catch-all.
    #[serde(other)]
    Other,
}

/// Failure of [`extract`]: no single delimited verdict block could be obtained.
///
/// Carries a typed [`ExtractionFailureCause`] alongside a human-readable message. Use
/// [`Self::cause`] to branch; never parse the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictExtractionError {
    cause: ExtractionFailureCause,
    message: String,
}

impl VerdictExtractionError {
    /// The typed reason this extraction failed. Branch on this, not on the message.
    #[must_use]
    pub fn cause(&self) -> ExtractionFailureCause {
        self.cause
    }
}

impl std::fmt::Display for VerdictExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for VerdictExtractionError {}

/// THE delimitation, parameterized by the marker predicate.
///
/// `is_marker(line, marker)` — the **first** argument is the candidate line, the
/// **second** is the constant. Not symmetric: swapping them compiles and silently never
/// matches on the inputs where normalization matters.
///
/// # Its only two callers
///
/// | Caller | Predicate | Applied to |
/// |---|---|---|
/// | [`extract`] | [`is_marker_line`] (permissive) | the MODEL's output |
/// | `prompts::validate_prompt` | [`is_exact_marker_line`] (strict) | OUR prompts |
///
/// **The guard is a FAITHFUL simulation of the parser.** Anywhere the guard sees *less*
/// than the parser is an evasion hole — with two separate implementations, a prompt with
/// a 7-key object inside a fence between the markers passed the guard (serde failed on
/// the fence) while the parser, which does strip fences, would have accepted it. The only
/// admissible difference between the two is the **marker predicate**, because that
/// asymmetry is deliberate and points the safe way: our own files are judged more
/// strictly, never more loosely.
///
/// # The ORDER of the checks is load-bearing
///
/// `MissingMarkers` → `Unterminated` → `Ambiguous` (count) → `Ambiguous` (order).
///
/// The orchestrator picks the retry feedback by the **type** of the error, so telling a
/// model that emitted **no** marker *"you emitted more than one block"* spends the retry
/// on a **false** instruction, and the mage dies to a bug in the algorithm that exists to
/// save it. Line numbers in messages are **1-based**: the reader is going to open the
/// file at line N.
///
/// **There is no "first close" nor "last close".** Choosing between them would be a
/// tie-break rule, and tie-break rules are heuristics — exactly what this module erases.
///
/// # Returns
///
/// The delimited block, fence-stripped and trimmed, **borrowed from `text`**. The region
/// is contiguous and only its ends move, so nothing is reconstructed.
///
/// # Errors
///
/// [`VerdictExtractionError`] with the cause set per the order above.
/// The lifetime is elided but **not absent**: the returned slice borrows from `text`, so
/// the block lives exactly as long as the raw response does.
pub(crate) fn locate_block(
    text: &str,
    is_marker: impl Fn(&str, &str) -> bool,
) -> Result<&str, VerdictExtractionError> {
    // (line_index, byte offset just past the line) for opens;
    // (line_index, byte offset at the line start) for closes.
    let mut opens: Vec<(usize, usize)> = Vec::new();
    let mut closes: Vec<(usize, usize)> = Vec::new();

    let mut cursor = 0usize;
    let mut index = 0usize;
    let mut classify = |start: usize, end: usize, index: usize| {
        let line = &text[start..end];
        if is_marker(line, VERDICT_OPEN) {
            opens.push((index, end));
        } else if is_marker(line, VERDICT_CLOSE) {
            closes.push((index, start));
        }
    };
    for m in LINE_BREAK_RE.find_iter(text) {
        classify(cursor, m.start(), index);
        cursor = m.end();
        index += 1;
    }
    classify(cursor, text.len(), index);

    let fail = |cause, message: String| Err(VerdictExtractionError { cause, message });

    if opens.is_empty() && closes.is_empty() {
        return fail(
            ExtractionFailureCause::MissingMarkers,
            format!(
                "no verdict markers found: expected {VERDICT_OPEN} and {VERDICT_CLOSE}, \
                 each alone on its own line"
            ),
        );
    }
    if opens.is_empty() || closes.is_empty() {
        return fail(
            ExtractionFailureCause::Unterminated,
            format!(
                "unterminated verdict block: {} open and {} close marker(s) \
                 (the signature of a truncated output)",
                opens.len(),
                closes.len()
            ),
        );
    }
    if opens.len() != 1 || closes.len() != 1 {
        return fail(
            ExtractionFailureCause::Ambiguous,
            format!(
                "expected exactly one verdict block, found {} open and {} close markers",
                opens.len(),
                closes.len()
            ),
        );
    }

    let (open_index, open_end) = opens[0];
    let (close_index, close_start) = closes[0];
    if close_index < open_index {
        return fail(
            ExtractionFailureCause::Ambiguous,
            format!(
                "close marker precedes its open marker (open at line {}, close at line {})",
                open_index + 1,
                close_index + 1
            ),
        );
    }

    let block = text.get(open_end..close_start).unwrap_or("").trim();
    Ok(strip_fence(block))
}

/// Strips the fence **only** when it wraps the block completely and both ends are the
/// **same type**.
///
/// If the fence does not wrap the whole block, the content is left **INTACT** and
/// `serde_json` decides: trimming *"whatever is in the way"* until something decodes
/// **would be searching again**.
///
/// Uses the **same line splitter** as [`locate_block`]: two notions of "first line"
/// inside one module would break the single definition exactly where it shows most (a
/// CR-only block).
///
/// Returns a **subslice** — never reconstructs lines.
///
/// # Which predicate recognizes a fence: NEITHER of the two
///
/// The asymmetry of the marker predicates is about **markers**, not fences. A fence line
/// is recognized from the **raw, merely trimmed** line — no `Cf`/`Mn` strip, no case
/// fold — because a fence delimits nothing and carries no authority; it only wraps.
/// Applying marker normalization here would copy a security mechanism to a place where
/// it buys nothing and costs an allocation per line.
fn strip_fence(block: &str) -> &str {
    let mut breaks = LINE_BREAK_RE.find_iter(block);
    let Some(first) = breaks.next() else {
        return block; // single line: it cannot be both ends of a pair (E15b)
    };
    let last = breaks.last().unwrap_or(first);

    let opener = &block[..first.start()];
    let closer = &block[last.end()..];
    let (Some(o), Some(c)) = (FENCE_RE.captures(opener), FENCE_RE.captures(closer)) else {
        return block;
    };
    if o.get(1).map(|m| m.as_str()) != c.get(1).map(|m| m.as_str()) {
        return block; // mismatched pair is not a fence: it is text (E15)
    }

    let (start, end) = (first.end(), last.start());
    if start > end {
        return ""; // fence with no content between its two lines
    }
    // `get` cannot be `None` here: both offsets come from regex match boundaries on
    // `block`, so they are char boundaries, and `start <= end` was just checked. The
    // fallback exists to keep the function TOTAL without an `expect()` — the standards
    // forbid one outside `#[cfg(test)]`, and a panic in the parser would be the worst
    // possible outcome for text a model controls. Its direction is deliberate:
    // returning the block UNSTRIPPED hands the decision to `serde_json`, so an
    // impossible state degrades to `InvalidJson` and a retry, never to a wrong slice.
    block.get(start..end).unwrap_or(block).trim()
}

/// Extracts the delimited verdict block from a **model's** raw output (permissive
/// predicate).
///
/// Returns a `&str` **borrowed** from `text`: the region between the markers is
/// contiguous, and stripping the fence and trimming leave it contiguous.
///
/// # Errors
///
/// [`VerdictExtractionError`]; branch on [`VerdictExtractionError::cause`], never on the
/// message text.
///
/// # Examples
///
/// ```
/// use magi_core::verdict_markers::{extract, ExtractionFailureCause, VERDICT_CLOSE, VERDICT_OPEN};
///
/// // The model may reason freely OUTSIDE the markers — none of it is read.
/// let raw = format!("thinking out loud...\n{VERDICT_OPEN}\n{{\"a\":1}}\n{VERDICT_CLOSE}");
/// assert_eq!(extract(&raw).unwrap(), "{\"a\":1}");
///
/// // A bare JSON object is NOT a verdict: there is no search outside the markers.
/// let err = extract("{\"agent\":\"caspar\"}").unwrap_err();
/// assert_eq!(err.cause(), ExtractionFailureCause::MissingMarkers);
/// ```
pub fn extract(text: &str) -> Result<&str, VerdictExtractionError> {
    locate_block(text, is_marker_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markers_are_ascii() {
        // Precondition that makes the ASCII case-fold guarantee STRUCTURAL: no
        // non-ASCII scalar can fabricate a marker, whatever the marker becomes.
        assert!(VERDICT_OPEN.is_ascii());
        assert!(VERDICT_CLOSE.is_ascii());
    }

    #[test]
    fn test_is_marker_line_accepts_invisibles_and_case_drift() {
        // E9/E10 — the model is untrusted and we do not control its output;
        // killing a mage over a zero-width character is giving away a retry.
        assert!(is_marker_line("<MAGI_\u{200b}VERDICT>", VERDICT_OPEN));
        assert!(is_marker_line("<MAGI\u{00ad}_VERDICT>", VERDICT_OPEN));
        assert!(is_marker_line("<MAGI\u{180e}_VERDICT>", VERDICT_OPEN));
        assert!(is_marker_line("  <magi_verdict>  ", VERDICT_OPEN));
        assert!(is_marker_line("</MAGI_VERDICT>", VERDICT_CLOSE));
    }

    #[test]
    fn test_is_marker_line_rejects_fullwidth_homoglyph() {
        // E11 — a homoglyph is a DIFFERENT character, not an invisible one.
        // Accepting it would mean accepting something that is not the marker.
        assert!(!is_marker_line(
            "\u{ff1c}MAGI_VERDICT\u{ff1e}",
            VERDICT_OPEN
        ));
    }

    #[test]
    fn test_is_marker_line_rejects_dotted_capital_i() {
        // E11b — the witness for step ORDER. `İ` (U+0130) is category Lu, so it
        // survives the Cf/Mn strip, and the ASCII fold cannot touch it because it
        // is not ASCII. Under a Unicode case-fold applied FIRST it would
        // decompose to `i` + U+0307, the Mn strip would remove the mark, and this
        // would match. Two independent defences; this test pins both.
        assert!(!is_marker_line("<MAG\u{0130}_VERDICT>", VERDICT_OPEN));
    }

    #[test]
    fn test_case_insensitivity_is_a_property_of_the_predicate_not_of_normalize_line() {
        // R4 step 3 lives in the COMPARISON (no allocation). This pins the split
        // so a refactor that "helpfully" compares normalize_line output with ==
        // fails loudly instead of silently becoming case-sensitive.
        assert_eq!(normalize_line("  <MAGI_VERDICT>  "), "<MAGI_VERDICT>");
        assert_ne!(normalize_line("<magi_verdict>"), VERDICT_OPEN);
        assert!(is_marker_line("<magi_verdict>", VERDICT_OPEN));
    }

    #[test]
    fn test_the_two_predicates_disagree_exactly_where_the_asymmetry_says() {
        // E21 — the trust asymmetry, pinned in one place. Permissive over the
        // model's output; strict over files we ship, where an invisible is
        // corruption and has to be visible.
        let corrupted = "<MAGI_\u{200b}VERDICT>";
        assert!(is_marker_line(corrupted, VERDICT_OPEN), "permissive: model");
        assert!(
            !is_exact_marker_line(corrupted, VERDICT_OPEN),
            "strict: our own files"
        );
        assert!(is_exact_marker_line("  <MAGI_VERDICT>  ", VERDICT_OPEN));
        assert!(is_exact_marker_line(VERDICT_CLOSE, VERDICT_CLOSE));
    }

    // ---- Etapa B: locate_block ----

    fn wrap(body: &str) -> String {
        format!("{VERDICT_OPEN}\n{body}\n{VERDICT_CLOSE}")
    }

    fn locate(text: &str) -> Result<&str, VerdictExtractionError> {
        locate_block(text, is_marker_line)
    }

    #[test]
    fn test_locate_returns_only_the_delimited_block() {
        // E1 — prose and <think> outside the markers are never read.
        let raw = format!("prosa\n<think>ruido</think>\n{}\ncola", wrap("{\"a\":1}"));
        assert_eq!(locate(&raw).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn test_locate_missing_markers_when_none_present() {
        // E2/E3 — a bare 7-key object is NOT a verdict. No fast path, no fallback.
        assert_eq!(
            locate("{\"agent\":\"caspar\"}").unwrap_err().cause(),
            ExtractionFailureCause::MissingMarkers
        );
    }

    #[test]
    fn test_locate_unterminated_when_only_open() {
        // E5 — the signature of a truncated output. The retry must say "re-emit whole".
        assert_eq!(
            locate(&format!("{VERDICT_OPEN}\n{{}}"))
                .unwrap_err()
                .cause(),
            ExtractionFailureCause::Unterminated
        );
    }

    #[test]
    fn test_locate_unterminated_when_only_close() {
        assert_eq!(
            locate(&format!("{{}}\n{VERDICT_CLOSE}"))
                .unwrap_err()
                .cause(),
            ExtractionFailureCause::Unterminated
        );
    }

    #[test]
    fn test_locate_ambiguous_when_two_blocks() {
        // E6 — no "first", no "last": choosing would be a tie-break, and tie-breaks are
        // heuristics. Fail closed.
        let raw = format!("{}\n{}", wrap("{\"a\":1}"), wrap("{\"b\":2}"));
        assert_eq!(
            locate(&raw).unwrap_err().cause(),
            ExtractionFailureCause::Ambiguous
        );
    }

    #[test]
    fn test_locate_reports_1_based_lines_when_close_precedes_open() {
        // E7 — 1-based because the message speaks to someone who will open the file at
        // line N, and every editor, grep and human counts from 1.
        let raw = format!("x\n{VERDICT_CLOSE}\nx\nx\nx\n{VERDICT_OPEN}\nx");
        let e = locate(&raw).unwrap_err();
        assert_eq!(e.cause(), ExtractionFailureCause::Ambiguous);
        let msg = e.to_string();
        assert!(msg.contains("line 6"), "open at 1-based line 6: {msg}");
        assert!(msg.contains("line 2"), "close at 1-based line 2: {msg}");
    }

    #[test]
    fn test_check_order_missing_beats_ambiguous() {
        // E8 — the [CRITICAL] regression. Telling a model that emitted NO marker "you
        // emitted more than one block" spends the retry on a FALSE instruction, and the
        // mage dies to a bug in the very algorithm meant to save it.
        assert_eq!(
            locate("solo prosa").unwrap_err().cause(),
            ExtractionFailureCause::MissingMarkers
        );
    }

    #[test]
    fn test_locate_supports_cr_only_line_breaks() {
        // E13 — str::lines() does not split on a lone \r; this is why we do not use it.
        let raw = format!("{VERDICT_OPEN}\r{{\"a\":1}}\r{VERDICT_CLOSE}");
        assert_eq!(locate(&raw).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn test_locate_supports_crlf() {
        // The `\r\n`-first alternation order: with `\r` ahead this would split into two
        // lines and the markers would stop anchoring.
        let raw = format!("{VERDICT_OPEN}\r\n{{\"a\":1}}\r\n{VERDICT_CLOSE}");
        assert_eq!(locate(&raw).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn test_u2028_inside_a_json_string_does_not_split_the_block() {
        // E12 — the case that killed a real mage. U+2028 is LEGAL raw inside a JSON
        // string, so treating it as a line break turned a quoted marker into a real one.
        let body = format!("{{\"detail\":\"quoting\u{2028}{VERDICT_CLOSE} here\"}}");
        assert_eq!(locate(&wrap(&body)).unwrap(), body);
    }

    #[test]
    fn test_same_line_markers_report_missing_not_a_block() {
        // E13b — markers are LINE-anchored: a line containing anything else is not a
        // marker. Fail-closed but imprecise, and accepted: the MissingMarkers feedback
        // already instructs "each one alone on its own line", which is the fix.
        let raw = format!("{VERDICT_OPEN}{{\"a\":1}}{VERDICT_CLOSE}");
        assert_eq!(
            locate(&raw).unwrap_err().cause(),
            ExtractionFailureCause::MissingMarkers
        );
    }

    #[test]
    fn test_strict_predicate_rejects_what_permissive_accepts_same_input() {
        // E19d — guard and parser see the SAME content; the only admissible difference
        // is that the strict predicate REJECTS invisibles.
        let raw = format!("<MAGI_\u{200b}VERDICT>\n{{\"a\":1}}\n{VERDICT_CLOSE}");
        assert_eq!(locate_block(&raw, is_marker_line).unwrap(), "{\"a\":1}");
        assert!(locate_block(&raw, is_exact_marker_line).is_err());
    }

    #[test]
    fn test_locate_block_argument_order_is_not_symmetric() {
        // The (line, marker) order cannot be enforced by the type system — both are
        // &str, so swapping them COMPILES.
        //
        // THE INPUT MUST BE DIRTY. With the correct order the LINE is normalized and
        // compared against the clean marker; swapped, the MARKER is normalized (a no-op)
        // and compared against the raw line. On CLEAN input both agree and return true,
        // so a clean-input version of this test would pass with the bug present — worse
        // than having no test at all.
        let swapped = |marker: &str, line: &str| is_marker_line(line, marker);
        let dirty = format!("<MAGI_\u{200b}VERDICT>\n{{\"a\":1}}\n{VERDICT_CLOSE}");
        assert!(
            locate_block(&dirty, is_marker_line).is_ok(),
            "correct order normalizes the LINE"
        );
        assert!(
            locate_block(&dirty, swapped).is_err(),
            "swapped args must not silently work"
        );
    }

    #[test]
    fn test_locate_never_panics_on_adversarial_input() {
        for raw in [
            "",
            "\r",
            "\n\n\n",
            VERDICT_OPEN,
            VERDICT_CLOSE,
            "\u{2028}",
            "\u{200b}",
        ] {
            let _ = locate(raw);
        }
    }

    // ---- Etapa C: strip_fence + extract ----

    #[test]
    fn test_strip_fence_removes_a_complete_matching_pair() {
        // E14
        assert_eq!(
            extract(&wrap("```json\n{\"a\":1}\n```")).unwrap(),
            "{\"a\":1}"
        );
        assert_eq!(extract(&wrap("~~~\n{\"a\":1}\n~~~")).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn test_strip_fence_leaves_an_unbalanced_fence_intact() {
        // E14 — trimming "whatever is in the way" until something decodes WOULD BE
        // searching again. Leave it and let serde_json decide.
        assert_eq!(
            extract(&wrap("```json\n{\"a\":1}")).unwrap(),
            "```json\n{\"a\":1}"
        );
    }

    #[test]
    fn test_strip_fence_leaves_a_mismatched_pair_intact() {
        // E15 — a mismatched pair is not a fence: it is text.
        assert_eq!(
            extract(&wrap("```\n{\"a\":1}\n~~~")).unwrap(),
            "```\n{\"a\":1}\n~~~"
        );
    }

    #[test]
    fn test_strip_fence_tolerates_info_strings_on_both_ends() {
        // E16 — models echo the opener's info string when closing. Spending a retry on
        // a verdict that was never wrong is the cost of being strict here.
        assert_eq!(
            extract(&wrap("```json title=\"x\"\n{\"a\":1}\n```json")).unwrap(),
            "{\"a\":1}"
        );
    }

    #[test]
    fn test_strip_fence_leaves_a_four_backtick_fence_intact() {
        // The fence pattern's `[^`~]*` cannot consume a fourth backtick, so a 4+ fence —
        // which markdown permits — is not recognized. That is a DOCUMENTED limitation, and
        // it is accepted because it fails in the safe direction: the fence stays, the
        // content is left intact, `serde_json` rejects it, and the mage gets a retry with
        // `InvalidJson`. A retry too many on a case no model produces, versus widening the
        // pattern and gaining somewhere new to be wrong.
        //
        // Without this test the limitation is only prose, and prose does not fail when
        // someone widens the pattern "harmlessly".
        assert_eq!(
            extract(&wrap("````json\n{\"a\":1}\n````")).unwrap(),
            "````json\n{\"a\":1}\n````"
        );
        // Holds WITHOUT an info string too — it is the fourth backtick that blocks the
        // match, not the trailing text.
        assert_eq!(
            extract(&wrap("````\n{\"a\":1}\n````")).unwrap(),
            "````\n{\"a\":1}\n````"
        );
    }

    #[test]
    fn test_strip_fence_uses_the_same_line_splitter_as_the_delimitation() {
        // The two axes are tested apart — CR-only anchoring, and fences over `\n` — but
        // their CROSSING is what the rustdoc claims: `strip_fence` must find its first
        // and last line with the R5 splitter, not `str::lines()`. With `str::lines()`
        // the CR-only case below sees ONE line, so the fence is never recognized and the
        // block comes back with its backticks — a stripping rule that silently stops
        // applying for a model that emits CR-only.
        let cr = format!("{VERDICT_OPEN}\r```json\r{{\"a\":1}}\r```\r{VERDICT_CLOSE}");
        assert_eq!(extract(&cr).unwrap(), "{\"a\":1}");

        let crlf = format!("{VERDICT_OPEN}\r\n```json\r\n{{\"a\":1}}\r\n```\r\n{VERDICT_CLOSE}");
        assert_eq!(extract(&crlf).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn test_strip_fence_leaves_a_single_line_block_intact() {
        // E15b — with one line, first and last are the SAME, and a line cannot be both
        // ends of a pair. An implementation comparing first/last without a length check
        // would eat the only line and return empty.
        assert_eq!(extract(&wrap("```json")).unwrap(), "```json");
        assert_eq!(extract(&wrap("{\"a\":1}")).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn test_extract_is_zero_copy_over_the_input() {
        // The returned &str points INTO the input: the region is contiguous and only its
        // ends move.
        let raw = wrap("{\"a\":1}");
        let got = extract(&raw).unwrap();
        let raw_range = raw.as_ptr() as usize..raw.as_ptr() as usize + raw.len();
        assert!(raw_range.contains(&(got.as_ptr() as usize)));
    }

    #[test]
    fn test_cause_serializes_as_kebab_case() {
        let json = serde_json::to_string(&ExtractionFailureCause::MissingMarkers).unwrap();
        assert_eq!(json, "\"missing-markers\"");
    }

    #[test]
    fn test_unknown_cause_deserializes_to_other_instead_of_failing() {
        // Forward-compat: a consumer on this version must still read a report written by
        // a newer one, rather than failing to parse the whole document.
        let c: ExtractionFailureCause = serde_json::from_str("\"some-future-cause\"").unwrap();
        assert_eq!(c, ExtractionFailureCause::Other);
    }

    #[test]
    fn test_normalize_line_strips_by_category_not_by_list() {
        // The category is exhaustive: U+180E was reclassified Zs -> Cf in Unicode
        // 6.3 and a hardcoded list missed it. U+0301 is a combining acute (Mn).
        assert_eq!(normalize_line("a\u{180e}b"), "ab");
        assert_eq!(normalize_line("a\u{00ad}b"), "ab");
        assert_eq!(normalize_line("a\u{0301}b"), "ab");
        assert_eq!(normalize_line("a\u{e0041}b"), "ab", "tag characters are Cf");
    }
}
