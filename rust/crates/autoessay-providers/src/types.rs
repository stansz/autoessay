//! Shared data types for the provider layer.
//!
//! Ported from the @dataclass definitions at the top of
//! autoessay/src/autoessay/provider.py.

use serde::{Deserialize, Serialize};

/// Three-tier model selection: "fast", "smart", or "judge".
///
/// Mirrors the tier strings used in providers.yaml. `judge` is optional: it lets
/// an evaluation ensemble use a distinct model without changing the writer or
/// editor tiers (see `roles.judge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Cheap, low-latency model (drafting, revision, ...).
    Fast,
    /// High-quality model (research, outline, evaluation, ...).
    Smart,
    /// Optional second-opinion evaluator for ensemble judging.
    Judge,
}

impl Tier {
    /// The config-file spelling of this tier ("fast" / "smart" / "judge").
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Fast => "fast",
            Tier::Smart => "smart",
            Tier::Judge => "judge",
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

    /// Parse "fast" / "smart" / "judge"; anything else is an UnknownTier error.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fast" => Ok(Tier::Fast),
            "smart" => Ok(Tier::Smart),
            "judge" => Ok(Tier::Judge),
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
    /// Prompt tokens served from the provider's prompt cache, when reported.
    ///
    /// These are billed much more cheaply than normal input tokens (OpenRouter:
    /// DeepSeek ~0.1x, Xiaomi cache reads ~0.008x), so surfacing them is how we
    /// verify that the cache-friendly prompt layout is actually paying off.
    #[serde(default)]
    pub cached_tokens: u32,
}

/// Structured-output request format (OpenAI-compatible `response_format`).
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseFormat {
    /// Provider-enforced "must be valid JSON" mode.
    JsonObject,
    /// JSON Schema mode, where the endpoint supports it.
    JsonSchema {
        /// Schema name reported to the provider.
        name: String,
        /// Ask for strict schema conformity where supported.
        strict: bool,
        /// The JSON Schema document.
        schema: serde_json::Value,
    },
}

impl ResponseFormat {
    /// The wire representation of this format.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ResponseFormat::JsonObject => serde_json::json!({"type": "json_object"}),
            ResponseFormat::JsonSchema {
                name,
                strict,
                schema,
            } => serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": name,
                    "strict": strict,
                    "schema": schema,
                }
            }),
        }
    }
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
