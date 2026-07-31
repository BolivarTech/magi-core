// TARGET: providers/subject.rs
use reqwest as _;
pub struct P {
    doc: &'static str,
    base_url: ProviderUrl,
}
const D: &str = "see http://example.com // for details";
