// TARGET: providers/provider_url.rs
// EXPECT: a client type escapes
use reqwest as _;
fn redacted(&self) -> String { String::new() }
impl X {
    pub(crate) async fn leak(&self) -> Response { todo!() }
}
