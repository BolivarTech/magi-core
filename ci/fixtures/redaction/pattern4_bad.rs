// TARGET: providers/provider_url.rs
// EXPECT: would print or persist
use reqwest as _;
fn redacted(&self) -> String { String::new() }
#[derive(Clone, Debug)]
pub(crate) struct ProviderUrl;
