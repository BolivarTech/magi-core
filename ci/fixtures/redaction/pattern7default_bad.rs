// TARGET: providers/subject.rs
// EXPECT: builds a default HTTP client
//! `Client::default()` is the same default client as `Client::new()`, spelled shorter.

use reqwest::Client;

pub fn build() -> Client {
    Client::default()
}
