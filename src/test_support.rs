// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-05-16

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
use crate::provider::{CompletionConfig, LlmProvider};
use crate::reporting::MagiReport;
use crate::rotation::{FallbackPool, Lineage, ProviderProbe, RotationKind};
use crate::schema::AgentName;
use crate::verdict_markers::{VERDICT_CLOSE, VERDICT_OPEN};

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
    ) -> Result<String, ProviderError> {
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
        Ok(seq.remove(0)?)
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
    /// Return a valid, agent-aware verdict.
    Ok,
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
        _c: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        let i = self.calls.fetch_add(1, Ordering::SeqCst);
        let beh = self
            .script
            .get(i)
            .or_else(|| self.script.last())
            .cloned()
            .unwrap_or(Beh::Ok);
        match beh {
            Beh::Ok => Ok(valid_verdict_for_current_agent()),
            Beh::BadJson => Ok(BAD_JSON.clone()),
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
    ) -> Result<String, ProviderError> {
        Ok(valid_verdict_for_current_agent())
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

/// The TWIN of [`build_two_network_failing_no_fallback`], differing in exactly one thing: the two
/// failing seats report `ProviderError::External` instead of `ProviderError::Network`.
///
/// Everything else — the lineages, the empty pool, the healthy middle seat — is identical on
/// purpose. The endpoint-down latch fires for the twin and must NOT fire here, and holding every
/// other variable still is what makes that difference attributable to the error class rather than
/// to the topology.
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
        assert_eq!(r1, "MEL_1");

        let r2 = CURRENT_AGENT_IDENTITY
            .scope(AgentName::Balthasar, mp.complete("sys", "x", &cfg))
            .await
            .unwrap();
        assert_eq!(r2, "BAL_1");

        let r3 = CURRENT_AGENT_IDENTITY
            .scope(AgentName::Melchior, mp.complete("sys", "x", &cfg))
            .await
            .unwrap();
        assert_eq!(r3, "MEL_2");
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
        assert_eq!(r2, "MEL_2");
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
    fn test_the_two_scripted_bodies_fail_and_succeed_where_their_names_claim() {
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

        // And the guard rail for the reverse drift: a bare body no longer models a
        // cooperative provider at all.
        assert_eq!(
            extract("not json at all").unwrap_err().cause(),
            ExtractionFailureCause::MissingMarkers
        );
    }
}
