// TARGET: providers/subject.rs
use reqwest as _;
fn f() {
    let c = reqwest::Client::builder().referer(false).build();
}
