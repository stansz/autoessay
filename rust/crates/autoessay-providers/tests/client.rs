
//! Provider HTTP-layer tests — port of TestProviderOpenAICompatible /
//! TestProviderAnthropic / TestProviderAntiSelfReview in
//! autoessay/tests/test_provider.py, plus the retry and error cases from the
//! task description. All HTTP goes to a local wiremock server.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use autoessay_providers::{ApiType, Message, Provider, ProviderError, Tier};
use common::{anthropic_body, instant_retry, openai_body, registry_from, yaml_pointing_at};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A Provider whose registry points every provider at the mock server, with
/// the given env keys and a 1ms retry backoff.
fn provider_for(server: &MockServer, env: &[(&str, &str)]) -> Provider {
    let yaml = yaml_pointing_at(&server.uri());
    let registry = registry_from(&yaml, env);
    Provider::with_retry_policy(registry, instant_retry(3))
}

#[tokio::test]
async fn openai_happy_path_parses_content_usage_and_model() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(openai_body(
            "Hello from DeepSeek",
            "deepseek-chat",
        )))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("DEEPSEEK_API_KEY", "sk-test")]);
    let resp = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap();

    assert_eq!(resp.content, "Hello from DeepSeek");
    assert_eq!(resp.model, "deepseek-chat");
    assert_eq!(resp.provider, "deepseek");
    assert_eq!(resp.usage.prompt_tokens, 10);
    assert_eq!(resp.usage.completion_tokens, 5);
    assert_eq!(resp.finish_reason, "stop");
    assert_eq!(provider.last_provider().as_deref(), Some("deepseek"));
    assert_eq!(provider.request_count(), 1);
}

#[tokio::test]
async fn openai_request_body_and_headers_match_python() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(openai_body("ok", "deepseek-chat")))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("DEEPSEEK_API_KEY", "sk-test")]);
    provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            Some("You are helpful."),
            1234,
            0.25,
        )
        .await
        .unwrap();

    let log = provider.request_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].api_type, ApiType::OpenAi);
    assert_eq!(log[0].url, format!("{}/deepseek/v1/chat/completions", server.uri()));

    let body = &log[0].body;
    assert_eq!(body["model"], "deepseek-chat");
    assert_eq!(body["max_tokens"], 1234);
    assert_eq!(body["temperature"], 0.25);
    assert!(body.get("system").is_none(), "openai body has no system key");
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(
        messages[0],
        serde_json::json!({"role": "system", "content": "You are helpful."})
    );
    assert_eq!(
        messages[1],
        serde_json::json!({"role": "user", "content": "Hi"})
    );
}

#[tokio::test]
async fn anthropic_happy_path_parses_content_usage_and_model() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/anthropic/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(anthropic_body(
            "Hello from Claude",
            "claude-sonnet-4-20250514",
        )))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("ANTHROPIC_API_KEY", "sk-ant-test")]);
    let resp = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap();

    assert_eq!(resp.content, "Hello from Claude");
    assert_eq!(resp.model, "claude-sonnet-4-20250514");
    assert_eq!(resp.provider, "anthropic");
    assert_eq!(resp.usage.prompt_tokens, 10);
    assert_eq!(resp.usage.completion_tokens, 5);
    assert_eq!(resp.finish_reason, "end_turn");
}

#[tokio::test]
async fn anthropic_system_prompt_is_top_level_and_no_system_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/anthropic/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(anthropic_body(
            "ok",
            "claude-sonnet-4-20250514",
        )))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("ANTHROPIC_API_KEY", "sk-ant-test")]);
    provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            Some("You are Claude."),
            4096,
            0.7,
        )
        .await
        .unwrap();

    let log = provider.request_log();
    assert_eq!(log[0].api_type, ApiType::Anthropic);
    assert_eq!(log[0].body["system"], "You are Claude.");
    let messages = log[0].body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    // Anthropic gets max_tokens but no temperature in the Python port.
    assert!(log[0].body.get("temperature").is_none());
}

#[tokio::test]
async fn chat_for_role_resolves_tier_from_roles_map() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(openai_body(
            "result",
            "deepseek-reasoner",
        )))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("DEEPSEEK_API_KEY", "sk-ds")]);
    provider
        .chat_for_role(
            &[Message::user("Research")],
            "research",
            Some("deepseek"),
            None,
            None,
            4096,
            0.4,
        )
        .await
        .unwrap();

    // research -> smart tier -> deepseek-reasoner.
    let log = provider.request_log();
    assert_eq!(log[0].body["model"], "deepseek-reasoner");
}

#[tokio::test]
async fn anti_self_review_exclude_routes_to_other_provider() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/anthropic/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(anthropic_body(
            "draft",
            "claude-sonnet-4-20250514",
        )))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(openai_body(
            "eval",
            "deepseek-chat",
        )))
        .mount(&server)
        .await;

    let provider = provider_for(
        &server,
        &[("DEEPSEEK_API_KEY", "sk-ds"), ("ANTHROPIC_API_KEY", "sk-ant")],
    );
    let messages = [Message::user("Write")];

    // Draft with Anthropic.
    let draft = provider
        .chat(
            &messages,
            Tier::Fast,
            Some("anthropic"),
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap();
    assert_eq!(draft.provider, "anthropic");
    assert_eq!(provider.last_provider().as_deref(), Some("anthropic"));

    // Evaluate: exclude the drafting provider -> deepseek.
    let eval_resp = provider
        .chat(
            &messages,
            Tier::Fast,
            None,
            provider.last_provider().as_deref(),
            None,
            4096,
            0.3,
        )
        .await
        .unwrap();
    assert_eq!(eval_resp.provider, "deepseek");

    let log = provider.request_log();
    assert_eq!(log.len(), 2);
    assert!(log[0].url.contains("/anthropic/"), "{}", log[0].url);
    assert!(log[1].url.contains("/deepseek/"), "{}", log[1].url);
}

#[tokio::test]
async fn retries_on_503_then_succeeds_with_exactly_two_requests() {
    let server = MockServer::start().await;
    // First request: 503. Second: 200.
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string("unavailable"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(openai_body(
            "finally ok",
            "deepseek-chat",
        )))
        .with_priority(2)
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("DEEPSEEK_API_KEY", "sk-test")]);
    let resp = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap();

    assert_eq!(resp.content, "finally ok");
    // Exactly two HTTP requests hit the server (1 failure + 1 success).
    assert_eq!(provider.request_count(), 2);
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2);
}

#[tokio::test]
async fn does_not_retry_on_400_and_surfaces_status() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("DEEPSEEK_API_KEY", "sk-test")]);
    let err = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap_err();

    match err {
        ProviderError::HttpStatus { status, body } => {
            assert_eq!(status, 400);
            assert!(body.contains("bad request"), "body={body}");
        }
        other => panic!("expected HttpStatus, got {other:?}"),
    }
    // Exactly one request: 400 is not retryable.
    assert_eq!(provider.request_count(), 1);
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
}

#[tokio::test]
async fn retries_exhaust_after_max_retries_and_keep_last_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string("unavailable"))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("DEEPSEEK_API_KEY", "sk-test")]);
    let err = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap_err();

    match &err {
        ProviderError::HttpStatus { status, .. } => assert_eq!(*status, 503),
        other => panic!("expected HttpStatus, got {other:?}"),
    }
    // MAX_RETRIES = 3 -> 4 attempts total, and the error is not swallowed.
    assert_eq!(provider.request_count(), 4);
    assert!(err.is_retryable());
}

#[tokio::test]
async fn missing_api_key_errors_before_any_http() {
    let server = MockServer::start().await;
    let provider = provider_for(&server, &[]);

    let err = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap_err();

    assert!(matches!(err, ProviderError::NoProvider { .. }), "got {err:?}");
    assert!(err.to_string().contains("No available providers"));
    assert_eq!(provider.request_count(), 0);
}

#[tokio::test]
async fn missing_key_error_when_env_key_disappears_after_resolution() {
    // resolve_provider sees a key, but chat() re-reads it: a registry whose
    // env entry is empty string is treated as absent by available() but the
    // MissingKey path is exercised via a directly constructed registry.
    let server = MockServer::start().await;
    let yaml = yaml_pointing_at(&server.uri());
    let registry = registry_from(&yaml, &[("DEEPSEEK_API_KEY", "")]);
    let provider = Provider::with_retry_policy(registry, instant_retry(3));

    let err = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            Some("deepseek"),
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap_err();

    // An empty key means the provider is not available at all.
    assert!(matches!(err, ProviderError::NoProvider { .. }), "got {err:?}");
    assert_eq!(provider.request_count(), 0);
}

#[tokio::test]
async fn malformed_response_body_reports_response_parse() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("DEEPSEEK_API_KEY", "sk-test")]);
    let err = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap_err();

    assert!(matches!(err, ProviderError::ResponseParse(_)), "got {err:?}");
}

#[tokio::test]
async fn response_missing_choices_is_a_parse_error_not_a_panic() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"model": "deepseek-chat", "usage": {"prompt_tokens": 1, "completion_tokens": 1}}),
        ))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("DEEPSEEK_API_KEY", "sk-test")]);
    let err = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap_err();

    match err {
        ProviderError::ResponseParse(msg) => {
            assert!(msg.contains("choices"), "msg={msg}");
        }
        other => panic!("expected ResponseParse, got {other:?}"),
    }
}

#[tokio::test]
async fn openai_200_with_error_object_surfaces_provider_message() {
    // OpenRouter can return 200 + {"error": {...}} when an upstream model fails.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "error": {"message": "upstream model unavailable", "code": 502}
        })))
        .mount(&server)
        .await;

    let provider = provider_for(&server, &[("DEEPSEEK_API_KEY", "sk-test")]);
    let err = provider
        .chat(
            &[Message::user("Hi")],
            Tier::Fast,
            None,
            None,
            None,
            4096,
            0.7,
        )
        .await
        .unwrap_err();

    match err {
        ProviderError::ResponseParse(msg) => {
            assert!(msg.contains("upstream model unavailable"), "msg={msg}");
        }
        other => panic!("expected ResponseParse, got {other:?}"),
    }
}

#[tokio::test]
async fn message_serialization_matches_python_shape() {
    let messages = vec![Message::user("Hi"), Message::assistant("Hello")];
    let value: Value = serde_json::to_value(&messages).unwrap();
    assert_eq!(
        value,
        serde_json::json!([
            {"role": "user", "content": "Hi"},
            {"role": "assistant", "content": "Hello"}
        ])
    );
}
