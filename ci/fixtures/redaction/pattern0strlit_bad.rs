// TARGET: providers/subject.rs
// EXPECT: stores base_url as String
// A `//` inside a string literal is not a comment. Cutting there erased the rest of the line from
// every rule — a false NEGATIVE, the direction that hides a leak.
use reqwest as _;
pub struct P {
    doc: &'static str,
    base_url: String,
}
const D: &str = "see http://example.com // for details";
