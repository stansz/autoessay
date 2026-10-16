//! Shared data types for the provider layer.
//!
//! Ported from the @dataclass definitions at the top of
//! autoessay/src/autoessay/provider.py.

use serde::{Deserialize, Serialize};

/// Two-tier model selection: "fast" or "smart".
///
/// Mirrors the tier strings used throughout the Python implementation and in
/// providers.yaml; the serde representation is exactly "fast" / "smart".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Cheap, low-latency model (drafting, evaluation, revision, ...).
    Fast,
    /// High-quality model (research, outline, reader panel, ...).
    Smart,
}

impl Tier {
    /// The config-file spelling of this tier ("fast" / "smart").
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Fast => "fast",
            Tier::Smart => "smart",
        }
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Tier {
    type Err = crate::error::ProviderError;

    /// Parse "fast" / "smart"; anything else is a ProviderError::UnknownTier.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fast" => Ok(Tier::Fast),
            "smart" => Ok(Tier::Smart),
            other => Err(crate::error::ProviderError::UnknownTier(other.to_string())),
        }
    }
}

/// A single chat message, {"role": "user", "content": "..."}.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// "system", "user" or "assistant".
    pub role: String,
    /// Message text.
    pub content: String,
}

impl Message {
    /// Convenience constructor for a (role, content) pair.
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Message {
            role: role.into(),
            content: content.into(),
        }
    }

    /// A user message.
    pub fn user(content: impl Into<String>) -> Self {
        Message::new("user", content)
    }

    /// An assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Message::new("assistant", content)
    }

    /// A system message (used when a system prompt is folded into the message
    /// list for OpenAI-compatible endpoints).
    pub fn system(content: impl Into<String>) -> Self {
        Message::new("system", content)
    }
}

/// Token accounting for a single completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt/input tokens (input_tokens on the Anthropic API).
    pub prompt_tokens: u32,
    /// Completion/output tokens (output_tokens on the Anthropic API).
    pub completion_tokens: u32,
}

/// Standardized response from any LLM provider.
///
/// Port of the Python LLMResponse dataclass. usage is a typed struct here
/// instead of a free-form dict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LLMResponse {
    /// The model's text output.
    pub content: String,
    /// Model id reported by the provider.
    pub model: String,
    /// Name of the provider that produced the response (from providers.yaml).
    pub provider: String,
    /// Token usage.
    pub usage: Usage,
    /// Why generation stopped; "stop" when the provider did not say.
    #[serde(default = "default_finish_reason")]
    pub finish_reason: String,
}

fn default_finish_reason() -> String {
    "stop".to_string()
}
