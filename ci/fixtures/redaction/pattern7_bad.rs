// TARGET: providers/subject.rs
// EXPECT: without referer(false)
use reqwest as _;
fn f() {
    let c = reqwest::Client::builder().timeout(t).build();
}
