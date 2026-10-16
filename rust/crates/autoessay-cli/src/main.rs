//! Binary entry point for the autoessay CLI.

use clap::Parser;

use autoessay_cli::cli::{run_with, Cli, EXIT_ERROR};
use autoessay_cli::llm::ProviderLlm;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

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
