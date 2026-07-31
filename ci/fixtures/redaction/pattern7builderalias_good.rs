// TARGET: providers/subject.rs
//! Both builders configured, one written each way.

use reqwest::{Client, ClientBuilder};

pub fn configured() -> Client {
    Client::builder().referer(false).build().unwrap_or_default()
}

pub fn also_configured() -> Client {
    ClientBuilder::new().referer(false).build().unwrap_or_default()
}
