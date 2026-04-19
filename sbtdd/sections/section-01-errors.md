# Section 01: Foundation -- Error Types (`error.rs`)

## Overview

This section implements `MagiError` and `ProviderError`, the two error enums that form the foundation for every other module in magi-core. Both enums use `thiserror` for `Display` and `Error` derives. No other module can compile without these types, so this section has zero internal dependencies and blocks all other sections.

## Dependencies

- **External crate**: `thiserror = "2"` (must be added to `Cargo.toml` under `[dependencies]`)
- **Standard library**: `std::io::Error`, `std::fmt`
- **Other sections**: None. This section is the root of the dependency graph.

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/Cargo.toml` | Add `thiserror = "2"`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"` to `[dependencies]` |
| `magi-core/src/error.rs` | Create -- contains `ProviderError` and `MagiError` enums |
| `magi-core/src/lib.rs` | Create or modify -- declare `pub mod error;` |

Note: `serde` and `serde_json` are added now because `From<serde_json::Error>` is required for `MagiError`. They will also be used by later sections.

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/error.rs` inside a `#[cfg(test)] mod tests` block. Write these tests before any implementation. They must fail (or not compile) until the Green phase.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // -- ProviderError Display tests --

    /// ProviderError::Http contains status code and body in Display output.
    #[test]
    fn test_provider_error_http_display_contains_status_and_body() {
        // Construct Http variant with status=500 and body="Internal Server Error"
        // Assert Display output contains "500" and "Internal Server Error"
    }

    /// ProviderError::Process includes exit_code and stderr in Display.
    #[test]
    fn test_provider_error_process_display_includes_exit_code_and_stderr() {
        // Construct Process variant with exit_code=1 and stderr="segfault"
        // Assert Display output contains "1" and "segfault"
    }

    // -- MagiError Display tests --

    /// MagiError::Validation contains descriptive message.
    #[test]
    fn test_magi_error_validation_contains_message() {
        // Construct Validation("confidence out of range")
        // Assert Display output contains "confidence out of range"
    }

    /// MagiError::InsufficientAgents formats succeeded and required in Display.
    #[test]
    fn test_magi_error_insufficient_agents_formats_counts() {
        // Construct InsufficientAgents { succeeded: 1, required: 2 }
        // Assert Display output contains "1" and "2"
    }

    /// MagiError::InputTooLarge formats size and max in Display.
    #[test]
    fn test_magi_error_input_too_large_formats_size_and_max() {
        // Construct InputTooLarge { size: 2_000_000, max: 1_048_576 }
        // Assert Display output contains "2000000" and "1048576"
    }

    // -- From impls --

    /// From<ProviderError> for MagiError wraps correctly into Provider variant.
    #[test]
    fn test_from_provider_error_wraps_into_magi_error_provider() {
        // let pe = ProviderError::Timeout { ... };
        // let me: MagiError = pe.into();
        // Assert matches MagiError::Provider(_)
    }

    /// From<serde_json::Error> for MagiError produces Deserialization variant.
    #[test]
    fn test_from_serde_json_error_produces_deserialization_variant() {
        // Force a serde_json::Error (e.g., serde_json::from_str::<String>("not json"))
        // Convert to MagiError, assert matches MagiError::Deserialization(_)
    }

    /// From<std::io::Error> for MagiError produces Io variant.
    #[test]
    fn test_from_io_error_produces_io_variant() {
        // let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        // let me: MagiError = io_err.into();
        // Assert matches MagiError::Io(_)
    }
}
```

## Implementation Details (Green Phase)

### `ProviderError` Enum

An enum representing errors originating from LLM provider implementations. Each variant uses `#[error("...")]` from `thiserror` for automatic `Display`:

- **`Http`** -- fields: `status: u16`, `body: String`. Display should include both the status code and body text.
- **`Network`** -- field: `message: String`. Generic network-level failures.
- **`Timeout`** -- field: `message: String`. Provider did not respond within the allowed time.
- **`Auth`** -- field: `message: String`. Authentication or authorization failures.
- **`Process`** -- fields: `exit_code: Option<i32>`, `stderr: String`. For CLI-subprocess providers when the child process fails.
- **`NestedSession`** -- no fields. Detected when a CLI provider would create a nested session (e.g., `CLAUDECODE` env var present).

Derive: `Debug`, `Clone`, `thiserror::Error`.

### `MagiError` Enum

The unified error type for the entire crate. All public APIs return `Result<T, MagiError>`.

- **`Validation(String)`** -- invalid input or schema violation. The string describes which field failed and why.
- **`Provider(ProviderError)`** -- wraps a provider error. Implement `From<ProviderError>`.
- **`InsufficientAgents`** -- fields: `succeeded: usize`, `required: usize`. Fewer agents completed successfully than the minimum threshold.
- **`Deserialization(String)`** -- JSON parse failures. Implement `From<serde_json::Error>` converting the serde error's `Display` output into this variant's string.
- **`InputTooLarge`** -- fields: `size: usize`, `max: usize`. Content exceeds configured maximum.
- **`Io(std::io::Error)`** -- filesystem I/O errors. Implement `From<std::io::Error>`. Note: `std::io::Error` does not implement `PartialEq`, so `MagiError` cannot derive `PartialEq` (use `matches!` macro in tests instead).

Derive: `Debug`, `thiserror::Error`.

### `From` Implementations

All three `From` conversions are handled via `#[from]` attribute on the relevant variants:

- `#[from]` on `Provider(ProviderError)` -- generates `From<ProviderError> for MagiError`
- `#[from]` on `Io(std::io::Error)` -- generates `From<std::io::Error> for MagiError`
- For `serde_json::Error` -- cannot use `#[from]` directly since the variant stores a `String`, not the error itself. Implement `From<serde_json::Error>` manually, converting via `.to_string()`.

### `lib.rs` Module Declaration

Add `pub mod error;` to `src/lib.rs`. This is the only module declaration needed for this section. Later sections will add their own module declarations.

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- All public types and variants have `///` Rustdoc
- `#[error("...")]` attributes provide clear, actionable messages with interpolated fields
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files

## Refactor Phase Notes

After Green phase passes all tests:

- Verify `Display` output is human-readable and includes all relevant context
- Ensure error messages are consistent in style (e.g., all lowercase after the colon, or all sentence case -- pick one and be consistent)
- Add Rustdoc `///` on the enum-level and each variant explaining when it is produced
- Confirm `cargo doc --no-deps` generates clean documentation
