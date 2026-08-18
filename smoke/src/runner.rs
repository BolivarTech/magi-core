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
use std::time::{Duration, Instant};

use crate::alias::magi_core::error::MagiError;
use crate::alias::magi_core::orchestrator::{Magi, MagiBuilder};
use crate::alias::magi_core::provider::LlmProvider;
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

/// What a scenario gets to look at: everything a run produced, and NOTHING it
/// did not.
///
/// An assertion cannot reach the network on its own, which is what makes "one
/// run, many assertions" cheap AND honest.
pub struct RunContext<'a> {
    /// Which shared run fed this assertion.
    pub run: RunId,
    /// `None` when the run produced no report. Read it TOGETHER with `error`:
    /// `None` + `Some(error)` is a typed crate failure (FAIL); `None` + `None`
    /// is a run that never happened (SKIP).
    pub report: Option<&'a MagiReport>,
    /// A typed failure from `analyze()`, rendered.
    pub error: Option<&'a str>,
    /// Everything the proxy saw on the wire during THIS run.
    pub records: &'a [RequestRecord],
    /// True if the proxy degraded. Assertions that read `records` must SKIP,
    /// because a partial registry could fail an assertion the crate satisfied
    /// perfectly.
    pub proxy_degraded: bool,
    /// How many attempts this run took. `2` means the first was inconclusive and
    /// the single retry was spent.
    pub attempts: u32,
    /// `Some` only when the run hit its time cap, so a reader can tell a TIME
    /// failure from an assertion failure.
    pub over_budget: Option<Duration>,
    /// The response to the DIRECT half of the transparency probe — the term of
    /// comparison. `None` on every run but the one that primed it.
    pub direct_probe_body: Option<&'a [u8]>,
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
    /// `(feature combination, did it build)` for the four combinations. `None`
    /// unless the feature matrix was built, in which case the scenario that
    /// reads it SKIPs naming the flag — never passes.
    pub build_matrix: Option<&'a [(String, bool)]>,
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

/// One scenario: an id, where it reads from, and what it asserts.
pub struct Scenario {
    /// Stable identifier, printed with every assertion it produces.
    pub id: &'static str,
    /// Where the assertion reads from.
    pub source: Source,
    /// Whether it needs a live backend. No default, on purpose.
    pub backend_tag: Option<BackendNeed>,
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
    /// Everything the proxy saw during this run.
    pub records: Vec<RequestRecord>,
    /// Whether the proxy degraded while this run was in flight.
    pub proxy_degraded: bool,
    /// How many attempts were spent. At most two, and two only for an
    /// inconclusive first attempt.
    pub attempts: u32,
    /// `Some` only when the run hit its time cap.
    pub over_budget: Option<Duration>,
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
            records: Vec::new(),
            proxy_degraded: false,
            attempts: 1,
            over_budget: None,
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

impl RunSpec {
    /// This stage's runs.
    ///
    /// The large-payload run is deliberately NOT among them: no assertion in
    /// this stage reads it, so launching it would pay for the harness's most
    /// expensive run, twice per cycle, for nobody. The payload is still
    /// generated and checked by size, which is the only claim available before
    /// the telemetry that would justify running it exists.
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
    pub fn for_stage_e1(
        cfg: &Config,
        repo_root: &Path,
        no_backend: bool,
    ) -> Result<Vec<RunSpec>, PayloadError> {
        let small = payload::generate(repo_root, SMALL_PAYLOAD_BYTES)?;
        let no_backend_spec = RunSpec {
            id: RunId::NoBackend,
            seats: cfg.seats.clone(),
            fallbacks: cfg.fallbacks.clone(),
            payload: small.clone(),
            injection: None,
            providers: ProviderKind::ExternalStub,
        };
        if no_backend {
            return Ok(vec![no_backend_spec]);
        }
        let injected_seat = cfg
            .seats
            .last()
            .map(|s| s.model.clone())
            .unwrap_or_default();
        Ok(vec![
            RunSpec {
                id: RunId::HappySmall,
                seats: cfg.seats.clone(),
                fallbacks: cfg.fallbacks.clone(),
                payload: small.clone(),
                injection: None,
                providers: ProviderKind::Ollama,
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
            },
            RunSpec {
                id: RunId::Degradation,
                seats: cfg.seats.clone(),
                fallbacks: cfg.fallbacks.clone(),
                payload: small,
                injection: Some(Injection::FailModel {
                    model: injected_seat,
                    status: INJECTED_FAILURE_STATUS,
                }),
                providers: ProviderKind::Ollama,
            },
            no_backend_spec,
        ])
    }
}

/// Size of the payload the cheap runs analyse.
///
/// Small on purpose: these runs exist to exercise paths, not to reproduce the
/// large-input failure, and the large payload's own run is not part of this
/// stage.
const SMALL_PAYLOAD_BYTES: usize = 2_048;

/// The status the proxy injects when a run wants a model to fail.
///
/// A server error rather than a client one: the crate must read it as the
/// backend failing, not as a request it built wrongly.
const INJECTED_FAILURE_STATUS: u16 = 500;

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
    pub async fn prime_transparency_probe(&mut self, backend: &str) {
        let body = format!("{{\"model\":\"{}\"}}", PROBE_MODEL);
        let mark = self.proxy.mark();
        // Through the proxy first, so its record exists before the direct half
        // can influence anything.
        let through = show_request(&self.proxy.base_url(), &body).await;
        let direct = show_request(backend, &body).await;
        let (Ok(_), Ok(direct_body)) = (through, direct) else {
            // Deliberately leaves every field None. Half a probe is worse than
            // none: a scenario comparing against a missing term would report a
            // difference that says nothing about transparency.
            return;
        };
        self.probe.sent_body = Some(body);
        self.probe.record = self.proxy.records_since(mark).into_iter().next();
        self.probe.direct_response = Some(direct_body);
    }

    /// Executes every run in order.
    ///
    /// # Parameters
    ///
    /// * `specs` — the runs to execute, in order.
    ///
    /// # Complexity
    ///
    /// One backend run per spec, plus at most one retry each.
    pub async fn execute(&mut self, specs: &[RunSpec]) -> Vec<RunResult> {
        let mut out = Vec::with_capacity(specs.len());
        for spec in specs {
            out.push(self.execute_one(spec).await);
        }
        out
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
        let cap = self.config.budget(spec.id);
        let started = Instant::now();
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
                records: Vec::new(),
                proxy_degraded: false,
                attempts: 1,
                over_budget: None,
            },
            // It ran out of time. A TIME failure is NOT a verdict about the
            // crate: it says the deployment is slower than the cap someone chose.
            Err(_) => RunResult {
                run: spec.id,
                outcome: RunOutcome::TimedOut,
                over_budget: Some(started.elapsed()),
                report: None,
                error: None,
                records: Vec::new(),
                proxy_degraded: false,
                attempts: 1,
            },
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
        let (report, error) = match magi.analyze(&Mode::Design, &spec.payload.text).await {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(render_error(&e))),
        };
        RunResult {
            run: spec.id,
            outcome: RunOutcome::Complete,
            report,
            error,
            records: proxy.records_since(mark),
            proxy_degraded: proxy.is_degraded(),
            attempts: 1,
            over_budget: None,
        }
    }
}

/// The model the transparency probe asks about.
///
/// Any model name works — the probe compares the two halves against each other,
/// not against a correct answer — so this one is a constant rather than
/// configuration nobody would ever change.
const PROBE_MODEL: &str = "smoke-transparency-probe";

/// ONE `POST /api/show`, sent verbatim, returning the raw body so a scenario can
/// compare bytes.
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
async fn show_request(base: &str, body: &str) -> Result<Vec<u8>, reqwest::Error> {
    reqwest::Client::builder()
        .referer(false)
        .build()?
        .post(format!("{base}/api/show"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await?
        .bytes()
        .await
        .map(|b| b.to_vec())
}

/// Renders a crate error for a scenario to read.
///
/// Kept as one function so every run renders a failure the same way, rather than
/// each call site choosing its own wording.
fn render_error(e: &MagiError) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inconclusive_run_is_retried_exactly_once() {
        // Two attempts, never three: a second retry doubles the cost of the most
        // expensive thing the harness does for a third identical answer.
        assert_eq!(attempts_for(&RunOutcome::Crashed), 2);
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
        let specs = RunSpec::for_stage_e1(&cfg, root, true).expect("payload generation");
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
        let specs = RunSpec::for_stage_e1(&cfg, root, false).expect("payload generation");
        assert!(specs.iter().all(|s| s.id != RunId::Large62k));
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
        let specs = RunSpec::for_stage_e1(&cfg, root, false).expect("payload generation");
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
                "candidate {:?} shares a lineage with a seat, so rotating to it                  reaches the same lineage the run was trying to leave",
                candidate.model
            );
        }
    }

    #[test]
    fn the_injected_runs_name_a_model_that_is_actually_in_the_trio() {
        // An injection naming a model no seat uses would fire on nothing, and
        // the rotation scenario would pass by never being exercised.
        let cfg = Config::default();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the manifest dir always has a parent");
        let specs = RunSpec::for_stage_e1(&cfg, root, false).expect("payload generation");
        let injected: Vec<&Injection> = specs.iter().filter_map(|s| s.injection.as_ref()).collect();
        assert_eq!(injected.len(), 2, "rotation and degradation both inject");
        for inj in injected {
            let Injection::FailModel { model, .. } = inj;
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
