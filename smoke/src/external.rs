// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-17

//! An `LlmProvider` implemented OUTSIDE `magi-core`.
//!
//! It lives here, in the harness, rather than in one of the crate's own tests
//! because inside `src/` every error variant is always constructible and a test
//! there would pass with the API broken — the lesson of the `E0639` regression
//! of `2.0.0`, which a consumer found eight days after the release.

use crate::alias::magi_core::error::{ExternalErrorKind, ProviderError};
use crate::alias::magi_core::provider::{Completion, CompletionConfig, LlmProvider};

/// The smallest possible outside implementation. Its ONLY job is to fail in a
/// typed way, which is the property `S1` asserts.
pub struct AlwaysFailsExternally;

#[async_trait::async_trait]
impl LlmProvider for AlwaysFailsExternally {
    async fn complete(
        &self,
        _system: &str,
        _user: &str,
        _cfg: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        // `external` is the ONLY constructor reachable from another crate, and
        // that is exactly the property S1 exists to hold.
        Err(ProviderError::external(
            "simulated outside failure",
            ExternalErrorKind::ServerError,
        ))
    }

    fn name(&self) -> &str {
        "external-stub"
    }

    fn model(&self) -> &str {
        "none"
    }
}
