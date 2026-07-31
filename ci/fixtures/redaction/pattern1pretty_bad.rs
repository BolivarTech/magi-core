// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
// Pretty-printed Debug: the same leak as `{e:?}`, one character away, and the form a developer
// reaches for when the one-line version is hard to read.
use reqwest as _;
fn f() {
    let a = format!("failed: {e:#?}");
}
