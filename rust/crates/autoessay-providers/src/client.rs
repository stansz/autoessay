//! LLM client — port of the Provider class in
//! autoessay/src/autoessay/provider.py.
//!
//! Routes calls to the right API (Anthropic native vs OpenAI-compatible) with
//! auth, retries and exponential backoff, and returns a normalized
//! LLMResponse.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::{ApiType, ProviderConfig, ProviderRegistry};
use crate::error::{ProviderError, ProviderResult};
use crate::types::{LLMResponse, Message, Tier, Usage};

/// HTTP statuses that trigger a retry. Port of RETRYABLE_STATUSES in Python.
pub const RETRYABLE_STATUSES: [u16; 5] = [429, 500, 502, 503, 504];

/// How many times a failing call is re-attempted after the first try
/// (Python MAX_RETRIES = 3 → at most 4 requests).
pub const MAX_RETRIES: u32 = 3;

/// Base backoff delay in seconds; doubled on every retry (2s, 4s, 8s).
pub const RETRY_BASE_DELAY: Duration = Duration::from_secs(2);

/// Default request timeout, matching httpx.Timeout(120) in Python.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Tuning knobs for the retry loop. Production uses the constants above;
/// tests shrink the delays so they run in milliseconds.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Extra attempts after the first one (Python MAX_RETRIES).
    pub max_retries: u32,
    /// First backoff delay; doubled on each retry.
    pub base_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_retries: MAX_RETRIES,
            base_delay: RETRY_BASE_DELAY,
        }
    }
}

impl RetryPolicy {
    /// Delay before retry number attempt (0-based): base * 2^attempt.
    fn delay_for(&self, attempt: u32) -> Duration {
        self.base_delay.saturating_mul(2u32.saturating_pow(attempt))
    }
}

/// A single recorded request, for tests and debugging.
#[derive(Debug, Clone)]
pub struct RequestRecord {
    /// Which API implementation handled it.
    pub api_type: ApiType,
    /// Full request URL.
    pub url: String,
    /// Request body as JSON.
    pub body: Value,
}

/// Routes LLM calls to the right API with auth, retries, and rate limiting.
///
/// Port of the Python Provider class. Usage:
///
/// let registry = ProviderRegistry::from_env(env)?;
/// let provider = Provider::new(registry);
///
/// // Simple call using any available fast-tier model
/// let resp = provider.chat(&messages, Tier::Fast, None, None, None, 4096, 0.7).await?;
///
/// // Evaluation — exclude the drafting provider
/// let eval_resp = provider
///     .chat(&messages, Tier::Fast, None, provider.last_provider().as_deref(), None, 4096, 0.7)
///     .await?;
///
/// // By pipeline role (tier resolved from providers.yaml roles)
/// let resp = provider.chat_for_role(&messages, "drafting", None, None, None, 4096, 0.7).await?;
#[derive(Debug)]
pub struct Provider {
    registry: ProviderRegistry,
    client: reqwest::Client,
    retry: RetryPolicy,
    last_provider: Mutex<Option<String>>,
    request_count: AtomicU64,
    request_log: Mutex<Vec<RequestRecord>>,
}

impl Provider {
    /// Build a provider on the default 120s-timeout HTTP client.
    pub fn new(registry: ProviderRegistry) -> Self {
        Self::with_client(registry, build_client(REQUEST_TIMEOUT), RetryPolicy::default())
    }

    /// Build a provider with an explicit HTTP client and retry policy.
    pub fn with_client(
        registry: ProviderRegistry,
        client: reqwest::Client,
        retry: RetryPolicy,
    ) -> Self {
        Provider {
            registry,
            client,
            retry,
            last_provider: Mutex::new(None),
            request_count: AtomicU64::new(0),
            request_log: Mutex::new(Vec::new()),
        }
    }

    /// Build a provider whose retry delays are shortened — for tests.
    pub fn with_retry_policy(registry: ProviderRegistry, retry: RetryPolicy) -> Self {
        Self::with_client(registry, build_client(REQUEST_TIMEOUT), retry)
    }

    /// The registry this provider resolves against.
    pub fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    /// Name of the provider used in the most recent successful call.
    ///
    /// Port of the Python last_provider property; used to enforce
    /// anti-self-review by passing it as exclude on the next call.
    pub fn last_provider(&self) -> Option<String> {
        self.last_provider
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or(None)
    }

    /// How many HTTP requests this provider has issued (including retries).
    pub fn request_count(&self) -> u64 {
        self.request_count.load(Ordering::SeqCst)
    }

    /// The bodies of the requests issued so far, in order (for assertions).
    pub fn request_log(&self) -> Vec<RequestRecord> {
        self.request_log
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    // ------------------------------------------------------------------
    // Public API
    // ------------------------------------------------------------------

    /// Send a chat completion request.
    ///
    /// Arguments mirror the Python keyword arguments:
    /// - messages: [{"role": "user", "content": "..."}],
    /// - tier: Fast or Smart — selects which model to use,
    /// - prefer: preferred provider name (auto-detected if None),
    /// - exclude: provider name to exclude (anti-self-review),
    /// - system: system prompt,
    /// - max_tokens: maximum completion tokens,
    /// - temperature: sampling temperature (0-1).
    #[allow(clippy::too_many_arguments)]
    pub async fn chat(
        &self,
        messages: &[Message],
        tier: Tier,
        prefer: Option<&str>,
        exclude: Option<&str>,
        system: Option<&str>,
        max_tokens: u32,
        temperature: f32,
    ) -> ProviderResult<LLMResponse> {
        let (cfg, model) = self.registry.resolve_provider(tier, prefer, exclude)?;

        let api_key = self
            .registry
            .env_var(&cfg.env_key)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| ProviderError::MissingKey {
                provider: cfg.name.clone(),
                env_key: cfg.env_key.clone(),
            })?;

        let request = CallRequest {
            cfg: cfg.clone(),
            model,
            api_key,
            messages: messages.to_vec(),
            system: system.map(str::to_string),
            max_tokens,
            temperature,
        };

        let result = self.retry_call(request).await?;

        self.set_last_provider(cfg.name.clone());
        Ok(result)
    }

    /// Send a chat request using the tier configured for a pipeline role.
    ///
    /// Port of chat_for_role(): the tier is resolved from providers.yaml roles
    /// first ("research", "outline", "drafting", "evaluation", "revision", ...),
    /// then chat() is called with the same arguments.
    #[allow(clippy::too_many_arguments)]
    pub async fn chat_for_role(
        &self,
        messages: &[Message],
        role: &str,
        prefer: Option<&str>,
        exclude: Option<&str>,
        system: Option<&str>,
        max_tokens: u32,
        temperature: f32,
    ) -> ProviderResult<LLMResponse> {
        let tier = self.registry.tier_for_role(role);
        self.chat(
            messages,
            tier,
            prefer,
            exclude,
            system,
            max_tokens,
            temperature,
        )
        .await
    }

    // ------------------------------------------------------------------
    // Internal — API routing
    // ------------------------------------------------------------------

    /// Route to the correct API implementation. Port of Provider._call().
    async fn call(&self, request: CallRequest) -> ProviderResult<LLMResponse> {
        match request.cfg.api_type {
            ApiType::Anthropic => self.call_anthropic(request).await,
            ApiType::OpenAi => self.call_openai_compatible(request).await,
        }
    }

    /// Native Anthropic Messages API call. Port of _call_anthropic().
    ///
    /// POST {base_url}/messages with x-api-key + anthropic-version, and the
    /// system prompt as a top-level "system" field.
    async fn call_anthropic(&self, request: CallRequest) -> ProviderResult<LLMResponse> {
        let mut body = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "messages": request.messages,
        });
        if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
            body["system"] = Value::String(system.to_string());
        }

        let url = format!("{}/messages", request.cfg.base_url);
        let data = self
            .send_json(
                &url,
                &body,
                &[
                    ("x-api-key", request.api_key.as_str()),
                    ("anthropic-version", ANTHROPIC_VERSION),
                    ("Content-Type", "application/json"),
                ],
                request.cfg.api_type,
            )
            .await?;

        let content = data
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                ProviderError::response_parse("missing content[0].text in Anthropic response")
            })?;
        let model = data
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| request.model.clone());
        let prompt_tokens = data
            .pointer("/usage/input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let completion_tokens = data
            .pointer("/usage/output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let finish_reason = data
            .get("stop_reason")
            .and_then(Value::as_str)
            .unwrap_or("stop")
            .to_string();

        Ok(LLMResponse {
            content,
            model,
            provider: request.cfg.name.clone(),
            usage: Usage {
                prompt_tokens: as_u32(prompt_tokens),
                completion_tokens: as_u32(completion_tokens),
            },
            finish_reason,
        })
    }

    /// OpenAI-compatible chat/completions call (DeepSeek, OpenRouter, Z.ai).
    /// Port of _call_openai_compatible().
    ///
    /// POST {base_url}/chat/completions with Bearer auth; the system prompt is
    /// prepended as a leading {"role": "system"} message.
    async fn call_openai_compatible(&self, request: CallRequest) -> ProviderResult<LLMResponse> {
        let mut messages = Vec::with_capacity(request.messages.len() + 1);
        if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
            messages.push(Message::system(system));
        }
        messages.extend(request.messages.iter().cloned());

        let body = json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
        });

        let url = format!("{}/chat/completions", request.cfg.base_url);
        let authorization = format!("Bearer {}", request.api_key);
        let data = self
            .send_json(
                &url,
                &body,
                &[
                    ("Authorization", authorization.as_str()),
                    ("Content-Type", "application/json"),
                ],
                request.cfg.api_type,
            )
            .await?;

        let content = data
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                ProviderError::response_parse(
                    "missing choices[0].message.content in OpenAI-compatible response",
                )
            })?;
        let model = data
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| request.model.clone());
        let prompt_tokens = data
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let completion_tokens = data
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let finish_reason = data
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
            .unwrap_or("stop")
            .to_string();

        Ok(LLMResponse {
            content,
            model,
            provider: request.cfg.name.clone(),
            usage: Usage {
                prompt_tokens: as_u32(prompt_tokens),
                completion_tokens: as_u32(completion_tokens),
            },
            finish_reason,
        })
    }

    // ------------------------------------------------------------------
    // Internal — HTTP + retry
    // ------------------------------------------------------------------

    /// POST a JSON body and return the parsed JSON object.
    async fn send_json(
        &self,
        url: &str,
        body: &Value,
        headers: &[(&str, &str)],
        api_type: ApiType,
    ) -> ProviderResult<Value> {
        self.request_count.fetch_add(1, Ordering::SeqCst);
        self.record_request(RequestRecord {
            api_type,
            url: url.to_string(),
            body: body.clone(),
        });

        let mut request = self.client.post(url).json(body);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }

        let response = request.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;

        if !status.is_success() {
            return Err(ProviderError::HttpStatus {
                status: status.as_u16(),
                body: truncate(&String::from_utf8_lossy(&bytes), 2048),
            });
        }

        serde_json::from_slice::<Value>(&bytes).map_err(|source| {
            ProviderError::response_parse(format!(
                "invalid JSON in response body ({} bytes): {source}",
                bytes.len()
            ))
        })
    }

    /// Exponential backoff on transient HTTP/network errors. Port of _retry().
    ///
    /// A call is attempted at most 1 + max_retries times; the backoff before
    /// retry N (0-based) is base_delay * 2^N (2s, 4s, 8s). The error from the
    /// final attempt is returned rather than swallowed.
    async fn retry_call(&self, request: CallRequest) -> ProviderResult<LLMResponse> {
        let max_retries = self.retry.max_retries;
        let mut last_error: Option<ProviderError> = None;

        for attempt in 0..=max_retries {
            match self.call(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    if error.is_retryable() && attempt < max_retries {
                        tokio::time::sleep(self.retry.delay_for(attempt)).await;
                        last_error = Some(error);
                        continue;
                    }
                    return Err(error);
                }
            }
        }

        // Unreachable: the loop either returns Ok or returns Err on the final
        // attempt. Keep the error value rather than panicking if that changes.
        Err(last_error.unwrap_or_else(|| {
            ProviderError::response_parse("retry loop ended without a result")
        }))
    }

    fn set_last_provider(&self, name: String) {
        if let Ok(mut guard) = self.last_provider.lock() {
            *guard = Some(name);
        }
    }

    fn record_request(&self, record: RequestRecord) {
        if let Ok(mut guard) = request_log_mut(&self.request_log) {
            guard.push(record);
        }
    }
}

/// One fully-resolved API call.
#[derive(Debug, Clone)]
struct CallRequest {
    cfg: ProviderConfig,
    model: String,
    api_key: String,
    messages: Vec<Message>,
    system: Option<String>,
    max_tokens: u32,
    temperature: f32,
}

/// Build the shared reqwest client (120s timeout, like the Python httpx client).
fn build_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_default()
}

/// Saturating u64 -> u32 for token counters.
fn as_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Truncate a string to at most max bytes, on a char boundary.
fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Re-borrow helper so a poisoned mutex degrades to an empty log instead of
/// panicking (no unwrap/expect/panic outside tests).
fn request_log_mut(
    log: &Mutex<Vec<RequestRecord>>,
) -> Result<std::sync::MutexGuard<'_, Vec<RequestRecord>>, ()> {
    log.lock().map_err(|_| ())
}
