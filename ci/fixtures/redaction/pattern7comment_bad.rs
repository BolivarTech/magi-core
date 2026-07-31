// TARGET: providers/subject.rs
// EXPECT: disable referer
// The mirror: a comment claiming referer(false) must not satisfy the rule for real code that
// never calls it.
use reqwest as _;
fn f() {
    // we should call referer(false) here one day
    let c = reqwest::Client::builder().timeout(t).build();
}
