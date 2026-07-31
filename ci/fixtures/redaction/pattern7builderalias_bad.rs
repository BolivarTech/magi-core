// TARGET: providers/subject.rs
// EXPECT: disable referer
//! Two builders, one configured. Written with the long spelling, which the tally used to miss.

use reqwest::{Client, ClientBuilder};

pub fn configured() -> Client {
    Client::builder().referer(false).build().unwrap_or_default()
}

pub fn bare() -> Client {
    ClientBuilder::new().build().unwrap_or_default()
}
