// TARGET: providers/subject.rs
use reqwest as _;
fn completions() {
    let c = reqwest::Client::builder().referer(false).build();
}
fn probe() {
    let c = reqwest::Client::builder().timeout(t).referer(false).build();
}
