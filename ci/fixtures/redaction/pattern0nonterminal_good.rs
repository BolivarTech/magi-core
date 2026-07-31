// TARGET: providers/subject.rs
use reqwest as _;
#[cfg(test)]
use serial_test::serial;
pub struct P {
    base_url: ProviderUrl,
}
#[cfg(test)]
mod tests {
    fn t() { let base_url: String = String::new(); }
}
