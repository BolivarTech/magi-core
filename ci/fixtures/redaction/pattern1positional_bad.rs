// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
// The form people reach for first, and the one the named-capture pattern never saw.
use reqwest as _;
fn f() {
    let a = format!("request failed: {}", e);
}
