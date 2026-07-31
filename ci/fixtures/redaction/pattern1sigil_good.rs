// TARGET: providers/subject.rs
use reqwest as _;
fn f() { tracing::warn!(url = %redacted, "boom"); }
