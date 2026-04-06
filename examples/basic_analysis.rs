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

/// Prints usage information and exits.
fn print_usage() {
    eprintln!(
        "Usage: basic_analysis [OPTIONS]

Options:
  --provider <name>   Provider to use: \"cli\" (default) or \"api\"
  --model <model>     Model alias or identifier (default: \"sonnet\")
  --mode <mode>       Analysis mode: \"code-review\" (default), \"design\", \"analysis\"
  --api-key <key>     API key for the API provider (required with --provider api)
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

/// Creates the LLM provider based on CLI arguments.
///
/// The provider selection is feature-gated at compile time.
#[allow(unused_variables)]
fn create_provider(
    provider_name: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<Arc<dyn LlmProvider>, Box<dyn std::error::Error>> {
    match provider_name {
        #[cfg(feature = "claude-cli")]
        "cli" => Ok(Arc::new(ClaudeCliProvider::new(model)?)),

        #[cfg(not(feature = "claude-cli"))]
        "cli" => Err("CLI provider not available. Recompile with: --features claude-cli".into()),

        #[cfg(feature = "claude-api")]
        "api" => {
            let key = api_key.ok_or("--api-key is required when using --provider api")?;
            Ok(Arc::new(ClaudeProvider::new(key, model)?))
        }

        #[cfg(not(feature = "claude-api"))]
        "api" => Err("API provider not available. Recompile with: --features claude-api".into()),

        other => Err(format!("Unknown provider: {other}. Use: cli, api").into()),
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
    // Default argument values
    let mut provider_name = "cli".to_string();
    let mut model = "opus".to_string();
    let mut mode_str = "code-review".to_string();
    let mut api_key: Option<String> = None;
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
                model = args.get(i).ok_or("--model requires a value")?.clone();
            }
            "--mode" => {
                i += 1;
                mode_str = args.get(i).ok_or("--mode requires a value")?.clone();
            }
            "--api-key" => {
                i += 1;
                api_key = Some(args.get(i).ok_or("--api-key requires a value")?.clone());
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

    // Create the provider
    let provider = create_provider(&provider_name, &model, api_key.as_deref())?;

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
