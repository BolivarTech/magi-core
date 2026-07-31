// TARGET: providers/subject.rs
//! The same client, built so the referer default can be turned off.

use reqwest::Client;

pub fn build() -> Client {
    Client::builder().referer(false).build().unwrap_or_default()
}
