// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
use reqwest as _;
fn f() {
    let a = format!("failed: {e}");
    let b = err.to_string();
    tracing::warn!(cause = %error, "boom");
    let c = format!("{err:?}");
}
