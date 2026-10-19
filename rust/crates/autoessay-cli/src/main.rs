//! Binary entry point for the autoessay CLI.

use clap::Parser;

use autoessay_cli::cli::{run_with, Cli, EXIT_ERROR};
use autoessay_cli::llm::ProviderLlm;

#[tokio::main]
async fn main() {
    // Load .env (if present) from the current directory or any ancestor, so
    // `cp .env.example .env` works like it did in the Python implementation.
    // Real environment variables keep precedence over .env values.
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    // An explicit --config wins over AUTOESSAY_CONFIG and config discovery.
    if let Some(path) = &cli.config {
        std::env::set_var("AUTOESSAY_CONFIG", path);
    }

    let llm = match ProviderLlm::from_env() {
        Ok(llm) => llm,
        Err(e) => {
            eprintln!("error: failed to initialize LLM client: {e}");
            std::process::exit(EXIT_ERROR);
        }
    };

    let code = run_with(&cli, &llm).await;
    std::process::exit(code);
}
