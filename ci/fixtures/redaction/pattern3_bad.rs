// TARGET: providers/provider_url.rs
// EXPECT: defined exactly once
use reqwest as _;
fn redacted(&self) -> String { one() }
fn redacted(&self) -> String { two() }
