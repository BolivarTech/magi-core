// TARGET: providers/provider_url.rs
// EXPECT: may return a string
use reqwest as _;
fn redacted(&self) -> String { String::new() }
impl X {
    pub(crate) async fn raw(&self) -> String { self.inner.to_string() }
}
