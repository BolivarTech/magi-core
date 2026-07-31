// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
use reqwest as _;
fn f() {
    let a = error.to_string();
}
