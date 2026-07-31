// TARGET: providers/subject.rs
// EXPECT: cannot disable referer
use reqwest as _;
fn f() {
    let c = reqwest::Client::new();
}
