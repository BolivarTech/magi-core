// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-05-16

//! Test-only support utilities. Gated `#[cfg(any(test, feature = "test-utils"))]`
//! at the module declaration in `lib.rs`.
//!
//! **Stability:** the `test-utils` feature is stable only within the v0.4.x
//! line. Future versions may rename, restructure, or remove this module.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::agent::CURRENT_AGENT_IDENTITY;
use crate::error::ProviderError;
use crate::provider::{CompletionConfig, LlmProvider};
use crate::schema::AgentName;

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

    /// MAGI R1 W9: invariant — each prompt file still contains the agent
    /// role marker. Not load-bearing for routing (we use task-local now),
    /// but keeps the option open for marker-based detection in downstream
    /// mock providers.
    #[test]
    fn test_each_prompt_file_contains_agent_role_marker() {
        assert!(crate::prompts::melchior_prompt().contains("Melchior"));
        assert!(crate::prompts::balthasar_prompt().contains("Balthasar"));
        assert!(crate::prompts::caspar_prompt().contains("Caspar"));
    }
}
