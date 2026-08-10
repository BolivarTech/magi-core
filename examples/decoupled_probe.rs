// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-09

//! Proves, by compiling as a SEPARATE crate, that a rotation probe can be declared
//! WITHOUT handing over the provider that serves completions.
//!
//! Before this, the only way to give the rotation policy a candidate's context window
//! was a single generic type implementing BOTH `LlmProvider` and `ProviderProbe`. The
//! only production type implementing `ProviderProbe` is the Ollama provider, so a
//! consumer whose completions provider was something else had no measured window at
//! all — with a strict context guard, every such candidate got filtered out and
//! rotation went silently inert. A capability the pool needed (measurement) was
//! dictated by which type happened to serve completions. That coupling cannot be
//! exercised from inside `src/`: there, the generic bound is satisfied trivially by
//! test doubles that implement both traits on the same type, which never proves the
//! two roles can be split.
//!
//! `push_with_probe` / `with_agent_and_probe` take two already-erased trait objects,
//! so the completions provider and the probe are free to be different concrete types —
//! `HostedModel` below never implements `ProviderProbe`, and `SidecarProbe` never
//! implements `LlmProvider`. What ties them together is not the type system but the
//! caller: both are built from the same `model_name` binding, and keeping that
//! agreement true — the probe must measure the model the completions provider actually
//! reports — is the caller's responsibility once the two roles are declared apart. The
//! crate cannot check it structurally, because a completions provider and a probe with
//! disagreeing model names still type-check.
//!
//! `MagiBuilder::build()` is deliberately never called here: it needs a full trio and a
//! live endpoint, neither of which this example has. The property under test is that
//! registering a decoupled probe COMPILES from outside the crate, not that a run
//! succeeds.
//!
//! ```text
//! cargo run --example decoupled_probe
//! ```

use async_trait::async_trait;
use magi_core::prelude::*;
use std::sync::Arc;

/// A completions-only provider: it never implements `ProviderProbe`. Standing in for a
/// hosted or third-party backend whose window/digest are not queryable through the same
/// object that serves completions.
struct HostedModel {
    model: String,
}

#[async_trait]
impl LlmProvider for HostedModel {
    async fn complete(
        &self,
        _system_prompt: &str,
        _user_prompt: &str,
        _config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        // Never invoked — this example proves registration compiles, not that a
        // completion runs.
        Ok(String::new())
    }

    fn name(&self) -> &str {
        "hosted-model"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// A probe-only object: it never implements `LlmProvider`. Standing in for a sidecar
/// that reports capability metadata for a model it does not itself serve completions
/// for — a fleet manifest or a capability registry queried out-of-band from the
/// inference path.
struct SidecarProbe;

#[async_trait]
impl ProviderProbe for SidecarProbe {
    async fn window(&self) -> Result<Option<usize>, ProviderError> {
        // A real sidecar would look this up by the model name it was configured with;
        // fixed here because the value itself is not what this example proves.
        Ok(Some(128_000))
    }

    async fn digest(&self) -> Result<Option<String>, ProviderError> {
        Ok(Some("sha256:example".into()))
    }
}

fn main() {
    // Declared once so the agreement between "what completes" and "what gets probed"
    // is visible at the source level — nothing in the type system enforces it once the
    // two roles are separate objects.
    let model_name = "frontier-model-v1";

    let provider: Arc<dyn LlmProvider> = Arc::new(HostedModel {
        model: model_name.to_string(),
    });
    let probe: Arc<dyn ProviderProbe> = Arc::new(SidecarProbe);
    let lineage = Lineage::new("example-vendor");

    // A second, probeless candidate: mixing measured and unmeasured candidates in one
    // pool is the case a consumer actually has, and it used to be unreachable without
    // making every candidate an Ollama provider.
    let unprobed: Arc<dyn LlmProvider> = Arc::new(HostedModel {
        model: "second-candidate".to_string(),
    });

    let pool = FallbackPool::builder()
        .push_with_probe(Arc::clone(&provider), lineage.clone(), Arc::clone(&probe))
        .push(unprobed, Lineage::new("second-vendor"))
        .build();

    // The proof this file carries is the COMPILATION, per the module doc — not this line. The
    // assert is a cheap sanity check on the registration, and it uses `assert!` rather than the
    // error handling the sibling example models on purpose: a failure here would mean the crate
    // is broken, not that a consumer did something they need to handle.
    assert_eq!(
        pool.len(),
        2,
        "both the probed and the probeless candidate must register"
    );
    println!(
        "fallback pool holds {} candidates: one measured by a SEPARATE probe object, one unmeasured",
        pool.len()
    );

    // Registering the primary this way is the actual proof: `provider` and `probe` are
    // handed over as two independent trait objects, and this file compiling as a
    // standalone crate is what a test inside `src/` cannot demonstrate.
    let _builder = MagiBuilder::new(Arc::clone(&provider)).with_agent_and_probe(
        AgentName::Melchior,
        provider,
        lineage,
        probe,
    );

    println!(
        "decoupled probe registration compiled: probing a candidate no longer requires \
         its completions provider to implement ProviderProbe"
    );
}
