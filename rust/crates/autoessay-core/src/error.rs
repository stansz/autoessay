//! Error type for the core domain crate.
//!
//! Every fallible public operation in this crate returns `Result<T, CoreError>`.
//! There is deliberately no `unwrap`/`expect`/`panic!` outside of tests: the
//! whole point of the Rust rewrite is to make failure paths explicit values.

/// Errors produced by `autoessay-core`.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A style profile name could not be resolved to a YAML document.
    #[error("style profile not found: {0}")]
    ProfileNotFound(String),

    /// A style profile YAML document could not be parsed.
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
}

impl CoreError {
    /// Convenience constructor for `CoreError::Parse`.
    pub fn parse(msg: impl Into<String>) -> Self {
        CoreError::Parse(msg.into())
    }

    /// Convenience constructor for `CoreError::ProfileNotFound`.
    pub fn profile_not_found(name: impl Into<String>) -> Self {
        CoreError::ProfileNotFound(name.into())
    }
}

/// Alias used throughout the crate.
pub type CoreResult<T> = Result<T, CoreError>;
