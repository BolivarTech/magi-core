// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-05-25

use crate::error::ProviderError;
use std::fmt;

pub struct OpenAiCompatibleProvider {
    #[allow(dead_code)] // used by complete() in Task 6
    client: reqwest::Client,
    base_url: String,
    model: String,
    #[allow(dead_code)] // used by auth_header()/complete() in Tasks 3/6
    api_key: Option<String>,
}

impl fmt::Debug for OpenAiCompatibleProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiCompatibleProvider")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

impl OpenAiCompatibleProvider {
    pub fn new(
        _base_url: impl Into<String>,
        _model: impl Into<String>,
        _api_key: Option<String>,
    ) -> Result<Self, ProviderError> {
        todo!("Task 1 Green")
    }

    pub fn name(&self) -> &str {
        todo!("Task 1 Green")
    }

    pub fn model(&self) -> &str {
        todo!("Task 1 Green")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_valid_url_ok_and_model_passthrough() {
        let p = OpenAiCompatibleProvider::new("http://127.0.0.1:11434/v1", "phi4-mini", None)
            .expect("valid url constructs");
        assert_eq!(p.model(), "phi4-mini");
        assert_eq!(p.name(), "openai-compat");
    }

    #[test]
    fn test_new_invalid_url_returns_network_error() {
        let result = OpenAiCompatibleProvider::new("notaurl", "m", None);
        assert!(matches!(result, Err(ProviderError::Network { .. })));
    }

    #[test]
    fn test_new_non_http_scheme_returns_network_error() {
        let result = OpenAiCompatibleProvider::new("file:///etc/passwd", "m", None);
        assert!(matches!(result, Err(ProviderError::Network { .. })));
    }

    #[test]
    fn test_new_cloud_with_key_passthrough() {
        let p = OpenAiCompatibleProvider::new(
            "https://api.openai.com/v1",
            "gpt-4o",
            Some("sk-x".into()),
        )
        .expect("valid url constructs");
        assert_eq!(p.model(), "gpt-4o");
    }

    #[test]
    fn test_debug_redacts_api_key() {
        let p = OpenAiCompatibleProvider::new("http://h/v1", "m", Some("sk-super-secret".into()))
            .expect("constructs");
        let dbg = format!("{p:?}");
        assert!(
            !dbg.contains("sk-super-secret"),
            "Debug must not leak key, got: {dbg}"
        );
    }
}
