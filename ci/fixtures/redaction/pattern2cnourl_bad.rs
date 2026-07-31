// TARGET: providers/claude_cli.rs
// EXPECT: exempt from rule 1 only because it has no URL
//! The subprocess provider is skipped by rule 1 on the premise that it has no URL. Here it grew
//! one, so the premise is false and the exemption must be withdrawn.

use crate::providers::provider_url::ProviderUrl;

pub struct Cli {
    endpoint: ProviderUrl,
}
