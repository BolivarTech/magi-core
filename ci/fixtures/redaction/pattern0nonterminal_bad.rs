// TARGET: providers/subject.rs
// EXPECT: stores base_url as String
use reqwest as _;
#[cfg(test)]
use serial_test::serial;
pub struct P {
    base_url: String,
}
#[cfg(test)]
mod tests {
    fn t() {}
}
