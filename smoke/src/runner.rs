// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! The shared-run executor, and the contract between runs and scenarios.
//!
//! **One run, many assertions.** Runs are the expensive thing here — each one
//! calls `analyze()` against a real backend — so scenarios do not each get their
//! own. A handful of runs execute once, and every scenario READS one of them.
//!
//! That is cheap only if it is also honest, which is what [`RunContext`] is for:
//! an assertion sees everything a run produced and nothing it did not, and has
//! no way to reach the network on its own.
//!
//! # Two of this task's specified tests live elsewhere, deliberately
//!
//! The plan lists five tests here, two of which assert over a `render_rows`
//! function that renders the results TABLE. That function belongs to the
//! reporting layer, not to the executor, and building it here would have put the
//! table in two places. They are implemented with the report module instead; a
//! reader looking for them here has not found a regression.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::alias::magi_core::error::MagiError;
use crate::alias::magi_core::orchestrator::{Magi, MagiBuilder};
use crate::alias::magi_core::provider::{CompletionConfig, LlmProvider, ReasoningControl};
use crate::alias::magi_core::providers::ollama::OllamaProvider;
use crate::alias::magi_core::reporting::MagiReport;
use crate::alias::magi_core::rotation::{FallbackPool, Lineage};
use crate::alias::magi_core::schema::{AgentName, Mode};
use crate::config::{Config, Fallback, RunId, Seat};
use crate::external;
use crate::outcome::{run_catching, RunOutcome, ScenarioState};
use crate::payload::{self, Payload, PayloadError};
use crate::proxy::{Injection, RequestRecord, SpyProxy};

/// One assertion's result, with the sentence it was asserting.
///
/// A scenario returns a **vector** of these rather than a single state: one of
/// them has four, and collapsing four into one state would make a red row unable
/// to say WHICH property failed — the one thing that row exists to distinguish.
#[derive(Debug, Clone)]
pub struct Assertion {
    /// The property, written as a sentence a reader can check against the code.
    pub name: &'static str,
    /// Whether it held, could not be tested, or was out of scope.
    pub state: ScenarioState,
}

impl Assertion {
    /// The assertion could not be tested, and says why.
    ///
    /// # Parameters
    ///
    /// * `name` — the property that went untested.
    /// * `reason` — what prevented it, in terms an operator can act on.
    pub fn skip(name: &'static str, reason: impl Into<String>) -> Self {
        Assertion {
            name,
            state: ScenarioState::Skip(reason.into()),
        }
    }

    /// This invocation never asked the question.
    ///
    /// # The line between this and [`Assertion::skip`], which decides the exit code
    ///
    /// `Skip` means *I tried and could not test* — exit 2, a fault of ours that
    /// somebody should look at. `OutOfScope` means *this run was not asked to*
    /// — exit 0, nothing to look at.
    ///
    /// Four scenarios here assert what happens when a preflight STAGE fails, and
    /// one asserts what the feature matrix does; none of those events occurs
    /// unless the invocation induces it. Reporting them as `Skip` made every
    /// healthy run exit 2 over faults that did not happen, which meant exit 0
    /// was unreachable by construction and "the harness is green" could not be
    /// demonstrated by running it.
    ///
    /// **It carries no reason, and `Skip` does, deliberately.** A skip's reason
    /// is the only field an operator can act on and it differs every time; an
    /// out-of-scope row always says the same thing — this invocation did not ask
    /// — so a per-call string would be ceremony that can drift from the truth.
    ///
    /// # Parameters
    ///
    /// * `name` — the property this run was not asked to check.
    pub fn out_of_scope(name: &'static str) -> Self {
        Assertion {
            name,
            state: ScenarioState::OutOfScope,
        }
    }
}

/// Builds a `Pass`/`Fail` assertion from a condition.
///
/// There is deliberately no `From<bool>` anywhere near this: naming the property
/// at the call site is what makes a red row readable.
///
/// # Parameters
///
/// * `name` — the property being asserted.
/// * `held` — whether it held.
pub fn assert_that(name: &'static str, held: bool) -> Assertion {
    Assertion {
        name,
        state: if held {
            ScenarioState::Pass
        } else {
            ScenarioState::Fail
        },
    }
}

/// Which of the two things a typed failure from `analyze()` can mean.
///
/// The distinction exists because both arrive as an `Err` from the same call and
/// only the variant tells them apart: the crate breaking is a verdict about the
/// crate, while the crate reporting that its backend died is the crate working.
/// Collapsing them costs an exit code in one direction or the other, and both
/// directions have already been paid for once — see
/// `scenarios::e1`'s `analyze_produced_a_report`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// The crate's own logic produced the failure. A verdict, exit 1.
    CrateFailure,
    /// The crate correctly REPORTED a failure of the environment around it —
    /// no reachable endpoint, or too few seats left standing. Not a verdict.
    Environment,
}

/// What a scenario gets to look at: everything a run produced, and NOTHING it
/// did not.
///
/// An assertion cannot reach the network on its own, which is what makes "one
/// run, many assertions" cheap AND honest.
pub struct RunContext<'a> {
    /// Which shared run fed this assertion.
    pub run: RunId,
    /// `None` when the run produced no report. Read it TOGETHER with `error`
    /// and `error_class`: `None` + `Some(error)` is a typed failure, whose
    /// CLASS decides whether it is a verdict about the crate (FAIL) or the
    /// crate correctly reporting its environment (SKIP); `None` + `None` is a
    /// run that never happened (SKIP).
    ///
    /// **The split is implemented, not merely described.** It states the exit
    /// code a scenario must produce, and three scenarios skipped both cases —
    /// so the crate failing in a typed way, which is what this harness came to
    /// find, left with exit 2, "a fault of ours". `scenarios::e1`'s
    /// `analyze_produced_a_report` is the shared implementation, and it explains
    /// why the verdict rides as its own row instead of turning the property
    /// assertions red: [`ScenarioState::Fail`] carries no text, and the error is
    /// the only thing an operator can act on.
    pub report: Option<&'a MagiReport>,
    /// A typed failure from `analyze()`, rendered.
    ///
    /// **Only ever the crate's**, by the time a scenario reads it.
    /// `main::evaluate` intercepts every other reason a report can be absent
    /// before the assertion runs — a run that could not START becomes a skip
    /// naming our own configuration fault, a run out of time becomes a TIME
    /// row, and either panic outcome carries no error at all. Without that
    /// interception this field was ambiguous, and a configuration fault was
    /// once read as though `analyze()` had returned it.
    pub error: Option<&'a str>,
    /// How `error` must be READ. `Some` exactly when `error` is.
    pub error_class: Option<ErrorClass>,
    /// Everything the proxy saw on the wire during THIS run.
    pub records: &'a [RequestRecord],
    /// True if the proxy degraded. Assertions that read `records` must SKIP,
    /// because a partial registry could fail an assertion the crate satisfied
    /// perfectly.
    pub proxy_degraded: bool,
    /// `Some` only when the run hit its time cap: the cap itself, so a reader
    /// can tell a TIME failure from an assertion failure and knows which number
    /// to raise. Never an overrun — the run was cut, so no overrun was measured.
    pub budget_exceeded: Option<Duration>,
    /// The response to the DIRECT half of the transparency probe — the term of
    /// comparison. `None` on every run but the one that primed it.
    pub direct_probe_body: Option<&'a [u8]>,
    /// The HTTP STATUS the direct half got back, the second term of comparison.
    ///
    /// It travels beside the body because transparency is a claim about both:
    /// a proxy that relayed the right bytes under a different status would have
    /// satisfied a body-only comparison while changing what the crate sees.
    pub direct_probe_status: Option<u16>,
    /// The record of the probe request that went THROUGH the proxy.
    ///
    /// It travels separately rather than inside `records` because the probe runs
    /// BEFORE the first run — before the first `mark()` — so `records_since`
    /// never contains it. A scenario looking for it there would have searched a
    /// slice where, by construction, it cannot be.
    pub probe_record: Option<&'a RequestRecord>,
    /// The EXACT body the probe sent, so the comparison is like with like
    /// instead of a guess at what the crate's own probe puts on the wire.
    pub probe_sent_body: Option<&'a str>,
    /// WHICH agent was injected during this run, when one was.
    ///
    /// A scenario needs it to assert that the seat that failed is the one that
    /// was asked to fail — "some seat failed" would also pass if the failure
    /// were real.
    pub injected_agent: Option<AgentName>,
    /// `(feature combination, what happened)` for the four combinations. `None`
    /// unless the feature matrix was built, in which case the scenario that
    /// reads it SKIPs naming the flag — never passes.
    pub build_matrix: Option<&'a [(String, BuildOutcome)]>,
    /// `git status` as it stood BEFORE any run started.
    ///
    /// The no-trace scenario needs a baseline, not an absolute: asserting the
    /// tree is clean blames the harness for whatever was already uncommitted —
    /// an operator running it over work in progress would get a red naming the
    /// harness for their own edits. What the scenario can honestly claim is that
    /// it added nothing.
    pub repo_status_before: Option<&'a str>,
}

impl RunContext<'static> {
    /// A context in which NOTHING was observed, for tests that set one field and read one
    /// assertion.
    ///
    /// Lives beside the type rather than inside one scenario module's test block: it started
    /// private to `scenarios::e1`, and the moment a second scenario module needed it the choice
    /// was to hoist it or to copy a fifteen-field literal — and a copied fixture is how two test
    /// suites start disagreeing about what "nothing observed" means.
    ///
    /// # Parameters
    ///
    /// * `run` — which shared run the context claims to describe.
    #[cfg(test)]
    pub(crate) fn blank(run: RunId) -> Self {
        Self {
            run,
            report: None,
            error: None,
            error_class: None,
            records: &[],
            proxy_degraded: false,
            budget_exceeded: None,
            direct_probe_body: None,
            direct_probe_status: None,
            probe_record: None,
            probe_sent_body: None,
            injected_agent: None,
            build_matrix: None,
            repo_status_before: None,
        }
    }
}

/// Where a scenario's assertion reads from.
///
/// **Not every scenario is a run.** Some fire during the preflight and some span
/// the whole session, so forcing them into a run id is what got them
/// mis-assigned twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Reads one shared run.
    Run(RunId),
    /// Reads the preflight's own outcome.
    Preflight,
    /// Reads the WHOLE session, after every run finished.
    Session,
    /// Reads the FEATURE MATRIX, not a run. Its property is a `compile_error!`,
    /// so there is no binary to observe it from — a scenario that runs has
    /// already compiled.
    Build,
}

/// Whether a scenario needs a live backend.
///
/// There is no default: a scenario with no tag is one nobody classified, and
/// under `--no-backend` it would be silently included or silently dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendNeed {
    /// Cannot run without a live backend.
    Required,
    /// Runs with no backend at all.
    None,
}

/// What one feature combination's `cargo check` did.
///
/// **Three states, not two.** A build that FAILED is data — the scenario
/// reading the matrix needs two combinations to fail. A `cargo` that could not
/// be SPAWNED is not data at all, and recording it as "did not build" let a
/// missing toolchain be reported as a verdict about the crate: exit 1 for a
/// fault of ours, which is the one confusion this harness exists to eliminate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildOutcome {
    /// `cargo check` ran and succeeded.
    Built,
    /// `cargo check` ran and rejected the combination. This is the expected
    /// answer for the two combinations that must not compile.
    DidNotBuild,
    /// `cargo` could not be run at all, so nothing was learned either way.
    CouldNotRun,
}

/// One scenario: an id, where it reads from, and what it asserts.
pub struct Scenario {
    /// Stable identifier, printed with every assertion it produces.
    pub id: &'static str,
    /// Where the assertion reads from.
    pub source: Source,
    /// Whether it needs a live backend.
    ///
    /// **Not an `Option`.** It was one, with all twelve scenarios writing
    /// `Some(..)`, so the `None` layer had no consumer and the "no default, on
    /// purpose" it was meant to express was a runtime assertion over a case
    /// nothing produced. Making the field mandatory says the same thing at
    /// compile time, which is strictly stronger.
    pub backend_tag: BackendNeed,
    /// Returns a VECTOR of assertions, never a single state.
    pub assert_fn: fn(&RunContext<'_>) -> Vec<Assertion>,
}

/// Which provider backs a run's seats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// The normal path: one `OllamaProvider` per seat, pointed at the proxy.
    Ollama,
    /// The outside implementation in `external.rs`. Used by the run whose
    /// failure comes from the provider rather than from the wire.
    ExternalStub,
}

/// One shared run's configuration.
pub struct RunSpec {
    /// Which run this is.
    pub id: RunId,
    /// The seats to build the trio from.
    pub seats: Vec<Seat>,
    /// The content to analyse.
    pub payload: Payload,
    /// What injection the proxy applies DURING this run.
    ///
    /// Without this field there is no path at all between a run's configuration
    /// and the proxy: the rotation and degradation scenarios need a specific
    /// model to fail, and nothing was telling the proxy so.
    pub injection: Option<Injection>,
    /// The rotation candidates available to every seat during this run.
    pub fallbacks: Vec<Fallback>,
    /// Which provider backs the seats.
    pub providers: ProviderKind,
    /// What this run asks the backend to do with its reasoning channel.
    ///
    /// Per RUN and not per harness, because the property worth measuring is a COMPARISON: the
    /// same payload against the same model converges with the channel off and burns its whole
    /// budget with it on. A single global setting could only ever show one side of that.
    pub reasoning: ReasoningControl,
    /// Whether this run asks for the reasoning trace's TEXT, not just its length.
    ///
    /// Per RUN for the same reason as [`RunSpec::reasoning`]: the property is a COMPARISON
    /// — with the flag off the report carries the length alone, with it on it carries both
    /// — and a single global setting shows one side of that and calls it certified.
    pub trace: bool,
}

/// Everything one shared run produced.
pub struct RunResult {
    /// Which run this was.
    pub run: RunId,
    /// How it ended.
    pub outcome: RunOutcome,
    /// The report, when the run produced one.
    pub report: Option<MagiReport>,
    /// A typed failure from `analyze()`.
    ///
    /// **`report: None` alone is ambiguous** — it cannot distinguish "the crate
    /// failed in a typed way" (FAIL) from "the run never happened" (SKIP), and
    /// collapsing those two buries crate defects under a SKIP nobody reads.
    pub error: Option<String>,
    /// How `error` must be read. `Some` exactly when `error` is, and derived
    /// from the SAME failure, so the two cannot disagree about one run.
    pub error_class: Option<ErrorClass>,
    /// Everything the proxy saw during this run.
    pub records: Vec<RequestRecord>,
    /// Whether the proxy degraded while this run was in flight.
    pub proxy_degraded: bool,
    /// How many attempts were spent. At most two, and two only for an
    /// inconclusive first attempt.
    pub attempts: u32,
    /// `Some` only when the run hit its time cap: the cap itself, never an
    /// overrun — the run was cut before it finished, so none was measured.
    pub budget_exceeded: Option<Duration>,
    /// WHICH seat this run injected a failure into, when it injected one.
    ///
    /// Derived from the injection's model rather than carried alongside it, so
    /// the two cannot disagree: a scenario asserting that the injected seat
    /// failed needs the seat, and the injection only names a model.
    pub injected_agent: Option<AgentName>,
}

impl RunResult {
    /// The run never happened for a reason that is OURS — a bad config, or a
    /// provider that would not build.
    ///
    /// Assertions reading it SKIP, and it is **not retried**: retrying a
    /// configuration fault reproduces it exactly.
    ///
    /// # Parameters
    ///
    /// * `run` — which run could not be attempted.
    /// * `reason` — what stopped it, in terms an operator can act on.
    pub fn cannot_test(run: RunId, reason: String) -> RunResult {
        RunResult {
            run,
            outcome: RunOutcome::CannotTest,
            report: None,
            error: Some(reason),
            // Never read by a scenario: `main::evaluate` intercepts
            // `CannotTest` before one sees it. Classed all the same, so the
            // "`Some` exactly when `error` is" invariant holds everywhere.
            error_class: Some(ErrorClass::Environment),
            records: Vec::new(),
            proxy_degraded: false,
            attempts: 1,
            budget_exceeded: None,
            injected_agent: None,
        }
    }
}

/// How many attempts a run gets.
///
/// **One retry for anything INCONCLUSIVE; never for a run that produced a
/// verdict.** A deterministic content failure surfaces as a verdict, so retrying
/// it burns a second full run — the most expensive thing this harness does — to
/// arrive at exactly the same place.
///
/// # Parameters
///
/// * `first` — how the first attempt ended.
pub fn attempts_for(first: &RunOutcome) -> u32 {
    if first.is_inconclusive() {
        2
    } else {
        1
    }
}

/// Builds a `Magi` whose every provider points at the PROXY, never at the
/// backend.
///
/// All traffic must be observable, and a seat wired straight to the endpoint
/// would be invisible on the wire.
///
/// `OllamaProvider` and not `OpenAiCompatibleProvider`: it is the only one that
/// also probes, and a scenario reads that probe.
///
/// # Parameters
///
/// * `base_url` — the proxy's address, never the backend's.
/// * `seats` — the trio to register.
/// * `fallbacks` — the rotation candidates, registered as the crate's shared
///   fallback pool. **Without them a seat whose model fails has nowhere to
///   rotate to**, so the run that exists to exercise rotation becomes
///   indistinguishable from the one that exercises degradation, and the scenario
///   reading it can only report that it could not be tested.
/// * `kind` — which provider backs them.
///
/// # Errors
///
/// `Result<_, String>` and not `MagiError`: the three failure sources here are
/// `ProviderError`, a seat name that is not a mage, and `MagiError`, and
/// flattening them at the boundary keeps the caller from needing three `From`
/// impls for a message it only ever prints.
pub fn build_magi_against(
    base_url: &str,
    seats: &[Seat],
    fallbacks: &[Fallback],
    kind: ProviderKind,
    reasoning: ReasoningControl,
    trace: bool,
) -> Result<Magi, String> {
    let first = seats.first().ok_or("no seats configured")?;
    // The dispatch is on the BUILDER call, not on an erased Arc, because the two
    // registration doors are not interchangeable: `with_probing_agent` is
    // generic over `LlmProvider + ProviderProbe` and cannot take an
    // `Arc<dyn LlmProvider>`.
    match kind {
        ProviderKind::Ollama => {
            // `MagiBuilder::new` wants a DEFAULT provider that the `with_*` calls
            // below then use for no seat at all — it is not a fourth mage. One is
            // built and reused as that default.
            let default =
                Arc::new(OllamaProvider::new(base_url, &first.model).map_err(|e| e.to_string())?);
            let mut builder = MagiBuilder::new(Arc::clone(&default) as Arc<dyn LlmProvider>);
            for seat in seats {
                let provider = Arc::new(
                    OllamaProvider::new(base_url, &seat.model).map_err(|e| e.to_string())?,
                );
                // The probing door: one object cannot disagree with itself about
                // which model it measures, and the window and digest it resolves
                // are what a scenario reads.
                let name = seat.agent_name().map_err(|e| e.to_string())?;
                builder =
                    builder.with_probing_agent(name, provider, Lineage::new(seat.lineage.clone()));
            }
            // The pool is shared by every seat, so it is built once and pushed
            // once — it belongs to the run, not to a mage.
            let mut pool = FallbackPool::builder();
            for candidate in fallbacks {
                let provider = Arc::new(
                    OllamaProvider::new(base_url, &candidate.model).map_err(|e| e.to_string())?,
                );
                pool = pool.push_probing(provider, Lineage::new(candidate.lineage.clone()));
            }
            builder
                .with_fallback_pool(pool.build())
                // The control reaches the seats ONLY through here. A run that wants reasoning
                // disabled and does not set it would still converge often enough to look
                // configured, which is why the run declares it and the scenario reads what came
                // back rather than what was asked for.
                .with_completion_config(
                    CompletionConfig::default()
                        .with_reasoning(reasoning)
                        .with_reasoning_trace(trace),
                )
                .build()
                .map_err(|e| e.to_string())
        }
        ProviderKind::ExternalStub => {
            // No probe, deliberately: the outside provider declares none, and the
            // run that uses it feeds no scenario that reads one.
            let stub: Arc<dyn LlmProvider> = Arc::new(external::AlwaysFailsExternally);
            let mut builder = MagiBuilder::new(Arc::clone(&stub));
            for seat in seats {
                let name = seat.agent_name().map_err(|e| e.to_string())?;
                builder =
                    builder.with_agent(name, Arc::clone(&stub), Lineage::new(seat.lineage.clone()));
            }
            builder.build().map_err(|e| e.to_string())
        }
    }
}

/// The ids [`RunSpec::all`] produces, without generating a payload.
///
/// It exists because the cost announcement is printed by the PREFLIGHT, before
/// any spec is built, and announcing a cost means naming the runs about to
/// happen. A second hand-written list of run ids would be one more thing that
/// can drift out of step with `all` in silence — which is exactly what
/// happened twice, once in each direction: it first summed the large-payload
/// run's budget for a run nothing launched, and then, when `S10` made that run
/// real, it omitted the budget of a run that now takes the longest of them all.
/// Both times the paired test is what said so.
///
/// `the_announced_runs_are_the_runs_this_stage_actually_launches` compares this
/// against what `all` really returns, so the two cannot disagree
/// without a red test.
///
/// # Parameters
///
/// * `no_backend` — the partition flag, with the same meaning it has in
///   [`RunSpec::all`].
///
/// # Complexity
///
/// `O(1)`: it allocates a fixed-size list and reads nothing.
pub fn stage_e1_run_ids(no_backend: bool) -> Vec<RunId> {
    if no_backend {
        return vec![RunId::NoBackend];
    }
    vec![
        RunId::HappySmall,
        RunId::Rotation,
        RunId::Degradation,
        RunId::Large62k,
        RunId::CrateDefect,
        RunId::NoBackend,
    ]
}

/// The one captured body of a request the backend ACCEPTED and did not generate from: HTTP
/// 200, `done_reason: "load"`, empty content, and — the discriminant — token counters
/// **absent** rather than zero.
///
/// Embedded rather than fetched: no healthy backend produces this shape on request, and the
/// classification it drives ABORTS a run, so the only honest way to exercise the abort is to
/// replay the artifact that was actually captured.
const CRATE_DEFECT_BODY: &str = include_str!("../../tests/fixtures/ec/native-E-malformed.json");

impl RunSpec {
    /// Every run the harness launches.
    ///
    /// # The large-payload run is now among them, and the reason it was not is what changed
    ///
    /// It used to be excluded because *no assertion read it*, so launching the most expensive
    /// run twice per cycle bought nobody anything. `S10` reads it: the whole point of raising
    /// the default output budget is that the 62 k bundle stops costing a seat, and a small
    /// payload cannot observe that — evidence run H passes clean against the same model and the
    /// same budget that run C fails. **A harness that only exercises small inputs certifies
    /// exactly what never breaks.**
    ///
    /// The function was called `all` while it was the E1 run set. It no longer is:
    /// the run it returns exists for an E2 scenario, and a name promising otherwise is the
    /// defect this project keeps closing.
    ///
    /// # Parameters
    ///
    /// * `cfg` — the loaded configuration.
    /// * `repo_root` — the tree the payload is generated from. Passed in rather
    ///   than resolved here so the function stays testable against a temporary.
    /// * `no_backend` — when true, only the run that needs no backend is
    ///   returned.
    ///
    /// # Errors
    ///
    /// Propagates a payload generation failure: without content there is no run
    /// to specify.
    ///
    /// # Complexity
    ///
    /// One payload generation, then `O(r)` in the number of runs.
    pub fn all(
        cfg: &Config,
        repo_root: &Path,
        no_backend: bool,
    ) -> Result<Vec<RunSpec>, PayloadError> {
        let small = payload::generate(repo_root, cfg.run_payload_bytes)?;
        let no_backend_spec = RunSpec {
            id: RunId::NoBackend,
            seats: cfg.seats.clone(),
            fallbacks: cfg.fallbacks.clone(),
            payload: small.clone(),
            injection: None,
            providers: ProviderKind::ExternalStub,
            reasoning: ReasoningControl::Default,
            trace: false,
        };
        if no_backend {
            return Ok(vec![no_backend_spec]);
        }
        let injected_seat = cfg
            .seats
            .last()
            .map(|s| s.model.clone())
            .unwrap_or_default();
        let injected_seat_defect = injected_seat.clone();
        let small_for_defect = small.clone();
        Ok(vec![
            RunSpec {
                id: RunId::HappySmall,
                seats: cfg.seats.clone(),
                fallbacks: cfg.fallbacks.clone(),
                payload: small.clone(),
                injection: None,
                providers: ProviderKind::Ollama,
                reasoning: ReasoningControl::Default,
                trace: false,
            },
            RunSpec {
                id: RunId::Rotation,
                seats: cfg.seats.clone(),
                fallbacks: cfg.fallbacks.clone(),
                payload: small.clone(),
                injection: Some(Injection::FailModel {
                    model: injected_seat.clone(),
                    status: INJECTED_FAILURE_STATUS,
                }),
                providers: ProviderKind::Ollama,
                reasoning: ReasoningControl::Default,
                trace: false,
            },
            RunSpec {
                id: RunId::Degradation,
                seats: cfg.seats.clone(),
                // DELIBERATELY EMPTY, and this is what separates this run from
                // the rotation one. Both inject the same failure; the difference
                // is whether the seat has anywhere to go. With a pool it rotates
                // and RECOVERS, so the run is not degraded and the scenario that
                // asserts honest degradation has nothing to observe — which is
                // exactly what happened the first time both runs shared a pool.
                fallbacks: Vec::new(),
                payload: small,
                injection: Some(Injection::FailModel {
                    model: injected_seat,
                    status: INJECTED_FAILURE_STATUS,
                }),
                providers: ProviderKind::Ollama,
                reasoning: ReasoningControl::Default,
                trace: false,
            },
            RunSpec {
                id: RunId::Large62k,
                seats: cfg.seats.clone(),
                fallbacks: cfg.fallbacks.clone(),
                // The one run sized from `payload_target_bytes` rather than
                // `run_payload_bytes`: it exists to reproduce the large-input case, and the
                // other runs exist to exercise paths cheaply.
                payload: payload::generate(repo_root, cfg.payload_target_bytes)?,
                injection: None,
                providers: ProviderKind::Ollama,
                reasoning: ReasoningControl::Default,
                // The ONE run that asks for the trace text, and the one where it is worth
                // having: a model burning a large budget is the case somebody turns this on
                // to understand. On a small run it would only show the field is not empty.
                trace: true,
            },
            RunSpec {
                id: RunId::CrateDefect,
                seats: cfg.seats.clone(),
                // NO fallbacks, and this is the assertion rather than a saving: a defect of ours
                // must not rotate, and a run with nowhere to rotate to could not tell whether it
                // refused or merely had no option. The pool is present so the refusal is a
                // choice — see the `fallbacks` line below.
                fallbacks: cfg.fallbacks.clone(),
                payload: small_for_defect,
                injection: Some(Injection::ReplayBody {
                    model: injected_seat_defect,
                    status: 200,
                    body: CRATE_DEFECT_BODY.as_bytes().to_vec(),
                }),
                providers: ProviderKind::Ollama,
                reasoning: ReasoningControl::Default,
                trace: false,
            },
            no_backend_spec,
        ])
    }
}

/// The status the proxy injects when a run wants a model to fail.
///
/// A server error rather than a client one: the crate must read it as the
/// backend failing, not as a request it built wrongly.
///
/// `pub(crate)` because the scenario that asserts no completion carried it read
/// the number from a copy of its own. Two spellings of one wire fact drift in
/// silence: the copy would keep asserting `500` after this one moved, and the
/// assertion would go on passing while checking nothing.
pub(crate) const INJECTED_FAILURE_STATUS: u16 = 500;

/// The OpenAI-compatible completions path `OllamaProvider` speaks in `3.2.0`
/// (the native `/api/chat` path arrives with the EC major, out of scope for
/// this stage).
///
/// Here rather than in a scenario because this module is what puts completions
/// on the wire, and every reader of that traffic has to name the same path to
/// find it.
///
/// # One copy is still outside this seam, and it is named rather than implied
///
/// `preflight.rs` now imports THIS constant instead of keeping its own. It used to keep a
/// private copy, and the note here predicted the failure exactly: *"the two can still drift,
/// and the symptom would be a probe aimed at a path no scenario reads"*. That is what happened
/// when the crate moved to the native endpoint — both copies were left pointing at a path
/// nothing calls any more. One definition removes the drift rather than describing it.
pub(crate) const COMPLETIONS_PATH: &str = "/api/chat";

/// What the transparency probe left behind.
///
/// It lives on the [`Runner`] and not in a [`RunResult`] because it runs before
/// the first run — it belongs to none of them.
#[derive(Default)]
pub struct TransparencyProbe {
    /// The exact body sent, identical down both halves.
    pub sent_body: Option<String>,
    /// The record of the half that went through the proxy.
    pub record: Option<RequestRecord>,
    /// The body the direct half got back, as the term of comparison.
    pub direct_response: Option<Vec<u8>>,
    /// The STATUS the direct half got back, the second term of comparison.
    ///
    /// Set together with [`direct_response`](TransparencyProbe::direct_response)
    /// and never on its own: half a probe compares against a term that is not
    /// there, which says nothing about transparency.
    pub direct_status: Option<u16>,
}

/// The executor. **Owner of the proxy and of the probe**: scenarios only READ,
/// so anything an assertion can look at has to have passed through here.
pub struct Runner {
    config: Config,
    proxy: SpyProxy,
    probe: TransparencyProbe,
}

impl Runner {
    /// Builds a runner over an already-started proxy.
    ///
    /// # Parameters
    ///
    /// * `config` — the loaded configuration, which owns the time budgets.
    /// * `proxy` — the started spy proxy every provider will point at.
    pub fn new(config: Config, proxy: SpyProxy) -> Self {
        Runner {
            config,
            proxy,
            probe: TransparencyProbe::default(),
        }
    }

    /// What the transparency probe left, for the evaluation step to copy into a
    /// [`RunContext`].
    pub fn probe(&self) -> &TransparencyProbe {
        &self.probe
    }

    /// Sends one request down BOTH paths — through the proxy and straight at the
    /// backend — so a scenario can compare what the proxy relayed against what
    /// the backend actually received and returned.
    ///
    /// This is the one deliberate exception to "every request goes through the
    /// proxy": the direct half IS the term of comparison, and without it the
    /// transparency claim has nothing to be checked against.
    ///
    /// Runs ONCE, before the first run. **A failure leaves the fields `None`,
    /// which makes the scenario that reads them SKIP — never FAIL.** A probe
    /// that could not run says nothing about whether the proxy is transparent.
    ///
    /// # Parameters
    ///
    /// * `backend` — the real endpoint, bypassed straight for the direct half.
    /// * `runs` — the runs this invocation will execute. When none of them uses
    ///   the backend, nothing is sent at all: `--no-backend` promises the
    ///   harness reaches no network, and priming unconditionally broke that
    ///   promise with the one request that does not even go through the proxy.
    pub async fn prime_transparency_probe(&mut self, backend: &str, runs: &[RunId]) {
        if !probe_is_in_scope(runs) {
            return;
        }
        let window = probe_window(&self.config);
        let body = format!("{{\"model\":\"{}\"}}", PROBE_MODEL);
        let mark = self.proxy.mark();
        // Through the proxy first, so its record exists before the direct half
        // can influence anything.
        let through = show_request(&self.proxy.base_url(), &body, window).await;
        let direct = show_request(backend, &body, window).await;
        let (Ok(_), Ok((direct_status, direct_body))) = (through, direct) else {
            // Deliberately leaves every field None. Half a probe is worse than
            // none: a scenario comparing against a missing term would report a
            // difference that says nothing about transparency.
            return;
        };
        self.probe.sent_body = Some(body);
        self.probe.record = self.proxy.records_since(mark).into_iter().next();
        self.probe.direct_response = Some(direct_body);
        self.probe.direct_status = Some(direct_status);
    }

    /// Executes one run, with a single retry for an inconclusive first attempt.
    ///
    /// # Parameters
    ///
    /// * `spec` — the run to execute.
    pub async fn execute_one(&mut self, spec: &RunSpec) -> RunResult {
        // The injection is applied BEFORE and cleared AFTER, pass or fail.
        // Leaving the clearing to a comment — as an earlier draft did — let the
        // rotation run's injected failure leak into the degradation run, which
        // executes afterwards over the SAME proxy: the second run would have
        // failed because of the first run's injection, and the scenario would
        // have blamed the crate.
        self.proxy.set_injection(spec.injection.clone());
        let first = self.attempt(spec).await;
        let result = if attempts_for(&first.outcome) > 1 {
            let mut retried = self.attempt(spec).await;
            retried.attempts = 2;
            retried
        } else {
            first
        };
        self.proxy.set_injection(None);
        result
    }

    /// One attempt, bounded by this run's time budget.
    ///
    /// The cap is enforced HERE and nowhere else: a scenario cannot enforce it
    /// (it only sees the result), and leaving it to the crate's own timeouts
    /// would mean the harness has no cap of its own — a run would last as long
    /// as the crate allows.
    async fn attempt(&mut self, spec: &RunSpec) -> RunResult {
        // The cap is the ONLY duration in scope, and deliberately so: the clock
        // reading that used to sit beside it was what the timeout arm reported,
        // and a reading taken when `tokio::time::timeout` fires IS the cap —
        // presented as an overrun it claimed a run had taken twice its budget.
        let cap = self.config.budget(spec.id);
        match tokio::time::timeout(cap, run_catching(Self::run_once(&mut self.proxy, spec))).await {
            // The run finished within its cap and did not panic.
            Ok(Ok(result)) => result,
            // It panicked. `run_catching` already attributed the panic to the
            // crate or to the harness; the attribution is what the outcome
            // carries.
            Ok(Err(outcome)) => RunResult {
                run: spec.id,
                outcome,
                report: None,
                error: None,
                error_class: None,
                records: Vec::new(),
                proxy_degraded: false,
                attempts: 1,
                budget_exceeded: None,
                injected_agent: injected_agent(spec),
            },
            // It ran out of time. A TIME failure is NOT a verdict about the
            // crate: it says the deployment is slower than the cap someone chose.
            Err(_) => timed_out(spec.id, cap, injected_agent(spec)),
        }
    }

    /// Does the work of one attempt: builds the `Magi` pointed at the proxy,
    /// calls `analyze()`, and collects everything the assertions will read.
    ///
    /// Takes the proxy rather than `&mut self` so the future it returns borrows
    /// only what it uses, which is what lets the caller wrap it in the
    /// panic-catching bridge.
    async fn run_once(proxy: &mut SpyProxy, spec: &RunSpec) -> RunResult {
        // Taken BEFORE the run so `records` below holds THIS run's traffic only.
        let mark = proxy.mark();
        let magi = match build_magi_against(
            &proxy.base_url(),
            &spec.seats,
            &spec.fallbacks,
            spec.providers,
            spec.reasoning,
            spec.trace,
        ) {
            Ok(m) => m,
            // A build failure is a CONFIG fault of ours, not a verdict: it must
            // not be retried and must not read as the crate misbehaving.
            Err(e) => return RunResult::cannot_test(spec.id, e),
        };
        // An `Err` from analyze() is a run that COMPLETED and produced a typed
        // failure — a verdict about the crate, not a crash. Conflating the two
        // would retry a deterministic failure and burn a second full run.
        //
        // **The error is KEPT.** Dropping it would leave a scenario unable to
        // tell "the crate returned a typed failure" from "the run never
        // happened", so a real crate defect would be reported as a SKIP — green
        // by omission with extra steps.
        let (report, error, error_class) =
            match magi.analyze(&Mode::Design, &spec.payload.text).await {
                Ok(r) => (Some(r), None, None),
                Err(e) => (None, Some(render_error(&e)), Some(classify_error(&e))),
            };
        RunResult {
            run: spec.id,
            outcome: RunOutcome::Complete,
            report,
            error,
            error_class,
            records: proxy.records_since(mark),
            proxy_degraded: proxy.is_degraded(),
            attempts: 1,
            budget_exceeded: None,
            injected_agent: injected_agent(spec),
        }
    }
}

/// The model the transparency probe asks about.
///
/// Any model name works — the probe compares the two halves against each other,
/// not against a correct answer — so this one is a constant rather than
/// configuration nobody would ever change.
const PROBE_MODEL: &str = "smoke-transparency-probe";

/// ONE `POST /api/show`, sent verbatim, returning the STATUS and the raw body so
/// a scenario can compare both.
///
/// The status travels with the body because the transparency claim covers it:
/// bytes relayed faithfully under a different status are not the answer the
/// backend gave.
///
/// # Parameters
///
/// * `base` — the address to send to: the proxy for one half, the backend for
///   the other.
/// * `body` — the body to send, THE SAME for both halves.
///
/// # Errors
///
/// Any transport failure. The caller turns it into "the probe did not run",
/// which is a SKIP rather than a failure.
/// Whether the transparency probe has anything to say about this invocation.
///
/// Derived from the runs that are about to execute, never from the flag that
/// selected them. `RunSpec::all` already returns only the offline run
/// under `--no-backend`, so reading the specs cannot disagree with what is
/// executed, while a second reading of the flag can — and this decides whether
/// the harness's one deliberate bypass of the proxy is used at all.
///
/// # Parameters
///
/// * `runs` — the ids of the runs this invocation will execute.
///
/// # Complexity
///
/// `O(n)` in `runs.len()`.
fn probe_is_in_scope(runs: &[RunId]) -> bool {
    runs.iter().any(|r| r.uses_backend())
}

/// The window a single probe half is given before it is abandoned.
///
/// `Config::probe_timeout` rather than a constant of its own: it is the value
/// an operator already sets for *how long a trivial request may take before the
/// harness stops waiting on this endpoint*, and the transparency probe asks
/// exactly that question of the same endpoint. A second knob would let the two
/// drift apart with nothing to say which one an operator meant.
///
/// # Parameters
///
/// * `cfg` — the loaded configuration.
fn probe_window(cfg: &Config) -> Duration {
    cfg.probe_timeout()
}

async fn show_request(
    base: &str,
    body: &str,
    within: Duration,
) -> Result<(u16, Vec<u8>), reqwest::Error> {
    let response = reqwest::Client::builder()
        .referer(false)
        .timeout(within)
        .build()?
        .post(format!("{base}/api/show"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await?;
    // Read BEFORE consuming the response into its body, which takes ownership.
    let status = response.status().as_u16();
    response.bytes().await.map(|b| (status, b.to_vec()))
}

/// The result of a run that reached its time cap.
///
/// # Parameters
///
/// * `run` — which run ran out of time.
/// * `cap` — the budget it was given.
/// * `injected_agent` — the seat this run injected into, if any.
fn timed_out(run: RunId, cap: Duration, injected_agent: Option<AgentName>) -> RunResult {
    RunResult {
        run,
        outcome: RunOutcome::TimedOut,
        budget_exceeded: Some(cap),
        report: None,
        error: None,
        error_class: None,
        records: Vec::new(),
        proxy_degraded: false,
        attempts: 1,
        injected_agent,
    }
}

/// Which seat a run injected into, resolved from the injection's model.
///
/// Returns `None` when the run injected nothing, and also when the injected
/// model matches no seat — which is a configuration mistake, not an agent.
///
/// # The match is EXACT on both sides, and it has to stay that way
///
/// This resolves the seat by `s.model == model`, and the proxy decides which
/// requests to fail by comparing the same name against the `"model"` field it
/// parses out of the body — also with `==`, never containment. Two configured
/// models where one name is a PREFIX or SUBSTRING of the other are therefore
/// still told apart, and the current trio would be fine even if they were not.
///
/// It is written down because the two comparisons live in different modules
/// and neither mentions the other. If either ever loosened to `contains`, the
/// injection would knock out more seats than the scenario asked for while this
/// function kept attributing the failure to one of them — a degradation run
/// that took out two seats, reported against one, and read as the crate
/// degrading badly rather than as the harness over-injecting.
fn injected_agent(spec: &RunSpec) -> Option<AgentName> {
    // An exhaustive match rather than a `let ... else`: both injections name a model, and a
    // third one that did not would have to say so here instead of silently attributing to none.
    let model = match spec.injection.as_ref()? {
        Injection::FailModel { model, .. } | Injection::ReplayBody { model, .. } => model,
    };
    spec.seats
        .iter()
        .find(|s| &s.model == model)
        .and_then(|s| s.agent_name().ok())
}

/// Renders a crate error for a scenario to read.
///
/// Kept as one function so every run renders a failure the same way, rather than
/// each call site choosing its own wording.
fn render_error(e: &MagiError) -> String {
    e.to_string()
}

/// Which of the two things a typed failure means — see [`ErrorClass`].
///
/// # The two that are NOT the crate's, and nothing else
///
/// `EndpointDown` and `InsufficientAgents` are the crate reporting, correctly,
/// that the world around it failed: no lineage was reachable, or too few seats
/// survived to reach consensus. A backend that dies AFTER the preflight is a
/// limitation this harness declares in its own README, so reading either as a
/// verdict sends whoever ran it into the crate to find nothing.
///
/// # Why the catch-all is `CrateFailure` and not the other way round
///
/// `MagiError` is `#[non_exhaustive]`, so this match needs a default, and the
/// two defaults are not symmetric. Defaulting to `Environment` would let a
/// variant added later — one that really is the crate breaking — leave as a
/// skip nobody investigates, which is the failure this split was written to
/// close. Defaulting here costs at worst one investigation that finds the
/// classification, not the crate, to be out of date.
///
/// # Parameters
///
/// * `e` — the failure `analyze()` returned.
fn classify_error(e: &MagiError) -> ErrorClass {
    match e {
        MagiError::EndpointDown { .. } | MagiError::InsufficientAgents { .. } => {
            ErrorClass::Environment
        }
        _ => ErrorClass::CrateFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_failures_the_crate_reports_correctly_are_not_crate_failures() {
        // Both are the crate telling the truth about the world around it: no
        // reachable endpoint, or too few seats left to reach consensus. Neither
        // is `analyze()` breaking, and reading them as such sends whoever runs
        // this into the crate over a backend that died.
        assert_eq!(
            classify_error(&MagiError::EndpointDown {
                lineages: vec![Lineage::new("alibaba")],
            }),
            ErrorClass::Environment
        );
        assert_eq!(
            classify_error(&MagiError::InsufficientAgents {
                succeeded: 0,
                required: 2,
            }),
            ErrorClass::Environment
        );
    }

    #[test]
    fn everything_else_is_the_crate_s_own_failure() {
        // The other side of the same split, and the DEFAULT for a variant added
        // later: `MagiError` is `#[non_exhaustive]`, so an unrecognised failure
        // must land where somebody looks at it.
        assert_eq!(
            classify_error(&MagiError::Validation("bad input".to_string())),
            ErrorClass::CrateFailure
        );
        assert_eq!(
            classify_error(&MagiError::Deserialization("not json".to_string())),
            ErrorClass::CrateFailure
        );
    }

    #[test]
    fn the_transparency_probe_is_out_of_scope_when_no_run_uses_the_backend() {
        // `--no-backend` promises the harness reaches no network. The probe was
        // primed unconditionally, so that invocation sent two requests — one
        // through the proxy, one STRAIGHT at the endpoint, which is the one
        // deliberate bypass in the whole harness — at exactly the moment the
        // operator asked for none.
        //
        // Derived from the SPECS rather than from the flag: `all`
        // already returns only the offline run under `--no-backend`, so reading
        // the specs cannot disagree with what is about to be executed, while a
        // second reading of the flag can.
        let offline_only = [RunId::NoBackend];
        let with_backend = [RunId::NoBackend, RunId::HappySmall];
        assert!(
            !probe_is_in_scope(&offline_only),
            "no run touches the backend, so there is nothing for a transparency probe to \
             compare and no reason to send its two requests"
        );
        assert!(
            probe_is_in_scope(&with_backend),
            "a run that uses the backend needs the probe, or the scenario reading it can only \
             report that it could not be tested"
        );
    }

    #[tokio::test]
    async fn the_transparency_probe_gives_up_instead_of_waiting_forever() {
        // `reqwest` applies NO timeout unless one is set, so the probe's two
        // requests had no bound of their own. A backend that accepts the
        // connection and never answers would hang them, and this runs BEFORE
        // the first run — so it is outside every per-run budget, which is the
        // only other thing that would have cut it.
        let stub = crate::testkit::stub_that_is_always_slow().await;
        let started = std::time::Instant::now();
        let answered = show_request(&stub.url(), "{}", Duration::from_millis(50)).await;
        assert!(
            answered.is_err(),
            "a backend slower than the window must leave the probe with nothing, which makes \
             the scenario reading it SKIP"
        );
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "the probe waited {:?}, so its own bound is not what stopped it",
            started.elapsed()
        );
    }

    #[test]
    fn a_timed_out_run_reports_its_cap_verbatim() {
        // `tokio::time::timeout` fires AT the cap, so an elapsed-time reading
        // taken when it fires IS the budget, give or take scheduling noise —
        // and rendered as an overrun it read as a run that took twice its
        // budget. Nobody measured that: the run was cut before it finished, so
        // how far past its cap it would have gone is unknowable. The cap is what
        // is known, so the cap is what travels.
        //
        // **What this pins and what it does not.** It pins that the value is
        // carried through untransformed. That the CALL SITE hands it the cap and
        // not a clock reading is structural rather than asserted here: `attempt`
        // no longer takes an `Instant`, so `cap` is the only `Duration` in scope
        // to pass. Observing the call site would mean driving a real run against
        // a stub past its budget, which costs more than the property is worth.
        let cap = Duration::from_secs(300);
        let result = timed_out(RunId::Large62k, cap, None);
        assert_eq!(
            result.budget_exceeded,
            Some(cap),
            "the cap must arrive unchanged, not scaled or re-derived"
        );
    }

    #[test]
    fn an_inconclusive_run_is_retried_exactly_once() {
        // Two attempts, never three: a second retry doubles the cost of the most
        // expensive thing the harness does for a third identical answer.
        assert_eq!(attempts_for(&RunOutcome::TimedOut), 2);
        assert_eq!(attempts_for(&RunOutcome::PanickedInHarness), 2);
    }

    #[test]
    fn a_run_that_produced_a_verdict_is_never_retried() {
        // A deterministic content failure surfaces as a verdict; retrying it
        // burns a second full run to arrive at the same place.
        assert_eq!(attempts_for(&RunOutcome::Complete), 1);
    }

    #[test]
    fn a_panic_in_the_crate_is_a_verdict_and_is_not_retried_either() {
        // The crate broke, and that is exactly what the harness came to find.
        // Retrying it is the same waste as retrying a completed run.
        assert_eq!(attempts_for(&RunOutcome::PanickedInCrate), 1);
    }

    #[test]
    fn a_run_that_could_not_start_is_never_retried() {
        // The bug this pins: `cannot_test` reported `Crashed`, which IS
        // inconclusive, so `execute_one` retried a configuration fault that
        // reproduces identically. Its own rustdoc said the opposite.
        assert_eq!(attempts_for(&RunOutcome::CannotTest), 1);
    }

    #[test]
    fn each_run_carries_a_declared_time_budget_with_its_measured_origin() {
        // These are CAPS, not predictions: a run that reaches one reports a TIME
        // failure, never a verdict about the crate.
        let c = Config::default();
        assert_eq!(c.budget(RunId::HappySmall), Duration::from_secs(120));
        assert_eq!(c.budget(RunId::Large62k), Duration::from_secs(300));
        assert_eq!(c.budget(RunId::Rotation), Duration::from_secs(180));
        assert_eq!(c.budget(RunId::NoBackend), Duration::from_secs(30));
    }

    #[test]
    fn cannot_test_is_a_skip_with_a_reason_not_a_silent_empty_result() {
        let r = RunResult::cannot_test(RunId::HappySmall, "no seats configured".into());
        assert!(r.report.is_none());
        assert_eq!(r.error.as_deref(), Some("no seats configured"));
        assert_eq!(
            r.attempts, 1,
            "a config fault reproduces exactly; never retry it"
        );
        // Asserted on the value the CONSTRUCTOR produces, not on the policy in
        // the abstract. The first version of this test checked only the
        // hardcoded `attempts` field, so it stayed green while `cannot_test`
        // reported `Crashed` — an inconclusive outcome that `execute_one`
        // dutifully retried, contradicting this constructor's own rustdoc.
        assert!(
            !r.outcome.is_inconclusive(),
            "cannot_test must not report an outcome the retry rule retries"
        );
        assert_eq!(attempts_for(&r.outcome), 1);
    }

    #[test]
    fn the_no_backend_run_is_the_only_one_returned_when_the_backend_is_off() {
        let cfg = Config::default();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the manifest dir always has a parent");
        let specs = RunSpec::all(&cfg, root, true).expect("payload generation");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].id, RunId::NoBackend);
        assert_eq!(specs[0].providers, ProviderKind::ExternalStub);
        assert!(
            specs[0].injection.is_none(),
            "a run with no backend has no wire to inject on"
        );
    }

    #[test]
    fn the_large_payload_run_is_not_part_of_this_stage() {
        // Launching it would pay for the harness's most expensive run twice per
        // cycle for an assertion nobody has written yet.
        let cfg = Config::default();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the manifest dir always has a parent");
        let specs = RunSpec::all(&cfg, root, false).expect("payload generation");
        // It used to be excluded, and the exclusion expired the moment an assertion read it:
        // `S10` certifies that the raised output budget stops costing a seat on the 62 k
        // bundle, and only the large payload can show that.
        assert!(specs.iter().any(|s| s.id == RunId::Large62k));
    }

    #[test]
    fn the_announced_runs_are_the_runs_this_stage_actually_launches() {
        // `stage_e1_run_ids` exists so the preflight can name the runs before a
        // spec is built, and a second hand-written list is precisely what drifts
        // out of step in silence — the announcement used to sum the
        // large-payload run's budget for a run that never launches. This is the
        // comparison that makes the two fail together instead.
        let cfg = Config::default();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the manifest dir always has a parent");
        for no_backend in [false, true] {
            let real: Vec<RunId> = RunSpec::all(&cfg, root, no_backend)
                .expect("payload generation")
                .iter()
                .map(|s| s.id)
                .collect();
            assert_eq!(
                stage_e1_run_ids(no_backend),
                real,
                "the announced run list must BE the launched one, in the same order \
                 (no_backend = {no_backend})"
            );
        }
    }

    #[test]
    fn the_rotation_run_has_somewhere_to_rotate_to() {
        // Without a fallback pool the rotation run is crate-identical to the
        // degradation run: a seat's model fails and nothing takes its place, so
        // the scenario that exists to observe rotation can only report that it
        // could not be tested. It did exactly that until this was wired.
        let cfg = Config::default();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the manifest dir always has a parent");
        let specs = RunSpec::all(&cfg, root, false).expect("payload generation");
        let rotation = specs
            .iter()
            .find(|s| s.id == RunId::Rotation)
            .expect("the rotation run is part of this stage");
        assert!(
            !rotation.fallbacks.is_empty(),
            "rotation with an empty pool tests nothing"
        );
        for candidate in &rotation.fallbacks {
            assert!(
                cfg.seats.iter().all(|s| s.lineage != candidate.lineage),
                "candidate {:?} shares a lineage with a seat, so rotating to it reaches \
                 the same lineage the run was trying to leave",
                candidate.model
            );
        }
    }

    #[test]
    fn the_degradation_run_has_no_pool_so_the_seat_actually_degrades() {
        // The two injected runs differ ONLY in whether a fallback exists. Give
        // both a pool and the degraded run recovers by rotating, so every
        // assertion about honest degradation reads a run that never degraded.
        let cfg = Config::default();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the manifest dir always has a parent");
        let specs = RunSpec::all(&cfg, root, false).expect("payload generation");
        let degradation = specs
            .iter()
            .find(|s| s.id == RunId::Degradation)
            .expect("the degradation run is part of this stage");
        assert!(
            degradation.fallbacks.is_empty(),
            "a seat with somewhere to rotate to does not degrade"
        );
        assert!(
            degradation.injection.is_some(),
            "without an injection nothing fails and there is no degradation either"
        );
    }

    #[test]
    fn the_injected_runs_name_a_model_that_is_actually_in_the_trio() {
        // An injection naming a model no seat uses would fire on nothing, and
        // the rotation scenario would pass by never being exercised.
        let cfg = Config::default();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the manifest dir always has a parent");
        let specs = RunSpec::all(&cfg, root, false).expect("payload generation");
        let injected: Vec<&Injection> = specs.iter().filter_map(|s| s.injection.as_ref()).collect();
        assert_eq!(
            injected.len(),
            3,
            "rotation, degradation and the crate-defect run all inject"
        );
        for inj in injected {
            let (Injection::FailModel { model, .. } | Injection::ReplayBody { model, .. }) = inj;
            assert!(
                cfg.seats.iter().any(|s| &s.model == model),
                "injected model {model:?} is in no seat"
            );
        }
    }

    #[test]
    fn an_assertion_that_could_not_be_tested_carries_its_reason() {
        let a = Assertion::skip("the window was measured", "the probe never answered");
        match a.state {
            ScenarioState::Skip(reason) => assert!(reason.contains("probe")),
            other => panic!("expected a Skip carrying its reason, got {other:?}"),
        }
    }

    #[test]
    fn assert_that_names_the_property_on_both_sides() {
        assert_eq!(assert_that("it held", true).state, ScenarioState::Pass);
        assert_eq!(assert_that("it did not", false).state, ScenarioState::Fail);
    }
}
