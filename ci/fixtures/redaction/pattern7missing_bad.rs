// TARGET: providers/subject.rs
// EXPECT: builds an HTTP client without
// A client built through the builder but never told to drop Referer. This is the half the rule
// exists for: the default carries the original URL, query included, to a redirect target, and no
// test can catch the call disappearing — only this presence rule can.
use reqwest as _;
fn f() {
    let c = reqwest::Client::builder()
        .timeout(t)
        .build();
}
