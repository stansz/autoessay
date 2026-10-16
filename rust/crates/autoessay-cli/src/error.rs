//! Error type for the pipeline + CLI crate.
//!
//! There is deliberately no unwrap/expect/panic! outside of tests: the whole
//! point of the Rust rewrite is to make failure paths explicit values.

/// Errors produced by autoessay-cli.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// A style profile name could not be resolved to a YAML document.
    #[error("style profile not found: {0}")]
    ProfileNotFound(String),

    /// A YAML document could not be parsed.
    #[error("failed to parse YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// A JSON document could not be parsed or serialized.
    #[error("failed to parse JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// An underlying filesystem operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// LLM output (or another free-form document) could not be parsed into the
    /// expected shape. Carries a human-readable explanation.
    #[error("parse error: {0}")]
    Parse(String),

    /// An LLM call failed (transport, HTTP, or provider selection).
    #[error("llm error: {0}")]
    Llm(String),

    /// Invalid user input or CLI usage.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// A project directory / state file problem.
    #[error("project error: {0}")]
    Project(String),
}

impl CliError {
    /// Convenience constructor for CliError::Parse.
    pub fn parse(msg: impl Into<String>) -> Self {
        CliError::Parse(msg.into())
    }

    /// Convenience constructor for CliError::ProfileNotFound.
    pub fn profile_not_found(name: impl Into<String>) -> Self {
        CliError::ProfileNotFound(name.into())
    }

    /// Convenience constructor for CliError::InvalidInput.
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        CliError::InvalidInput(msg.into())
    }

    /// Convenience constructor for CliError::Project.
    pub fn project(msg: impl Into<String>) -> Self {
        CliError::Project(msg.into())
    }
}

/// Alias used throughout the crate.
pub type CliResult<T> = Result<T, CliError>;
