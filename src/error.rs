// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use thiserror::Error;

/// Errors originating from LLM provider implementations.
///
/// Each variant represents a distinct failure mode that providers
/// can encounter when communicating with LLM backends.
#[derive(Debug, Clone, Error)]
pub enum ProviderError {
    /// HTTP response with a non-success status code.
    #[error("http error {status}: {body}")]
    Http {
        /// HTTP status code.
        status: u16,
        /// Response body text.
        body: String,
    },

    /// Network-level failure (DNS, connection refused, etc.).
    #[error("network error: {message}")]
    Network {
        /// Description of the network failure.
        message: String,
    },

    /// Provider did not respond within the allowed time.
    #[error("timeout: {message}")]
    Timeout {
        /// Description of the timeout condition.
        message: String,
    },

    /// Authentication or authorization failure.
    #[error("auth error: {message}")]
    Auth {
        /// Description of the authentication failure.
        message: String,
    },

    /// CLI subprocess provider failed.
    #[error("process error (exit_code={exit_code:?}): {stderr}")]
    Process {
        /// Exit code of the child process, if available.
        exit_code: Option<i32>,
        /// Standard error output from the child process.
        stderr: String,
    },

    /// Detected nested session (e.g., `CLAUDECODE` env var present).
    #[error("nested session detected: cannot launch CLI provider from within an existing session")]
    NestedSession,
}

/// Unified error type for the magi-core crate.
///
/// All public APIs return `Result<T, MagiError>`. This enum unifies
/// provider errors, validation failures, and I/O errors into a single type.
#[derive(Debug, Error)]
pub enum MagiError {
    /// Invalid input or schema violation.
    #[error("validation error: {0}")]
    Validation(String),

    /// Wraps a provider-specific error.
    #[error(transparent)]
    Provider(#[from] ProviderError),

    /// Fewer agents completed successfully than the minimum threshold.
    #[error("insufficient agents: {succeeded} succeeded, {required} required")]
    InsufficientAgents {
        /// Number of agents that completed successfully.
        succeeded: usize,
        /// Minimum number of agents required.
        required: usize,
    },

    /// JSON deserialization failure.
    #[error("deserialization error: {0}")]
    Deserialization(String),

    /// Content exceeds configured maximum input size.
    #[error("input too large: {size} bytes exceeds maximum of {max} bytes")]
    InputTooLarge {
        /// Actual size of the input in bytes.
        size: usize,
        /// Maximum allowed size in bytes.
        max: usize,
    },

    /// Filesystem I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<serde_json::Error> for MagiError {
    fn from(err: serde_json::Error) -> Self {
        MagiError::Deserialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- ProviderError Display tests --

    /// ProviderError::Http contains status code and body in Display output.
    #[test]
    fn test_provider_error_http_display_contains_status_and_body() {
        let err = ProviderError::Http {
            status: 500,
            body: "Internal Server Error".to_string(),
        };
        let display = format!("{err}");
        assert!(
            display.contains("500"),
            "Display should contain status code"
        );
        assert!(
            display.contains("Internal Server Error"),
            "Display should contain body"
        );
    }

    /// ProviderError::Network includes message in Display.
    #[test]
    fn test_provider_error_network_display_contains_message() {
        let err = ProviderError::Network {
            message: "connection refused".to_string(),
        };
        let display = format!("{err}");
        assert!(
            display.contains("connection refused"),
            "Display should contain message"
        );
    }

    /// ProviderError::Timeout includes message in Display.
    #[test]
    fn test_provider_error_timeout_display_contains_message() {
        let err = ProviderError::Timeout {
            message: "exceeded 30s".to_string(),
        };
        let display = format!("{err}");
        assert!(
            display.contains("exceeded 30s"),
            "Display should contain message"
        );
    }

    /// ProviderError::Auth includes message in Display.
    #[test]
    fn test_provider_error_auth_display_contains_message() {
        let err = ProviderError::Auth {
            message: "invalid api key".to_string(),
        };
        let display = format!("{err}");
        assert!(
            display.contains("invalid api key"),
            "Display should contain message"
        );
    }

    /// ProviderError::Process includes exit_code and stderr in Display.
    #[test]
    fn test_provider_error_process_display_includes_exit_code_and_stderr() {
        let err = ProviderError::Process {
            exit_code: Some(1),
            stderr: "segfault".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("1"), "Display should contain exit code");
        assert!(
            display.contains("segfault"),
            "Display should contain stderr"
        );
    }

    /// ProviderError::Process with no exit code still displays stderr.
    #[test]
    fn test_provider_error_process_display_none_exit_code() {
        let err = ProviderError::Process {
            exit_code: None,
            stderr: "killed".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("killed"), "Display should contain stderr");
    }

    /// ProviderError::NestedSession has a meaningful Display.
    #[test]
    fn test_provider_error_nested_session_display() {
        let err = ProviderError::NestedSession;
        let display = format!("{err}");
        assert!(!display.is_empty(), "Display should not be empty");
    }

    // -- MagiError Display tests --

    /// MagiError::Validation contains descriptive message.
    #[test]
    fn test_magi_error_validation_contains_message() {
        let err = MagiError::Validation("confidence out of range".to_string());
        let display = format!("{err}");
        assert!(
            display.contains("confidence out of range"),
            "Display should contain validation message"
        );
    }

    /// MagiError::InsufficientAgents formats succeeded and required in Display.
    #[test]
    fn test_magi_error_insufficient_agents_formats_counts() {
        let err = MagiError::InsufficientAgents {
            succeeded: 1,
            required: 2,
        };
        let display = format!("{err}");
        assert!(
            display.contains("1"),
            "Display should contain succeeded count"
        );
        assert!(
            display.contains("2"),
            "Display should contain required count"
        );
    }

    /// MagiError::InputTooLarge formats size and max in Display.
    #[test]
    fn test_magi_error_input_too_large_formats_size_and_max() {
        let err = MagiError::InputTooLarge {
            size: 2_000_000,
            max: 1_048_576,
        };
        let display = format!("{err}");
        assert!(
            display.contains("2000000"),
            "Display should contain actual size"
        );
        assert!(
            display.contains("1048576"),
            "Display should contain max size"
        );
    }

    // -- From impls --

    /// From<ProviderError> for MagiError wraps correctly into Provider variant.
    #[test]
    fn test_from_provider_error_wraps_into_magi_error_provider() {
        let pe = ProviderError::Timeout {
            message: "timed out".to_string(),
        };
        let me: MagiError = pe.into();
        assert!(
            matches!(me, MagiError::Provider(_)),
            "Should wrap into Provider variant"
        );
    }

    /// From<serde_json::Error> for MagiError produces Deserialization variant.
    #[test]
    fn test_from_serde_json_error_produces_deserialization_variant() {
        let result: Result<String, _> = serde_json::from_str("not json");
        let serde_err = result.unwrap_err();
        let me: MagiError = serde_err.into();
        assert!(
            matches!(me, MagiError::Deserialization(_)),
            "Should produce Deserialization variant"
        );
    }

    /// From<std::io::Error> for MagiError produces Io variant.
    #[test]
    fn test_from_io_error_produces_io_variant() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let me: MagiError = io_err.into();
        assert!(matches!(me, MagiError::Io(_)), "Should produce Io variant");
    }
}
