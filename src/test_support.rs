// Author: Julian Bolivar
// Version: 4.1.0
// Date: 2026-09-08

//! Test-only support utilities. Gated `#[cfg(any(test, feature = "test-utils"))]`
//! at the module declaration in `lib.rs`.
//!
//! **Stability:** the `test-utils` feature is stable only within the v0.4.x
//! line. Future versions may rename, restructure, or remove this module.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;

use crate::agent::CURRENT_AGENT_IDENTITY;
use crate::error::{ExternalErrorKind, ProviderError};
use crate::orchestrator::{Magi, MagiBuilder};
use crate::provider::{Completion, CompletionConfig, LlmProvider};
use crate::reporting::MagiReport;
use crate::rotation::{FallbackPool, Lineage, ProviderProbe, RotationKind};
use crate::schema::AgentName;
use crate::verdict_markers::{VERDICT_CLOSE, VERDICT_OPEN};

/// A short literal prompt, for the cases that do not measure size.
pub const USER_PROMPT: &str = "analyze this";

/// A prompt of exactly `bytes` bytes.
///
/// It TAKES its size rather than choosing one, and that is a correction: with the
/// size baked in, a scenario had TWO sources for the same number -- its own
/// constant and whatever this produced -- so the precondition probe could be
/// certifying a block against a payload the test never sends.
pub fn big_prompt(bytes: usize) -> String {
    "p".repeat(bytes)
}

/// The byte budget every diagnosis on `ProviderError::Process.stderr` fits within.
///
/// Re-exported so a test in another crate asserts against the SAME constant the code
/// applies, instead of a literal of its own. That is R-19's rule reaching a second
/// table before it can go wrong: a test carrying its own copy of a bound compares the
/// copy against itself and stays green when the bound moves.
///
/// Gated on `claude-cli` and NOT on a copy of the source constant's own three-feature
/// list, which would be a second list to keep in step -- the defect one file over. The
/// only consumer is the CLI integration suite, so this is the honest condition, and it
/// inherits the tripwire rather than duplicating it: the day `claude-cli` leaves that
/// list, this line stops compiling and names itself.
#[cfg(feature = "claude-cli")]
pub const ERROR_BODY_CAP: usize = crate::error::MAX_ERROR_BODY_PREFIX_BYTES;

/// What a capped diagnosis ends with. Re-exported for the same reason as
/// [`ERROR_BODY_CAP`]: a length bound alone is satisfied by a silent truncation, so
/// the announcement is half of what "capped" means and has to be assertable too.
#[cfg(feature = "claude-cli")]
pub const CAP_MARKER: &str = crate::error::TRUNCATION_MARKER;

/// Aborts the current test as NOT RUN, with the reason and the host.
///
/// # It panics rather than exiting zero, and that is the whole design
///
/// Under nextest each test is its own process, so `std::process::exit(0)` would be
/// reported as PASSED -- green by omission, with the marker invisible, because only
/// failing tests have their captured output shown. Panicking leaves it red with the
/// reason in view, satisfies the `-> !` on its own, and needs no `grep`
/// infrastructure around it. Fail-closed, which is the rule: nothing run is not
/// success.
///
/// The message carries the host, so the output nextest captures already contains
/// what a closing report has to record.
///
/// # The prefix is suppressed on a runner, and it SAYS SO when it is
///
/// Adjudicating a platform-dependent skip is a local, human step; on a runner there
/// must be nothing to adjudicate. `MAGI_CI` -- set and non-empty -- suppresses the
/// `CANNOT_TEST:` prefix, and the message then says `[prefix suppressed: MAGI_CI is
/// set]`, because a suppression nobody can see is worse than no suppression: a local
/// machine with the variable set for any reason would close the adjudication path
/// and show an ordinary red instead.
///
/// Non-empty is checked, not just presence: `MAGI_CI=` exported blank would
/// otherwise count as set. The project's own variable rather than a bare `CI`,
/// which half the industry exports -- a developer machine with it set for another
/// tool would change this guard's behaviour without anyone deciding so.
pub fn cannot_test(reason: &str) -> ! {
    let suppressed = std::env::var("MAGI_CI").is_ok_and(|v| !v.is_empty());
    let host = format!(
        "[host: {} {}]",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    if suppressed {
        panic!("{reason} {host} [prefix suppressed: MAGI_CI is set]");
    }
    panic!("CANNOT_TEST: {reason} {host}");
}

// ---------------------------------------------------------------------------
// CLAUDECODE, saved and restored around a closure.
//
// Moved here from `claude_cli.rs`'s private test module rather than copied: the
// integration tests live in another crate and could not see it there, and two
// copies of a helper with `unsafe` inside is the duplication this release removes
// in several other places -- the one that drifts is always the one nobody reads.
// ---------------------------------------------------------------------------

/// Saves `CLAUDECODE`, clears it, runs `f`, then restores the original value.
///
/// A development machine that is itself a Claude Code session sets this variable,
/// and `ClaudeCliProvider`'s constructors refuse to build under it. Every test that
/// constructs the provider therefore wraps the CONSTRUCTION in this.
///
/// # This mutates process-global state and is NOT thread-safe
///
/// `CLAUDECODE` belongs to the process, not to a thread, so two threads calling
/// this at once **corrupt the environment of the entire process** -- not just the
/// tests. The caller MUST serialise these calls. In this crate's own suite that is
/// `#[serial]` on each call site; a consumer enabling `test-utils` inherits the
/// obligation but not the attribute, which is why it is stated here.
///
/// The attribute deliberately does NOT travel with the helper: it marks the TEST,
/// not the function that mutates the environment, and `serial_test` is a
/// dev-dependency that would not exist for an external crate compiling this module
/// under `test-utils`.
pub fn without_claudecode<F: FnOnce()>(f: F) {
    let original = std::env::var("CLAUDECODE").ok();
    unsafe {
        std::env::remove_var("CLAUDECODE");
    }
    f();
    if let Some(val) = original {
        unsafe {
            std::env::set_var("CLAUDECODE", val);
        }
    }
}

/// Sets `CLAUDECODE`, runs `f`, then restores the original value.
///
/// # This mutates process-global state and is NOT thread-safe
///
/// Same contract as [`without_claudecode`], and the same damage if it is broken:
/// two threads calling either of these at once corrupt the environment of the
/// whole process. Serialise them.
/// # Its visibility SUPERSEDES a plan decision, and that is recorded rather than done quietly
///
/// The plan moved this helper and its twin out of a private test module as a PAIR and
/// declared the consequence in writing: both would be `pub` behind `test-utils`, and
/// an `unsafe` environment mutation would widen its reach with them. That decision was
/// approved on the premise that the integration tests needed both.
///
/// Measured after the move, they do not: `without_claudecode` is consumed from
/// `tests/` -- another crate -- and this one is called from exactly one place, a unit
/// test inside this crate. So the premise holds for one half of the pair and not the
/// other, which is the class of checkable-and-false claim this release exists to
/// correct.
///
/// Splitting the pair is the deviation, and it is taken because the rule stated below
/// is the stronger one: `docs.rs` builds with every feature on, so `pub` here means
/// published API under the stability policy, and surface with no outside consumer is
/// what the standards forbid.
#[cfg(all(test, feature = "claude-cli"))]
pub(crate) fn with_claudecode<F: FnOnce()>(f: F) {
    let original = std::env::var("CLAUDECODE").ok();
    unsafe {
        std::env::set_var("CLAUDECODE", "1");
    }
    f();
    if let Some(val) = original {
        unsafe {
            std::env::set_var("CLAUDECODE", val);
        }
    } else {
        unsafe {
            std::env::remove_var("CLAUDECODE");
        }
    }
}

// ---------------------------------------------------------------------------
// Captured CLI envelopes, and the mutations the CLI provider's tests need.
//
// The fixtures are CAPTURED from the real `claude --print --output-format json`
// and then redacted, never hand-written. A hand-written envelope reproduces the
// view the crate already has, which is exactly how R-26 survived: two rustdoc
// blocks said "verified" while what they had been checked against was
// `CliOutput` -- the crate's own view -- instead of the wire.
//
// The paths are relative to THIS file, so they carry the `providers/` segment
// that the unit tests inside `src/providers/claude_cli.rs` do not.
// ---------------------------------------------------------------------------

// Visibility here is NOT uniform, and the split is deliberate. `docs.rs` builds this
// crate with `all-features = true`, so anything `pub` behind `test-utils` renders as
// published API and falls under the stability policy the feature's own comment in
// `Cargo.toml` declares. What `tests/` -- a separate crate -- actually consumes stays
// `pub`; what only this crate's own unit tests reach is `pub(crate)`, and what only
// this module reaches is private. Surface without an outside consumer, under a
// stability guarantee, is the thing the release's standards forbid.
//
// The `pub(crate)` half also carries `#[cfg(test)]`, and that is not belt-and-braces:
// this module compiles under `test-utils` WITHOUT `cfg(test)`, and there a crate-visible
// item whose only callers are `#[cfg(test)]` unit tests is dead code that `-D warnings`
// rejects. Gating them on `cfg(test)` is what makes them exist exactly where they are
// used.

/// The captured failure envelope: `--model no-such-model-xyz`, redacted to the
/// three fields a REQ consumes.
pub const CAPTURED_404: &str = include_str!("providers/fixtures/envelopes/failure_404.json");

/// The captured success envelope, redacted to the four fields a REQ consumes.
const CAPTURED_SUCCESS: &str = include_str!("providers/fixtures/envelopes/success_end_turn.json");

/// Every key the redacted SUCCESS fixture is expected to carry, and no other.
///
/// Fixed by the capture spike on 2026-09-08 by reading the redacted file, never
/// from the raw capture: redaction removes fields, so a list written before it
/// counts keys that are gone and goes red for the fixture instead of the code.
#[cfg(all(test, feature = "claude-cli"))]
pub(crate) const SUCCESS_KEEP_LIST: [&str; 4] = ["is_error", "result", "stop_reason", "usage"];

/// Every key the redacted FAILURE fixture is expected to carry, and no other.
///
/// Three and not four, although the raw capture DOES carry `stop_reason`
/// (measured value: `"stop_sequence"`): a field consumed by the other envelope
/// type is removed anyway, because no REQ consumes `stop_reason` on a failure.
#[cfg(all(test, feature = "claude-cli"))]
pub(crate) const FAILURE_KEEP_LIST: [&str; 3] = ["api_error_status", "is_error", "result"];

/// The captured SUCCESS envelope with its `stop_reason` replaced.
///
/// Only that one field is synthetic; the shape stays the measured one.
///
/// # Panics
/// If the fixture no longer carries the anchor this substitution expects. That is
/// deliberate: a silent no-op would leave the caller asserting on an unmodified
/// envelope, which passes for the wrong reason.
pub fn captured_envelope_with_stop_reason(reason: &str) -> String {
    replace_once(
        CAPTURED_SUCCESS,
        "\"stop_reason\": \"end_turn\"",
        &format!("\"stop_reason\": \"{reason}\""),
    )
}

/// The captured FAILURE envelope with its `api_error_status` replaced by another
/// integer.
///
/// # Panics
/// As [`captured_envelope_with_stop_reason`].
pub fn captured_failure_with_api_error_status(status: i64) -> String {
    replace_once(
        CAPTURED_404,
        "\"api_error_status\": 404",
        &format!("\"api_error_status\": {status}"),
    )
}

/// The captured FAILURE envelope with `api_error_status` set to a raw JSON value
/// that is NOT an integer.
///
/// `value` is spliced in verbatim, so the caller controls the exact token --
/// which is what the overflow case needs: `99999999999999999999` is a perfectly
/// good JSON number that no integer type holds, and re-serialising it through a
/// parser would quietly turn it into something else.
///
/// # Panics
/// As [`captured_envelope_with_stop_reason`].
#[cfg(all(test, feature = "claude-cli"))]
pub(crate) fn captured_failure_with_unusable_api_error_status(value: &str) -> String {
    replace_once(
        CAPTURED_404,
        "\"api_error_status\": 404",
        &format!("\"api_error_status\": {value}"),
    )
}

/// The captured FAILURE envelope with `api_error_status` removed entirely.
///
/// A local CLI failure never reached the API, so the field is absent rather than
/// unreadable -- and those two are the distinction the three-state type exists to
/// keep.
///
/// # Panics
/// As [`captured_envelope_with_stop_reason`].
pub fn captured_failure_without_api_error_status() -> String {
    replace_once(
        CAPTURED_404,
        "  \"api_error_status\": 404,
",
        "",
    )
}

/// The captured FAILURE envelope with `is_error` flipped to `false`, KEEPING its
/// `api_error_status`.
///
/// The (a) edge: a status present on an envelope that does not claim failure.
/// `is_error` governs and the status is not read, so this must parse as a success.
///
/// # Panics
/// As [`captured_envelope_with_stop_reason`].
#[cfg(all(test, feature = "claude-cli"))]
pub(crate) fn envelope_with_is_error_false_and_status() -> String {
    replace_once(CAPTURED_404, "\"is_error\": true", "\"is_error\": false")
}

/// The captured FAILURE envelope with `is_error` flipped to `false` AND its
/// `api_error_status` REMOVED.
///
/// TWO fields, which is what separates it from its sibling above: without removing
/// the status the two functions would be the same and the (a) edge would lose its
/// own input. This one is what a process/envelope disagreement needs -- the envelope
/// says success while the process exits non-zero.
///
/// # Panics
/// As [`captured_envelope_with_stop_reason`].
pub fn envelope_with_is_error_false() -> String {
    let without_status = replace_once(CAPTURED_404, "  \"api_error_status\": 404,\n", "");
    replace_once(&without_status, "\"is_error\": true", "\"is_error\": false")
}

/// The captured FAILURE envelope with its `result` replaced by `bytes` of filler.
///
/// The cap assertion is VACUOUS against the real fixture: its `result` is two lines,
/// so `len() <= 8 KiB` holds with the truncation deleted. Only a `result` that
/// EXCEEDS the cap can tell a working cap from an absent one.
///
/// # Panics
/// As [`captured_envelope_with_stop_reason`].
#[cfg(all(test, feature = "claude-cli"))]
pub(crate) fn captured_failure_with_oversized_result(bytes: usize) -> String {
    let value: serde_json::Value =
        serde_json::from_str(CAPTURED_404).expect("the captured fixture parses");
    let mut object = match value {
        serde_json::Value::Object(map) => map,
        other => panic!("the captured fixture is an object, got {other}"),
    };
    object.insert(
        "result".to_string(),
        serde_json::Value::String("R".repeat(bytes)),
    );
    serde_json::Value::Object(object).to_string()
}

/// The `result` field of an envelope, as the crate would read it.
///
/// Tests assert against what the envelope SAYS rather than against a literal copied
/// beside them: a copy drifts from the fixture and the assertion then pins the copy.
///
/// # Panics
/// If `raw` is not an object with a string `result`.
pub fn result_of(raw: &str) -> String {
    let value: serde_json::Value =
        serde_json::from_str(raw).expect("the envelope under test parses");
    value
        .get("result")
        .and_then(|r| r.as_str())
        .expect("the envelope under test carries a string `result`")
        .to_string()
}

/// Substitutes `needle` once, and panics if it is not there.
///
/// The panic is the point: a substitution that silently does nothing hands the
/// caller an envelope it did not ask for, and the assertion then passes or fails
/// for a reason nobody chose.
fn replace_once(haystack: &str, needle: &str, replacement: &str) -> String {
    assert!(
        haystack.contains(needle),
        "the captured fixture no longer contains `{needle}`; the substitution would be a no-op"
    );
    haystack.replacen(needle, replacement, 1)
}

/// Mock provider that routes `complete()` calls to per-agent response
/// sequences using the `CURRENT_AGENT_IDENTITY` task-local set by
/// [`crate::agent::Agent::execute`]. Fails closed if no task-local scope
/// is active.
///
/// Production providers (Claude HTTP, Claude CLI) ignore the task-local;
/// they never read it. This mock uses it for deterministic test routing
/// without parsing the system prompt or polluting `CompletionConfig`.
///
/// # Example
///
/// ```ignore
/// use magi_core::test_support::RoutingMockProvider;
/// use magi_core::schema::AgentName;
///
/// let provider = RoutingMockProvider::new()
///     .with_agent_responses(
///         AgentName::Melchior,
///         vec![Ok("first".to_string()), Ok("second".to_string())],
///     );
/// // When invoked from inside CURRENT_AGENT_IDENTITY.scope(Melchior, ...),
/// // the first call returns "first", the second returns "second".
/// ```
pub struct RoutingMockProvider {
    sequences: Mutex<HashMap<AgentName, Vec<Result<String, ProviderError>>>>,
}

impl RoutingMockProvider {
    /// Creates an empty routing mock with no agent sequences registered.
    pub fn new() -> Self {
        Self {
            sequences: Mutex::new(HashMap::new()),
        }
    }

    /// Registers a FIFO response sequence for the given agent.
    ///
    /// Responses are consumed in order on subsequent `complete()` calls
    /// scoped to this agent. Errors injected via `Err(ProviderError::...)`
    /// surface to the caller verbatim.
    pub fn with_agent_responses(
        self,
        agent: AgentName,
        responses: Vec<Result<String, ProviderError>>,
    ) -> Self {
        self.sequences.lock().unwrap().insert(agent, responses);
        self
    }
}

impl Default for RoutingMockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for RoutingMockProvider {
    async fn complete(
        &self,
        _system_prompt: &str,
        _user_prompt: &str,
        _config: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        let identity =
            CURRENT_AGENT_IDENTITY
                .try_with(|name| *name)
                .map_err(|_| ProviderError::Process {
                    exit_code: None,
                    stderr: "RoutingMockProvider: CURRENT_AGENT_IDENTITY not in scope; \
                         caller must wrap the call in `Agent::execute` or \
                         `CURRENT_AGENT_IDENTITY.scope(...)`"
                        .to_string(),
                })?;

        let mut sequences = self.sequences.lock().unwrap();
        let seq = sequences
            .get_mut(&identity)
            .ok_or_else(|| ProviderError::Process {
                exit_code: None,
                stderr: format!("RoutingMockProvider: no sequence registered for {identity:?}"),
            })?;

        if seq.is_empty() {
            return Err(ProviderError::Process {
                exit_code: None,
                stderr: format!("RoutingMockProvider: sequence exhausted for {identity:?}"),
            });
        }
        Ok(Completion::new(seq.remove(0)?))
    }

    fn name(&self) -> &str {
        "routing-mock"
    }

    fn model(&self) -> &str {
        "test"
    }
}

// ---------------------------------------------------------------------------
// Rotation test support: ScriptProvider, MockProbe, and thin trio builders.
// ---------------------------------------------------------------------------

/// Content that is delimited correctly but is **not JSON** — so it exercises the
/// `InvalidJson` cause, which is the failure a cooperative-but-sloppy model actually
/// produces.
///
/// Wrapped in the markers on purpose. Left bare, this would fail at delimitation with
/// `MissingMarkers` and never reach `serde_json`, so a variant named `BadJson` would test
/// the *absence* of markers instead of bad JSON — and the `InvalidJson` rotation path
/// would go unexercised by the shared helpers. `MissingMarkers` is already covered
/// exhaustively by the unit tests, so no bare-body variant is added here: nothing would
/// consume it.
static BAD_JSON: LazyLock<String> =
    LazyLock::new(|| format!("{VERDICT_OPEN}\nnot json at all\n{VERDICT_CLOSE}"));

/// A block that OPENS and never closes — the signature of a response cut off mid-flight.
///
/// The JSON inside is deliberately well-formed and complete: what is missing is only the
/// closing marker, so the failure is `Unterminated` and not `InvalidJson`. A truncated body
/// with mangled JSON would fail for the nearer reason and never exercise the sentinel's
/// unterminated arm at all.
static TRUNCATED: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{VERDICT_OPEN}\n{{\"agent\":\"caspar\",\"verdict\":\"approve\",\"confidence\":0.9,\
         \"summary\":\"ok\",\"reasoning\":\"r\",\"recommendation\":\"go\",\"findings\":[]}}"
    )
});

/// AGENT-AWARE valid verdict body. Reads the `CURRENT_AGENT_IDENTITY` task-local
/// (set by [`crate::agent::Agent::execute`]/`execute_with`) and emits a verdict
/// whose `agent` field MATCHES the launched mage — so a shared-pool fallback
/// serving whichever mage rotated to it produces the correct identity.
///
/// `AgentName` is `#[serde(rename_all = "lowercase")]`, so the wire token is
/// lowercase (`"caspar"`), NOT `display_name()` (`"Caspar"`). We serialize the
/// enum to get the exact token — a hand-written `"Caspar"` would fail to
/// deserialize.
///
/// # Delimited with the verdict markers (3.0.0)
///
/// The body is wrapped in [`VERDICT_OPEN`]/[`VERDICT_CLOSE`], each alone on its line,
/// because since 3.0.0 the parser reads **only** what lies between them — a bare object
/// is `MissingMarkers`, not a verdict. A mock that returns bare JSON therefore no longer
/// models a *cooperative* provider; it models one that ignores the contract. This helper
/// is the single place that shape is defined, so the rotation and probe suites express
/// "this attempt succeeds" without each restating the wire format.
pub fn valid_verdict_for_current_agent() -> String {
    let who = CURRENT_AGENT_IDENTITY
        .try_with(|a| *a)
        .unwrap_or(AgentName::Melchior);
    let agent = serde_json::to_value(who)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "melchior".into());
    format!(
        "{VERDICT_OPEN}\n{{\"agent\":\"{agent}\",\"verdict\":\"approve\",\"confidence\":0.9,\
         \"summary\":\"ok\",\"reasoning\":\"r\",\"recommendation\":\"go\",\"findings\":[]}}\n\
         {VERDICT_CLOSE}"
    )
}

/// One behavior per attempt index; the last entry repeats for further calls.
#[derive(Clone)]
pub enum Beh {
    /// Return a valid, agent-aware verdict, with a MEASURED telemetry attached.
    ///
    /// Measured rather than blank because the success path's whole job is copying that
    /// telemetry into the report; with `unmeasured()` every field a test could look at is
    /// `None`, so the copy is observable only in a unit test of the conversion and never end
    /// to end. The values model an ordinary completion: it ended on its own (`Stop`), well
    /// under its budget, having reasoned a little.
    Ok,
    /// Return a valid, agent-aware verdict that was nonetheless CUT at the output budget.
    ///
    /// The case the report's two telemetry maps are required to keep DISJOINT: extraction
    /// succeeded, so nothing belongs in `extraction_failures`, while `completions` must still
    /// record that the answer arrived at the ceiling. Scripted because that disjointness is a
    /// property of the orchestrator filling both maps, and a test that inserts into one of
    /// them by hand asserts only its own fixture.
    OkAtTheCap,
    /// Surface `ProviderError::Network` (connection-level → counts toward
    /// endpoint-down).
    Network,
    /// Surface `ProviderError::Http { status: 503 }` (transport, NON-connection).
    Http5xx,
    /// Panic the task (never rotates — surfaces as a failure).
    Panic,
    /// Return a correctly delimited block whose content is **not JSON** (`InvalidJson`
    /// → schema failure → mage-local rotation). Delimited rather than bare on purpose: a
    /// bare body would fail at delimitation with `MissingMarkers` and never reach
    /// `serde_json`, so this variant would test the *absence* of markers instead of bad
    /// JSON. The full reasoning is on the private `BAD_JSON` constant.
    BadJson,
    /// Return a block that OPENS and never closes (`Unterminated` → schema failure →
    /// mage-local rotation).
    ///
    /// The JSON inside is complete on purpose: only the closing marker is missing, so the
    /// failure is the sentinel's unterminated arm and not the nearer `InvalidJson` one.
    Truncated,
    /// Surface `ProviderError::NoGeneration` — the footprint of a request THIS CRATE built
    /// wrongly, which the backend accepted and did not generate from.
    ///
    /// Scripted so the abort can be observed end to end. It is the one behaviour here whose
    /// consequence is the whole RUN rather than one seat, and reading that from a report
    /// field is impossible — the run produces no report at all.
    NoGeneration,
    /// Surface `ProviderError::EmptyCompletion` — the model returned no content because it
    /// spent the whole output budget before producing any.
    ///
    /// The headline failure of `4.0.0`, and the one whose CONSEQUENCE changed: it is
    /// mage-local, so the seat rotates and the lineage stays available to the other two.
    /// Scripted because that consequence can only be observed by watching the registry
    /// across a real rotation, not by classifying an error in isolation.
    EmptyCompletion,
    /// Surface `ProviderError::ResponseTooLarge`.
    ///
    /// A CONTENT failure that looks superficially like transport: the server answered fine, it
    /// answered too much. Scripted so the mage-local consequence can be observed end to end
    /// rather than inferred from the variant.
    Oversized,
    /// Surface `ProviderError::external(.., ExternalErrorKind::Network)`.
    ///
    /// The `Network` shape is chosen deliberately: it is the one an external provider is most
    /// likely to report, and the one whose IN-CRATE twin (`Beh::Network`) trips the endpoint-down
    /// latch. Running both over the SAME topology is what proves the scope difference is real
    /// rather than incidental.
    External,
}

/// A provider whose behavior is FIXED per instance and scripted per call index.
/// The pool assigns a distinct provider per lineage, so "which model ran" equals
/// "which provider was called" — the [`ScriptProvider::calls`] counter proves a
/// wrapped `RetryProvider` actually retried before the FSM rotated.
pub struct ScriptProvider {
    name: String,
    model: String,
    script: Vec<Beh>,
    calls: AtomicUsize,
}

impl ScriptProvider {
    /// Builds a scripted provider (returned as `Arc` for direct use as a
    /// provider handle). `script` behaviors are consumed by call index; the last
    /// entry repeats.
    pub fn new(model: &str, script: Vec<Beh>) -> Arc<Self> {
        Arc::new(Self {
            name: format!("mock-{model}"),
            model: model.into(),
            script,
            calls: AtomicUsize::new(0),
        })
    }

    /// Number of `complete` calls this provider received.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl LlmProvider for ScriptProvider {
    async fn complete(
        &self,
        _s: &str,
        _u: &str,
        config: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        let i = self.calls.fetch_add(1, Ordering::SeqCst);
        let beh = self
            .script
            .get(i)
            .or_else(|| self.script.last())
            .cloned()
            .unwrap_or(Beh::Ok);
        match beh {
            Beh::Ok => Ok(
                Completion::new(valid_verdict_for_current_agent()).with_telemetry(
                    crate::provider::CompletionTelemetry::unmeasured()
                        .with_finish(crate::provider::FinishReason::Stop)
                        .with_completion_tokens(512)
                        .with_prompt_tokens(1_024)
                        .with_reasoning(crate::provider::ReasoningState::Measured {
                            chars: 64,
                            text: None,
                        }),
                ),
            ),
            Beh::OkAtTheCap => Ok(Completion::new(valid_verdict_for_current_agent())
                .with_telemetry(
                    crate::provider::CompletionTelemetry::unmeasured()
                        .with_finish(crate::provider::FinishReason::Length)
                        .with_completion_tokens(config.max_tokens)
                        .with_reasoning(crate::provider::ReasoningState::Measured {
                            chars: 4_096,
                            text: None,
                        }),
                )),
            Beh::BadJson => Ok(Completion::new(BAD_JSON.clone())),
            Beh::Truncated => Ok(Completion::new(TRUNCATED.clone())),
            // Modelled on `resp-C.json`, the capture this milestone is named after: the model
            // spent the WHOLE budget reasoning and emitted nothing, so `completion_tokens`
            // equals the cap and the reasoning measurement is non-zero. A double that returned
            // a bare termination reason would let an integration test believe the empty path
            // carries no measurement -- which is precisely the defect being fixed.
            Beh::EmptyCompletion => Err(ProviderError::EmptyCompletion {
                telemetry: crate::provider::CompletionTelemetry::unmeasured()
                    .with_finish(crate::provider::FinishReason::Length)
                    .with_completion_tokens(config.max_tokens)
                    .with_reasoning(crate::provider::ReasoningState::Measured {
                        chars: 15_409,
                        text: None,
                    }),
                // READ, never invented: a double that hardcodes the budget lets a test
                // configure a different one, assert the record carries it, and pass on a
                // number the production path never chose.
                cap: config.max_tokens,
            }),
            Beh::NoGeneration => Err(ProviderError::NoGeneration {
                // The value the one captured case carried. Named rather than `None` so the
                // scripted footprint matches the real one it stands for.
                done_reason: Some(crate::provider::FinishReason::Load),
            }),
            Beh::Network => Err(ProviderError::Network {
                message: "connection refused".into(),
            }),
            Beh::Http5xx => Err(ProviderError::Http {
                status: 503,
                body: String::new(),
                retry_after_raw: vec![],
                received_at: None,
            }),
            Beh::Oversized => Err(ProviderError::ResponseTooLarge { limit: 1 << 20 }),
            Beh::External => Err(ProviderError::external(
                "third-party backend unreachable",
                ExternalErrorKind::Network,
            )),
            Beh::Panic => panic!("mock panic"),
        }
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn model(&self) -> &str {
        &self.model
    }
}

/// An Ollama-like provider that is BOTH an [`LlmProvider`] and a
/// [`ProviderProbe`], for probe/verify tests (window + digest).
pub struct MockProbe {
    name: String,
    model: String,
    window: Option<usize>,
    digest: Option<String>,
}

impl MockProbe {
    /// A probe with a fixed digest and a comfortable 200k window.
    pub fn with_digest(model: &str, d: &str) -> Arc<Self> {
        Arc::new(Self {
            name: format!("probe-{model}"),
            model: model.into(),
            window: Some(200_000),
            digest: Some(d.into()),
        })
    }
    /// A probe with a configurable window (`None` = unmeasurable) and a
    /// model-derived digest.
    pub fn with_window(model: &str, w: Option<usize>) -> Arc<Self> {
        Arc::new(Self {
            name: format!("probe-{model}"),
            model: model.into(),
            window: w,
            digest: Some(format!("sha:{model}")),
        })
    }
}

#[async_trait]
impl LlmProvider for MockProbe {
    async fn complete(
        &self,
        _s: &str,
        _u: &str,
        _c: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        Ok(Completion::new(valid_verdict_for_current_agent()))
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn model(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl ProviderProbe for MockProbe {
    async fn window(&self) -> Result<Option<usize>, ProviderError> {
        Ok(self.window)
    }
    async fn digest(&self) -> Result<Option<String>, ProviderError> {
        Ok(self.digest.clone())
    }
}

/// A `ScriptProvider` that always succeeds — the default happy provider.
fn ok(model: &str) -> Arc<ScriptProvider> {
    ScriptProvider::new(model, vec![Beh::Ok])
}

/// A trio where Melchior/Balthasar succeed and Caspar uses `caspar`; `pool` is a
/// list of `(model, lineage)` fallbacks with `max_rotations = 2`.
pub fn build_trio_with_caspar(
    caspar: Arc<dyn LlmProvider>,
    pool: Vec<(&'static str, &'static str)>,
) -> Magi {
    let mut pb = FallbackPool::builder().max_rotations(2);
    for (model, lin) in pool {
        pb = pb.push(ScriptProvider::new(model, vec![Beh::Ok]), Lineage::new(lin));
    }
    MagiBuilder::new(ok("default") as Arc<dyn LlmProvider>)
        .with_agent(
            AgentName::Melchior,
            ok("m-alibaba"),
            Lineage::new("alibaba"),
        )
        .with_agent(
            AgentName::Balthasar,
            ok("m-moonshot"),
            Lineage::new("moonshot"),
        )
        .with_agent(AgentName::Caspar, caspar, Lineage::new("deepseek"))
        .with_fallback_pool(pb.build())
        .build()
        .unwrap()
}

/// Caspar's deepseek primary returns `BadJson` on both attempt and corrective
/// retry → schema fail (mage-local) → rotate to the pool. deepseek is never
/// condemned run-wide (schema ≠ transport), which an integration test asserts.
pub fn build_schema_local_case() -> Magi {
    build_trio_with_caspar(
        ScriptProvider::new("deepseek", vec![Beh::BadJson]),
        vec![("glm", "zhipu")],
    )
}

/// Melchior + Caspar both transport-fail with `Http 5xx` (NON-connection, so the
/// endpoint-down latch never fires — that is S13's job). Only ONE free fallback
/// lineage (`zhipu`) exists → exactly one of them reserves it, the other gets
/// `no_fitting_candidate`. Diversity is preserved (never a duplicated lineage).
pub fn build_two_failing_with_single_free_fallback() -> Magi {
    MagiBuilder::new(ok("default") as Arc<dyn LlmProvider>)
        .with_agent(
            AgentName::Melchior,
            ScriptProvider::new("m-alibaba", vec![Beh::Http5xx]),
            Lineage::new("alibaba"),
        )
        .with_agent(
            AgentName::Balthasar,
            ok("m-moonshot"),
            Lineage::new("moonshot"),
        )
        .with_agent(
            AgentName::Caspar,
            ScriptProvider::new("m-deepseek", vec![Beh::Http5xx]),
            Lineage::new("deepseek"),
        )
        .with_fallback_pool(
            FallbackPool::builder()
                .push(ok("glm"), Lineage::new("zhipu"))
                .max_rotations(2)
                .build(),
        )
        .build()
        .unwrap()
}

/// Melchior + Caspar Network-fail (connection) with an EMPTY pool → the
/// endpoint-down latch fires at the 2nd distinct connection lineage (S13).
pub fn build_two_network_failing_no_fallback() -> Magi {
    MagiBuilder::new(ok("default") as Arc<dyn LlmProvider>)
        .with_agent(
            AgentName::Melchior,
            ScriptProvider::new("m-alibaba", vec![Beh::Network]),
            Lineage::new("alibaba"),
        )
        .with_agent(
            AgentName::Balthasar,
            ok("m-moonshot"),
            Lineage::new("moonshot"),
        )
        .with_agent(
            AgentName::Caspar,
            ScriptProvider::new("m-deepseek", vec![Beh::Network]),
            Lineage::new("deepseek"),
        )
        // Empty pool → rotation is engaged (registry built) but there is nothing
        // to rotate to; the connection failures still trip endpoint-down.
        .with_fallback_pool(FallbackPool::builder().build())
        .build()
        .unwrap()
}

/// One seat hits an oversized response and rotates to another lineage; the other two are healthy.
///
/// Mirrors the schema-failure builder rather than the transport one, because that is the claim:
/// an oversized body condemns MAGE-LOCAL, so the lineage stays available to the other seats.
pub fn build_oversized_case() -> Magi {
    build_trio_with_caspar(
        ScriptProvider::new("deepseek", vec![Beh::Oversized]),
        vec![("glm", "zhipu")],
    )
}

/// The TWIN of [`build_two_network_failing_no_fallback`], differing in exactly one thing: the two
/// failing seats report `ProviderError::External` instead of `ProviderError::Network`.
///
/// Everything else — the lineages, the empty pool, the healthy middle seat — is identical on
/// purpose. The endpoint-down latch fires for the twin and must NOT fire here, and holding every
/// other variable still is what makes that difference attributable to the error class rather than
/// to the topology.
pub fn build_two_external_failing_no_fallback() -> Magi {
    MagiBuilder::new(ok("default") as Arc<dyn LlmProvider>)
        .with_agent(
            AgentName::Melchior,
            ScriptProvider::new("m-alibaba", vec![Beh::External]),
            Lineage::new("alibaba"),
        )
        .with_agent(
            AgentName::Balthasar,
            ok("m-moonshot"),
            Lineage::new("moonshot"),
        )
        .with_agent(
            AgentName::Caspar,
            ScriptProvider::new("m-deepseek", vec![Beh::External]),
            Lineage::new("deepseek"),
        )
        .with_fallback_pool(FallbackPool::builder().build())
        .build()
        .unwrap()
}

/// Melchior + Caspar `Http 5xx` (NON-connection) with TWO free local fallbacks →
/// both rotate successfully, no endpoint-down (S14).
pub fn build_two_5xx_with_local_fallbacks() -> Magi {
    MagiBuilder::new(ok("default") as Arc<dyn LlmProvider>)
        .with_agent(
            AgentName::Melchior,
            ScriptProvider::new("m-alibaba", vec![Beh::Http5xx]),
            Lineage::new("alibaba"),
        )
        .with_agent(
            AgentName::Balthasar,
            ok("m-moonshot"),
            Lineage::new("moonshot"),
        )
        .with_agent(
            AgentName::Caspar,
            ScriptProvider::new("m-deepseek", vec![Beh::Http5xx]),
            Lineage::new("deepseek"),
        )
        .with_fallback_pool(
            FallbackPool::builder()
                .push(ok("glm"), Lineage::new("zhipu"))
                .push(ok("minimax"), Lineage::new("minimax-lin"))
                .max_rotations(2)
                .build(),
        )
        .build()
        .unwrap()
}

/// Test helper: the set of lineages that some agent **left via a completed
/// TRANSPORT/TIMEOUT rotation hop**, read from telemetry. This is a *subset* of the
/// run's transport-condemned lineages — a lineage condemned by a mage that could
/// NOT rotate away from it (no eligible fallback) leaves no hop and so does not
/// appear here. Schema hops are mage-local and are excluded. Sufficient for the
/// tests that assert a schema-failed lineage is NOT transport-condemned run-wide.
pub fn report_run_failed(report: &MagiReport) -> std::collections::BTreeSet<Lineage> {
    report
        .rotations
        .values()
        .flat_map(|r| r.chain.iter())
        .filter(|e| matches!(e.kind(), RotationKind::Transport | RotationKind::Timeout))
        .map(|e| e.from().clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_routing_mock_provider_routes_by_task_local_identity() {
        let mp = RoutingMockProvider::new()
            .with_agent_responses(
                AgentName::Melchior,
                vec![Ok("MEL_1".to_string()), Ok("MEL_2".to_string())],
            )
            .with_agent_responses(AgentName::Balthasar, vec![Ok("BAL_1".to_string())]);
        let cfg = CompletionConfig::default();

        let r1 = CURRENT_AGENT_IDENTITY
            .scope(AgentName::Melchior, mp.complete("sys", "x", &cfg))
            .await
            .unwrap();
        assert_eq!(r1.text, "MEL_1");

        let r2 = CURRENT_AGENT_IDENTITY
            .scope(AgentName::Balthasar, mp.complete("sys", "x", &cfg))
            .await
            .unwrap();
        assert_eq!(r2.text, "BAL_1");

        let r3 = CURRENT_AGENT_IDENTITY
            .scope(AgentName::Melchior, mp.complete("sys", "x", &cfg))
            .await
            .unwrap();
        assert_eq!(r3.text, "MEL_2");
    }

    #[tokio::test]
    async fn test_routing_mock_provider_fails_when_no_task_local_scope() {
        let mp = RoutingMockProvider::new()
            .with_agent_responses(AgentName::Caspar, vec![Ok("CAS_1".to_string())]);
        let cfg = CompletionConfig::default();
        // NO scope around the call — task-local not in scope
        let r = mp.complete("sys", "x", &cfg).await;
        assert!(
            matches!(r, Err(ProviderError::Process { .. })),
            "must fail-closed if CURRENT_AGENT_IDENTITY not in scope; got {r:?}"
        );
    }

    #[tokio::test]
    async fn test_routing_mock_provider_exhausted_sequence_errors() {
        let mp = RoutingMockProvider::new()
            .with_agent_responses(AgentName::Caspar, vec![Ok("CAS_1".to_string())]);
        let cfg = CompletionConfig::default();
        let _ = CURRENT_AGENT_IDENTITY
            .scope(AgentName::Caspar, mp.complete("s", "x", &cfg))
            .await
            .unwrap();
        let r = CURRENT_AGENT_IDENTITY
            .scope(AgentName::Caspar, mp.complete("s", "x", &cfg))
            .await;
        assert!(matches!(r, Err(ProviderError::Process { .. })), "got {r:?}");
    }

    #[tokio::test]
    async fn test_routing_mock_provider_can_inject_provider_errors() {
        let mp = RoutingMockProvider::new().with_agent_responses(
            AgentName::Melchior,
            vec![
                Err(ProviderError::Timeout {
                    message: "t".to_string(),
                }),
                Ok("MEL_2".to_string()),
            ],
        );
        let cfg = CompletionConfig::default();
        let r1 = CURRENT_AGENT_IDENTITY
            .scope(AgentName::Melchior, mp.complete("s", "x", &cfg))
            .await;
        assert!(matches!(r1, Err(ProviderError::Timeout { .. })));
        let r2 = CURRENT_AGENT_IDENTITY
            .scope(AgentName::Melchior, mp.complete("s", "x", &cfg))
            .await
            .unwrap();
        assert_eq!(r2.text, "MEL_2");
    }

    /// Invariant — each prompt file still contains the agent
    /// role marker. Not load-bearing for routing (we use task-local now),
    /// but keeps the option open for marker-based detection in downstream
    /// mock providers.
    #[test]
    fn test_each_prompt_file_contains_agent_role_marker() {
        assert!(crate::prompts::melchior_prompt().contains("Melchior"));
        assert!(crate::prompts::balthasar_prompt().contains("Balthasar"));
        assert!(crate::prompts::caspar_prompt().contains("Caspar"));
    }

    #[test]
    fn test_every_scripted_body_fails_and_succeeds_where_its_name_claims() {
        use crate::verdict_markers::{ExtractionFailureCause, extract};

        // Both causes map to `Deserialization`, so every rotation test passes either
        // way — which is exactly how the meaning of `Beh::BadJson` drifted from "bad
        // JSON" to "no markers" unnoticed when the wire format changed. Pinning the
        // cause is what makes the drift loud instead of silent.
        let block = extract(&BAD_JSON).expect("BAD_JSON must be correctly delimited");
        assert!(
            serde_json::from_str::<crate::schema::AgentOutput>(block).is_err(),
            "BAD_JSON must fail INSIDE the markers (InvalidJson), not at delimitation"
        );

        let ok = valid_verdict_for_current_agent();
        let block = extract(&ok).expect("the success body must be correctly delimited");
        serde_json::from_str::<crate::schema::AgentOutput>(block)
            .expect("the success body must deserialize as a full 7-key verdict");

        // TRUNCATED is the THIRD scripted body and the newest, so it is the one whose
        // meaning is likeliest to drift -- and its drift is invisible: `Unterminated` and
        // `MissingMarkers` both land on a schema failure, so every rotation test passes
        // either way. That is precisely how `Beh::BadJson` drifted from "bad JSON" to "no
        // markers" unnoticed when the wire format changed. Pinning is what makes it loud.
        assert_eq!(
            extract(&TRUNCATED).unwrap_err().cause(),
            ExtractionFailureCause::Unterminated,
            "TRUNCATED must fail because the CLOSING marker is missing, not because              delimitation never started"
        );

        // And the guard rail for the reverse drift: a bare body no longer models a
        // cooperative provider at all.
        assert_eq!(
            extract("not json at all").unwrap_err().cause(),
            ExtractionFailureCause::MissingMarkers
        );
    }
}
