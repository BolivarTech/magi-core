// Fixture: a provider that builds its HTTP client WITHOUT turning Referer off.
//
// The leak is invisible in this file — nothing here renders a URL. It happens on a redirect, where
// the client hands the original url (query string and all) to the target origin as `Referer`.
use std::time::Duration;

pub(crate) struct Subject {
    client: reqwest::Client,
}

impl Subject {
    pub(crate) fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { client }
    }
}
