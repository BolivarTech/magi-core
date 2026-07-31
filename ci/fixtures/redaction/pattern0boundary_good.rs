// TARGET: providers/subject.rs
use reqwest as _;
#[cfg(all(test, feature = "x"))]
mod tests {
    fn t() { let base_url: String = String::new(); }
}
