//! The LLM client abstraction - the seam that makes every phase testable offline.
//!
//! Every pipeline phase takes "&impl LlmClient" instead of reaching for a global
//! provider. This is the Rust equivalent of the Python tests' patch-then-Mock
//! style, but enforced by the compiler: a phase cannot make an HTTP call unless
//! handed a client, and tests hand it a MockLlm with canned responses.
//!
//! The trait is async because autoessay-providers' Provider::chat is async. The
//! phase functions stay synchronous-friendly by being async themselves and are
//! driven by the tokio runtime in the binary and tests.
//!
//! LlmClient is intentionally shaped like autoessay_providers::Provider's
//! chat_for_role so a Provider can implement it directly (see ProviderLlm
//! below) and a MockLlm can implement it without any network.

use std::fmt;

use autoessay_providers::{LLMResponse, Message};

use crate::error::{CliError, CliResult};

// Re-export the provider message/response types so phases and tests only need
// to reach into one place.
pub use autoessay_providers::Message as ChatMessage;
pub use autoessay_providers::LLMResponse as LlmResponse;

/// Parameters for a single chat call, mirroring Provider.chat_for_role.
///
/// role selects the tier (research/outline/revision_brief -> smart;
/// drafting/evaluation/revision -> fast) via the provider registry. prefer names
/// a preferred provider, exclude names one to avoid (anti-self-review).
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub role: String,
    pub prefer: Option<String>,
    pub exclude: Option<String>,
    pub system: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl ChatRequest {
    /// Start building a request for a pipeline role.
    pub fn for_role(role: impl Into<String>, messages: Vec<Message>) -> Self {
        ChatRequest {
            messages,
            role: role.into(),
            prefer: None,
            exclude: None,
            system: None,
            max_tokens: 4096,
            temperature: 0.7,
        }
    }

    /// Set the preferred provider name.
    pub fn prefer(mut self, prefer: impl Into<String>) -> Self {
        self.prefer = Some(prefer.into());
        self
    }

    /// Set the excluded provider name (anti-self-review).
    pub fn exclude(mut self, exclude: impl Into<String>) -> Self {
        self.exclude = Some(exclude.into());
        self
    }

    /// Set the system prompt.
    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Set the max completion tokens.
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set the sampling temperature.
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// The full prompt text (system + user messages) - useful for assertions.
    pub fn all_text(&self) -> String {
        let mut out = String::new();
        if let Some(sys) = &self.system {
            out.push_str(sys);
        }
        for m in &self.messages {
            out.push_str(&m.content);
        }
        out
    }
}

/// The seam every phase talks to. Implement it with a mock for tests and a real
/// provider for production.
pub trait LlmClient {
    /// Send a chat request and return the model's response.
    fn chat(
        &self,
        request: ChatRequest,
    ) -> impl std::future::Future<Output = CliResult<LLMResponse>> + Send;
}

// ═══════════════════════════════════════════════════════════════════════════
// Mock - offline testing
// ═══════════════════════════════════════════════════════════════════════════

/// A recording mock client that returns canned responses and captures every
/// request. This is the Rust analogue of the Python tests' Mock() provider.
pub struct MockLlm {
    responses: Vec<LLMResponse>,
    recorded: std::sync::Mutex<Vec<ChatRequest>>,
    fail_with: Option<String>,
}

fn canned(content: impl Into<String>) -> LLMResponse {
    LLMResponse {
        content: content.into(),
        model: "mock-model".to_string(),
        provider: "mock".to_string(),
        usage: autoessay_providers::Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
        },
        finish_reason: "stop".to_string(),
    }
}

impl MockLlm {
    /// A mock that always returns content for every call.
    pub fn always(content: impl Into<String>) -> Self {
        MockLlm {
            responses: vec![canned(content)],
            recorded: std::sync::Mutex::new(Vec::new()),
            fail_with: None,
        }
    }

    /// A mock that returns the given contents in order, then repeats the last.
    pub fn sequence(contents: Vec<String>) -> Self {
        MockLlm {
            responses: contents.into_iter().map(canned).collect(),
            recorded: std::sync::Mutex::new(Vec::new()),
            fail_with: None,
        }
    }

    /// A mock that fails every call with the given message.
    pub fn failing(message: impl Into<String>) -> Self {
        MockLlm {
            responses: Vec::new(),
            recorded: std::sync::Mutex::new(Vec::new()),
            fail_with: Some(message.into()),
        }
    }

    /// Every request received, in order.
    pub fn requests(&self) -> Vec<ChatRequest> {
        match self.recorded.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// The most recent request, if any.
    pub fn last_request(&self) -> Option<ChatRequest> {
        self.requests().pop()
    }

    /// Number of requests received.
    pub fn call_count(&self) -> usize {
        self.requests().len()
    }
}

impl LlmClient for MockLlm {
    async fn chat(&self, request: ChatRequest) -> CliResult<LLMResponse> {
        if let Some(msg) = &self.fail_with {
            return Err(CliError::Llm(msg.clone()));
        }
        let index = match self.recorded.lock() {
            Ok(mut guard) => {
                let idx = guard.len();
                guard.push(request);
                idx
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                let idx = guard.len();
                guard.push(request);
                idx
            }
        };
        if self.responses.is_empty() {
            return Err(CliError::Llm(
                "MockLlm has no canned responses configured".to_string(),
            ));
        }
        let pick = if index < self.responses.len() {
            index
        } else {
            self.responses.len() - 1
        };
        Ok(self.responses[pick].clone())
    }
}

impl fmt::Debug for MockLlm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockLlm")
            .field("responses", &self.responses.len())
            .field("fail_with", &self.fail_with)
            .finish()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Real client - wraps autoessay-providers' async Provider
// ═══════════════════════════════════════════════════════════════════════════

/// Adapter that makes autoessay_providers::Provider satisfy LlmClient.
pub struct ProviderLlm {
    provider: autoessay_providers::Provider,
}

impl ProviderLlm {
    /// Wrap a configured Provider.
    pub fn new(provider: autoessay_providers::Provider) -> Self {
        ProviderLlm { provider }
    }

    /// Build from the default registry loaded from environment + config.
    pub fn from_env() -> CliResult<Self> {
        let registry = autoessay_providers::ProviderRegistry::load()
            .map_err(|e| CliError::Llm(format!("failed to load provider config: {e}")))?;
        Ok(ProviderLlm {
            provider: autoessay_providers::Provider::new(registry),
        })
    }
}

impl LlmClient for ProviderLlm {
    async fn chat(&self, request: ChatRequest) -> CliResult<LLMResponse> {
        self.provider
            .chat_for_role(
                &request.messages,
                &request.role,
                request.prefer.as_deref(),
                request.exclude.as_deref(),
                request.system.as_deref(),
                request.max_tokens,
                request.temperature,
            )
            .await
            .map_err(|e| CliError::Llm(e.to_string()))
    }
}
