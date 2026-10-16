//! Shared helpers for the provider-layer integration tests.
//!
//! Every test builds its own ProviderRegistry from a test-local env map
//! (mirroring the Python _env hook) and talks to a local wiremock server, so
//! the suite runs fully offline and never touches process env.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    dead_code
)]

use std::collections::HashMap;

use autoessay_providers::{ProviderRegistry, RetryPolicy};
use serde_json::json;
use std::time::Duration;

/// Minimal providers.yaml used by most tests (mirrors the Python fixture).
pub const MINIMAL_PROVIDERS_YAML: &str = r#"
providers:
  deepseek:
    base_url: https://api.deepseek.com/v1
    env_key: DEEPSEEK_API_KEY
    models:
      fast: deepseek-chat
      smart: deepseek-reasoner
  anthropic:
    base_url: https://api.anthropic.com/v1
    env_key: ANTHROPIC_API_KEY
    models:
      fast: claude-sonnet-4-20250514
      smart: claude-opus-4-20250514
roles:
  drafting: fast
  evaluation: fast
  research: smart
"#;

/// The two providers from the fixture, with base_url swapped for the local mock
/// server and distinct paths per provider so assertions can tell them apart.
pub fn yaml_pointing_at(base: &str) -> String {
    format!(
        r#"
providers:
  deepseek:
    base_url: {base}/deepseek/v1
    env_key: DEEPSEEK_API_KEY
    models:
      fast: deepseek-chat
      smart: deepseek-reasoner
  anthropic:
    base_url: {base}/anthropic/v1
    env_key: ANTHROPIC_API_KEY
    models:
      fast: claude-sonnet-4-20250514
      smart: claude-opus-4-20250514
roles:
  drafting: fast
  evaluation: fast
  research: smart
"#
    )
}

/// A registry from an explicit YAML document plus a test-local env map.
pub fn registry_from(yaml_text: &str, env: &[(&str, &str)]) -> ProviderRegistry {
    let env: HashMap<String, String> = env
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    ProviderRegistry::from_yaml_str(yaml_text, env, "<test>".into())
        .expect("test config should parse")
}

/// A registry for the minimal fixture config.
pub fn registry_with_env(env: &[(&str, &str)]) -> ProviderRegistry {
    registry_from(MINIMAL_PROVIDERS_YAML, env)
}

/// Retry policy with no backoff delay so retry tests stay fast.
pub fn instant_retry(max_retries: u32) -> RetryPolicy {
    RetryPolicy {
        max_retries,
        base_delay: Duration::from_millis(1),
    }
}

/// An OpenAI-compatible success body.
pub fn openai_body(content: &str, model: &str) -> serde_json::Value {
    json!({
        "model": model,
        "choices": [
            {"message": {"content": content, "role": "assistant"}, "finish_reason": "stop"}
        ],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5}
    })
}

/// An Anthropic Messages API success body.
pub fn anthropic_body(content: &str, model: &str) -> serde_json::Value {
    json!({
        "model": model,
        "content": [{"text": content, "type": "text"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    })
}
