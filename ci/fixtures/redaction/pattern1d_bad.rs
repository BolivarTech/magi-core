// TARGET: providers/subject.rs
// EXPECT: transport error constructed
use reqwest as _;
fn f() {
    let e = ProviderError::Network { message: "x".into() };
}
