impl From<reqwest::Error> for ProviderError { fn from(_: reqwest::Error) -> Self { unimplemented!() } }
