// TARGET: providers/subject.rs
// EXPECT: stores base_url as String
use reqwest as _;
    #[cfg(test)]
    mod helpers {}
pub struct P {
    base_url: String,
}
