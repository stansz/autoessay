//! Error type for the provider layer.
//!
//! Every fallible operation in this crate returns Result<T, ProviderError>.
//! There is deliberately no unwrap()/expect()/panic! outside of tests: the
//! whole point of the Rust rewrite is to make failure paths explicit values.

use crate::types::Tier;

/// Result alias used throughout this crate.
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Errors produced by the provider layer.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// No provider is available for the requested tier.
    ///
    /// Port of the two RuntimeError messages raised by
    /// ProviderRegistry.resolve_provider; the wording of the Display impl is
    /// character-for-character the Python one so downstream error text tests
    /// keep working.
    #[error("{}", no_provider_message(.tier, .excluded))]
    NoProvider {
        /// The tier that could not be resolved ("fast" / "smart").
        tier: Tier,
        /// The provider excluded from selection (anti-self-review), if any.
        excluded: Option<String>,
    },

    /// The resolved provider has no API key in the environment.
    #[error("Provider '{provider}' requires {env_key} in environment")]
    MissingKey {
        /// Provider name from providers.yaml.
        provider: String,
        /// Name of the environment variable that must hold the API key.
        env_key: String,
    },

    /// The named provider is not present in providers.yaml.
    #[error("Unknown provider: '{name}'. Available: {available}")]
    UnknownProvider {
        /// The requested provider name.
        name: String,
        /// Debug-formatted list of configured provider names.
        available: String,
    },

    /// providers.yaml could not be found on disk.
    #[error("providers.yaml not found. Set AUTOESSAY_CONFIG or run from repo root.")]
    ConfigNotFound,

    /// providers.yaml could not be read or parsed.
    #[error("failed to read providers config {path}: {source}")]
    ConfigIo {
        /// Path of the config file that failed.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// providers.yaml parsed but is missing required keys or is malformed.
    #[error("Invalid providers.yaml: {0}")]
    ConfigInvalid(String),

    /// providers.yaml could not be parsed as YAML.
    #[error("failed to parse providers config {path}: {source}")]
    ConfigYaml {
        /// Path of the config file that failed.
        path: String,
        /// Underlying YAML error.
        #[source]
        source: serde_yaml::Error,
    },

    /// A transport-level failure (connection refused, DNS, timeout, ...).
    #[error("http transport error: {0}")]
    Http(#[from] reqwest::Error),

    /// The endpoint answered with a non-success status code.
    ///
    /// On the final retry attempt the error is surfaced rather than consumed,
    /// so callers always see the last status/body pair.
    #[error("HTTP {status}: {body}")]
    HttpStatus {
        /// HTTP status code.
        status: u16,
        /// Response body (truncated for display).
        body: String,
    },

    /// The response body was not the JSON shape the API promised.
    #[error("failed to parse provider response: {0}")]
    ResponseParse(String),

    /// A tier string that is neither "fast" nor "smart".
    #[error("unknown tier '{0}' (expected \"fast\" or \"smart\")")]
    UnknownTier(String),
}

impl ProviderError {
    /// True when the error is a transient HTTP failure worth retrying:
    /// one of RETRYABLE_STATUSES, or a transport-level error.
    ///
    /// Port of the Python _retry() classification of
    /// httpx.HTTPStatusError / (httpx.ConnectError, httpx.TimeoutException).
    pub fn is_retryable(&self) -> bool {
        match self {
            ProviderError::HttpStatus { status, .. } => {
                crate::client::RETRYABLE_STATUSES.contains(status)
            }
            ProviderError::Http(err) => err.is_connect() || err.is_timeout() || err.is_request(),
            _ => false,
        }
    }

    /// Convenience constructor for a response-shape failure.
    pub(crate) fn response_parse(msg: impl Into<String>) -> Self {
        ProviderError::ResponseParse(msg.into())
    }
}

/// Reproduces the two RuntimeError message shapes of
/// ProviderRegistry.resolve_provider() exactly.
fn no_provider_message(tier: &Tier, excluded: &Option<String>) -> String {
    match excluded {
        Some(name) => format!(
            "No providers for tier='{tier}' after excluding '{name}'. \
             Set API keys for at least two providers to enable evaluation."
        ),
        None => format!(
            "No available providers for tier='{tier}'. \
             Set at least one provider API key."
        ),
    }
}
