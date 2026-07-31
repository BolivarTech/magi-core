// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
// The error is not the FIRST argument. An anchored regex saw only the first one.
use reqwest as _;
fn f() {
    let a = format!("{} failed: {}", op, e);
}
