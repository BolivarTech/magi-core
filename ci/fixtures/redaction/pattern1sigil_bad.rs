// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
use reqwest as _;
fn f() {
    tracing::warn!(cause = %source, "boom");
}
