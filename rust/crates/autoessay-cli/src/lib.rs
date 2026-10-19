//! autoessay-cli — the pipeline phases and end-to-end orchestrator for the
//! autoessay non-fiction essay generator.
//!
//! This crate is the Rust port of the Python autoessay pipeline. It contains
//! the five pipeline phases (research, outline, drafting, evaluation,
//! revision), the orchestrator that runs them end to end, and a clap-based CLI.
//!
//! Everything is designed for offline testability: every phase function takes
//! "&impl LlmClient", so a test hands it a MockLlm with canned responses and no
//! HTTP is ever touched. See llm.rs for the seam and tests/ for the coverage.

pub mod cli;
pub mod docs;
pub mod domains;
pub mod error;
pub mod llm;
pub mod orchestrator;
pub mod phases;
pub mod sources;
pub mod styles;
pub mod verify;

pub use error::{CliError, CliResult};
pub use llm::{
    ChatMessage, ChatRequest, LlmClient, LlmResponse, MockLlm, ProviderLlm, Route, UsageTotals,
};
