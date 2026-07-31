// TARGET: providers/subject.rs
use reqwest as _;
fn f() {
    // A second Client::builder() would need referer(false) too — this line is prose, and
    // counting it would fail a file that is actually correct.
    let c = reqwest::Client::builder().referer(false).build();
}
