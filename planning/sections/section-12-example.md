# Section 12: Example Binary (`examples/basic_analysis.rs`)

## Overview

This section implements an example binary that demonstrates how to use the magi-core library for a basic multi-perspective analysis. The example defaults to `ClaudeCliProvider` (since it requires no API key configuration) and supports CLI arguments for selecting alternative providers. It showcases the builder pattern, provider configuration, and the full analysis flow including report output. This is the final section and depends on all other sections being complete.

## Dependencies

- **Internal sections**: All sections 01-11 must be complete.
  - The example uses types from `prelude` (Section 11)
  - It instantiates `ClaudeCliProvider` (Section 10) by default
  - Optionally instantiates `ClaudeProvider` (Section 09) via CLI args
  - Uses `Magi`, `MagiBuilder`, `Mode`, `MagiReport` from the orchestrator and schema
- **External crates**:
  - `tokio` -- async runtime with `#[tokio::main]`
  - No additional crates beyond what magi-core already provides
- **Feature flags**: The example requires at least one provider feature to be enabled. Default: `claude-cli`. The example should document which features to enable when compiling.

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/examples/basic_analysis.rs` | Create -- example binary |
| `magi-core/Cargo.toml` | Verify `[[example]]` section exists (Cargo auto-discovers examples, but explicit entry allows specifying required features) |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

Example binaries are not unit-tested in the traditional sense. However, they must compile and the logic must be sound. Verification is done via compilation tests.

```rust
// No #[cfg(test)] block for example binaries.
// Verification strategy:
//
// 1. cargo build --example basic_analysis --features claude-cli
//    → Must compile without errors
//
// 2. cargo build --example basic_analysis --features claude-api
//    → Must compile without errors (API provider path)
//
// 3. cargo build --example basic_analysis --features claude-api,claude-cli
//    → Must compile without errors (both providers available)
//
// 4. The example should print a helpful error message if no provider
//    feature is enabled at compile time (use compile_error! or a
//    clear runtime message).
```

## Implementation Details (Green Phase)

### `examples/basic_analysis.rs`

The example binary demonstrates a minimal but complete usage of the magi-core API.

#### Structure

1. **Parse CLI arguments**: simple `std::env::args()` parsing (no external arg-parsing crate needed). Arguments:
   - `--provider <name>` -- selects the provider: `"cli"` (default) or `"api"`
   - `--model <model>` -- model alias or identifier (default: `"sonnet"`)
   - `--mode <mode>` -- analysis mode: `"code-review"` (default), `"design"`, `"analysis"`
   - `--api-key <key>` -- API key for `ClaudeProvider` (required when `--provider api`)
   - `--input <text>` or read from stdin -- the content to analyze
   - `--help` -- print usage information

2. **Create provider**: based on the `--provider` argument:
   - `"cli"` (default): `ClaudeCliProvider::new(model)?` -- feature-gated with `#[cfg(feature = "claude-cli")]`
   - `"api"`: `ClaudeProvider::new(api_key, model)` -- feature-gated with `#[cfg(feature = "claude-api")]`
   - If the requested provider feature is not enabled, print a compile-time or runtime error message explaining which feature to enable

3. **Build Magi**: use `Magi::new(provider)` for the simple case, or demonstrate `MagiBuilder` for the configured case:
   ```rust
   let magi = MagiBuilder::new(provider)
       .with_timeout(Duration::from_secs(120))
       .build()?;
   ```

4. **Run analysis**: call `magi.analyze(&mode, &content).await?`

5. **Print output**: display the formatted report to stdout:
   ```rust
   println!("{}", report.banner);
   println!("{}", report.report);
   ```
   Optionally, print the JSON-serialized `MagiReport` when a `--json` flag is passed.

#### Example Code Skeleton

```rust
//! Basic MAGI analysis example.
//!
//! # Usage
//!
//! ```bash
//! # Using CLI provider (default):
//! cargo run --example basic_analysis --features claude-cli -- --input "fn main() {}"
//!
//! # Using API provider:
//! cargo run --example basic_analysis --features claude-api -- \
//!   --provider api --api-key sk-... --input "fn main() {}"
//!
//! # Reading from stdin:
//! cat src/main.rs | cargo run --example basic_analysis --features claude-cli
//! ```

use magi_core::prelude::*;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI args
    // Create provider (feature-gated)
    // Build Magi (simple or builder)
    // Read input content
    // Run analysis
    // Print report
}
```

### CLI Argument Parsing

Use a simple hand-rolled parser with `std::env::args()`. The example should be self-contained without pulling in `clap` or other arg-parsing crates. This keeps the example dependency-light and focused on demonstrating magi-core usage.

Parse arguments in a loop:
- Match `"--provider"`, `"--model"`, `"--mode"`, `"--api-key"`, `"--input"`, `"--json"`, `"--help"`
- For unknown arguments, print an error and exit
- For `--help`, print usage text and exit with code 0

### Provider Selection

The provider selection logic uses `cfg` attributes to ensure compile-time safety:

```rust
fn create_provider(
    provider_name: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<Arc<dyn LlmProvider>, Box<dyn std::error::Error>> {
    match provider_name {
        #[cfg(feature = "claude-cli")]
        "cli" => Ok(Arc::new(ClaudeCliProvider::new(model)?)),

        #[cfg(feature = "claude-api")]
        "api" => {
            let key = api_key.ok_or("--api-key is required for API provider")?;
            Ok(Arc::new(ClaudeProvider::new(key, model)))
        }

        other => Err(format!("Unknown provider: {other}").into()),
    }
}
```

### Mode Parsing

Convert the `--mode` string argument to a `Mode` enum:

```rust
fn parse_mode(s: &str) -> Result<Mode, String> {
    match s {
        "code-review" => Ok(Mode::CodeReview),
        "design" => Ok(Mode::Design),
        "analysis" => Ok(Mode::Analysis),
        other => Err(format!("Unknown mode: {other}. Use: code-review, design, analysis")),
    }
}
```

### Input Reading

If `--input` is provided, use that text directly. Otherwise, read from stdin until EOF:

```rust
fn read_input(input_arg: Option<String>) -> Result<String, std::io::Error> {
    match input_arg {
        Some(text) => Ok(text),
        None => {
            let mut buffer = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)?;
            Ok(buffer)
        }
    }
}
```

### Cargo.toml Example Entry (Optional)

Cargo auto-discovers files in `examples/`, but an explicit entry can specify required features:

```toml
[[example]]
name = "basic_analysis"
required-features = []  # No required features -- handles missing features at compile/runtime
```

### Error Handling

The example uses `Box<dyn std::error::Error>` as the return type from `main()` for simplicity. All errors are propagated with `?`. The example should print a user-friendly message before exiting on error, not a raw debug dump.

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` in example code -- use `?` throughout
- No `unsafe`
- Example compiles with `--features claude-cli`, `--features claude-api`, or both
- Example does not require any external crates beyond what magi-core provides
- CLI argument parsing is hand-rolled (no `clap` dependency)
- API key is never hardcoded -- must be provided via `--api-key` argument
- Feature-gated code uses `#[cfg(feature = "...")]` for compile-time safety
- Module-level `//!` Rustdoc with usage examples showing compilation commands
- File header present
- `rustfmt` and `clippy -- -D warnings` clean

## Refactor Phase Notes

After Green phase passes (example compiles):

- Verify `cargo run --example basic_analysis --features claude-cli -- --help` prints useful help text
- Ensure error messages are user-friendly (e.g., "API key required" not a raw debug dump)
- Add comments in the example code explaining each step for educational value
- Verify stdin reading works correctly on both Unix and Windows
- Consider adding a `--timeout` CLI argument to demonstrate `MagiBuilder` configuration
- Confirm the example appears in `cargo doc --no-deps` generated documentation
- Test that the example compiles with all feature flag combinations
