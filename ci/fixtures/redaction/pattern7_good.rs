// Fixture: the same client, built the way this crate requires.
use std::time::Duration;

pub(crate) struct Subject {
    client: reqwest::Client,
}

impl Subject {
    pub(crate) fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            // The line the gate exists to keep: without it a redirect carries the original url,
            // query string included, to the target origin.
            .referer(false)
            .build()
            .unwrap_or_default();
        Self { client }
    }
}
