// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

//! Basic MAGI analysis example.
//!
//! Demonstrates how to use the magi-core library for multi-perspective analysis
//! using the MAGI system (Melchior, Balthasar, Caspar).
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
//! # Using OpenAI-compatible provider (e.g. local Ollama):
//! cargo run --example basic_analysis --features openai-compat -- \
//!   --provider openai-compat --base-url http://127.0.0.1:11434/v1 \
//!   --model phi4-mini --timeout 300 --input "fn main() {}"
//!
//! # OpenAI-compatible provider — dead port (connection refused, fast failure):
//! cargo run --example basic_analysis --features openai-compat -- \
//!   --provider openai-compat --base-url http://127.0.0.1:9/v1 \
//!   --model phi4-mini --input "fn main() {}"
//!
//! # Specify mode and model:
//! cargo run --example basic_analysis --features claude-cli -- \
//!   --mode design --model opus --input "Propose a caching layer"
//!
//! # Reading from stdin:
//! cat src/main.rs | cargo run --example basic_analysis --features claude-cli
//!
//! # JSON output:
//! cargo run --example basic_analysis --features claude-cli -- \
//!   --json --input "fn main() {}"
//! ```

use magi_core::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// Configures the console output codepage to UTF-8 on Windows so that
/// subsequent `println!` calls can emit multibyte UTF-8 sequences (em-dash,
/// ellipsis, etc.) without panicking on cp1252-default consoles.
///
/// On non-Windows platforms this is a no-op — terminals are already UTF-8
/// by default on Linux/macOS.
///
/// **MAGI R1 W15:** the SetConsoleOutputCP return value is checked; a failed
/// call (e.g., stdout redirected to a file with no console attached) emits
/// a stderr warning instead of being silently ignored. Downstream consumers
/// that pipe the example's output may still see codepage-related corruption
/// in that case, but the warning makes it diagnosable.
#[cfg(windows)]
fn setup_console_encoding() {
    // SAFETY: SetConsoleOutputCP is a Win32 API that takes a single u32
    // by value and returns a BOOL (i32 — nonzero on success, zero on
    // failure). It accesses no shared memory, has no aliasing concerns,
    // and is documented thread-safe by Microsoft. Calling it once at
    // process start with CP_UTF8 (65001) is the canonical way to
    // configure UTF-8 console output on Windows.
    const CP_UTF8: u32 = 65001;
    unsafe extern "system" {
        fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
    }
    let ok = unsafe { SetConsoleOutputCP(CP_UTF8) };
    if ok == 0 {
        eprintln!(
            "warning: SetConsoleOutputCP(CP_UTF8) failed (likely no console attached); \
             UTF-8 output may be corrupted in downstream consumers"
        );
    }
}

#[cfg(not(windows))]
fn setup_console_encoding() {}

#[cfg(test)]
mod tests {
    use super::*;

    /// MAGI R1 W11 regression guard: ensures `setup_console_encoding`
    /// compiles and runs without panicking on both Windows and non-Windows.
    /// Does NOT verify the side effect (codepage change) — that requires a
    /// manual smoke test on a Windows console.
    #[test]
    fn test_setup_console_encoding_runs_without_panic() {
        setup_console_encoding();
    }
}

/// Prints usage information and exits.
fn print_usage() {
    eprintln!(
        "Usage: basic_analysis [OPTIONS]

Options:
  --provider <name>   Provider to use: \"cli\" (default), \"api\", or \"openai-compat\"
  --model <model>     Model alias or identifier (default: \"sonnet\")
  --mode <mode>       Analysis mode: \"code-review\" (default), \"design\", \"analysis\"
  --api-key <key>     API key (required with --provider api; optional for openai-compat)
  --base-url <url>    Base URL for OpenAI-compatible endpoint (required with --provider openai-compat)
  --input <text>      Content to analyze (reads from stdin if omitted)
  --timeout <secs>    Timeout per agent in seconds (default: 120)
  --json              Output the full MagiReport as JSON
  --help              Show this help message"
    );
}

/// Parses a mode string into a [`Mode`] enum value.
fn parse_mode(s: &str) -> Result<Mode, String> {
    match s {
        "code-review" => Ok(Mode::CodeReview),
        "design" => Ok(Mode::Design),
        "analysis" => Ok(Mode::Analysis),
        other => Err(format!(
            "Unknown mode: {other}. Use: code-review, design, analysis"
        )),
    }
}

/// Provider-construction inputs from CLI flags. Each provider reads the subset
/// it needs; stable signature as providers are added.
#[allow(dead_code)]
struct ProviderArgs<'a> {
    model: &'a str,
    api_key: Option<&'a str>,
    base_url: Option<&'a str>,
}

/// Creates the LLM provider based on CLI arguments.
///
/// The provider selection is feature-gated at compile time.
#[allow(unused_variables)]
fn create_provider(
    provider_name: &str,
    args: ProviderArgs<'_>,
) -> Result<Arc<dyn LlmProvider>, Box<dyn std::error::Error>> {
    match provider_name {
        #[cfg(feature = "claude-cli")]
        "cli" => Ok(Arc::new(ClaudeCliProvider::new(args.model)?)),
        #[cfg(not(feature = "claude-cli"))]
        "cli" => Err("CLI provider not available. Recompile with: --features claude-cli".into()),

        #[cfg(feature = "claude-api")]
        "api" => {
            let key = args
                .api_key
                .ok_or("--api-key is required when using --provider api")?;
            Ok(Arc::new(ClaudeProvider::new(key, args.model)?))
        }
        #[cfg(not(feature = "claude-api"))]
        "api" => Err("API provider not available. Recompile with: --features claude-api".into()),

        #[cfg(feature = "openai-compat")]
        "openai-compat" => {
            let base_url = args
                .base_url
                .ok_or("--base-url is required when using --provider openai-compat")?;
            Ok(Arc::new(OpenAiCompatibleProvider::new(
                base_url,
                args.model,
                args.api_key.map(|k| k.to_string()),
            )?))
        }
        #[cfg(not(feature = "openai-compat"))]
        "openai-compat" => Err(
            "openai-compat provider not available. Recompile with: --features openai-compat".into(),
        ),

        other => Err(format!("Unknown provider: {other}. Use: cli, api, openai-compat").into()),
    }
}

/// Reads input content from the `--input` argument or stdin.
fn read_input(input_arg: Option<String>) -> Result<String, std::io::Error> {
    match input_arg {
        Some(text) => Ok(text),
        None => {
            eprintln!("Reading input from stdin (press Ctrl+D to finish)...");
            let mut buffer = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)?;
            Ok(buffer)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_console_encoding();

    // Default argument values. `model` defaults to None; if the user does
    // not pass --model, we resolve it via `default_model_for_mode(mode)`
    // once mode is parsed (v0.4.0, Python v2.2.3 parity).
    let mut provider_name = "cli".to_string();
    let mut model: Option<String> = None;
    let mut mode_str = "code-review".to_string();
    let mut api_key: Option<String> = None;
    let mut base_url: Option<String> = None;
    let mut input: Option<String> = None;
    let mut timeout_secs: u64 = 120;
    let mut json_output = false;

    // Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            "--provider" => {
                i += 1;
                provider_name = args.get(i).ok_or("--provider requires a value")?.clone();
            }
            "--model" => {
                i += 1;
                model = Some(args.get(i).ok_or("--model requires a value")?.clone());
            }
            "--mode" => {
                i += 1;
                mode_str = args.get(i).ok_or("--mode requires a value")?.clone();
            }
            "--api-key" => {
                i += 1;
                api_key = Some(args.get(i).ok_or("--api-key requires a value")?.clone());
            }
            "--base-url" => {
                i += 1;
                base_url = Some(args.get(i).ok_or("--base-url requires a value")?.clone());
            }
            "--input" => {
                i += 1;
                input = Some(args.get(i).ok_or("--input requires a value")?.clone());
            }
            "--timeout" => {
                i += 1;
                timeout_secs = args
                    .get(i)
                    .ok_or("--timeout requires a value")?
                    .parse::<u64>()
                    .map_err(|e| format!("Invalid timeout value: {e}"))?;
            }
            "--json" => {
                json_output = true;
            }
            other => {
                return Err(format!("Unknown argument: {other}. Use --help for usage.").into());
            }
        }
        i += 1;
    }

    // Parse mode
    let mode = parse_mode(&mode_str).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    // Resolve model: explicit --model wins; otherwise use the mode default
    // (v0.4.0 / Python v2.2.3 MODE_DEFAULT_MODELS parity).
    let model = model.unwrap_or_else(|| default_model_for_mode(mode).to_string());

    // Create the provider
    let provider = create_provider(
        &provider_name,
        ProviderArgs {
            model: &model,
            api_key: api_key.as_deref(),
            base_url: base_url.as_deref(),
        },
    )?;

    // Build the Magi orchestrator with timeout configuration
    let magi = Magi::builder(provider)
        .with_timeout(Duration::from_secs(timeout_secs))
        .build()?;

    // Read input content
    let content = read_input(input)?;
    if content.trim().is_empty() {
        return Err("Input content is empty. Provide text via --input or stdin.".into());
    }

    // Run the analysis
    eprintln!("Running MAGI analysis (mode: {mode}, model: {model})...");
    let report = magi.analyze(&mode, &content).await?;

    // Output results
    if json_output {
        let json = serde_json::to_string_pretty(&report)?;
        println!("{json}");
    } else {
        println!("{}", report.report);
    }

    Ok(())
}
