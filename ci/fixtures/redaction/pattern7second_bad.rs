// TARGET: providers/subject.rs
// EXPECT: but only 1 disable referer
// Two clients, one configured. A file-level check is satisfied by the first and never looks at
// the second — which is the shape a provider module with a completions client and a probe client
// actually has.
use reqwest as _;
fn completions() {
    let c = reqwest::Client::builder().referer(false).build();
}
fn probe() {
    let c = reqwest::Client::builder().timeout(t).build();
}
