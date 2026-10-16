//! # autoessay-providers
//!
//! LLM provider abstraction for the autoessay pipeline — the Rust port of
//! autoessay/src/autoessay/provider.py.
//!
//! It provides:
//! - provider auto-detection from environment variables,
//! - two-tier model selection (fast / smart),
//! - Anthropic native API + OpenAI-compatible API support,
//! - anti-self-review enforcement (different provider for evaluation),
//! - exponential backoff retry on transient errors,
//! - standardized response objects.
//!
//! Everything fallible returns Result<T, ProviderError>; there is no
//! unwrap()/expect()/panic! outside of tests.

#![forbid(unsafe_code)]

pub mod client;
pub mod config;
pub mod error;
pub mod types;

pub use client::{
    Provider, RequestRecord, RetryPolicy, MAX_RETRIES, REQUEST_TIMEOUT, RETRYABLE_STATUSES,
    RETRY_BASE_DELAY,
};
pub use config::{ApiType, ProviderConfig, ProviderRegistry, DEFAULT_PROVIDERS_YAML};
pub use error::{ProviderError, ProviderResult};
pub use types::{LLMResponse, Message, Tier, Usage};
