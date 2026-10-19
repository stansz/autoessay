
//! ProviderRegistry tests — port of TestProviderRegistry in
//! autoessay/tests/test_provider.py.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use autoessay_providers::{ApiType, ProviderError, ProviderRegistry, Tier, DEFAULT_PROVIDERS_YAML};
use common::{registry_from, registry_with_env, MINIMAL_PROVIDERS_YAML};

#[test]
fn loads_providers_and_infers_api_type() {
    let reg = registry_with_env(&[]);
    assert!(reg.providers.contains_key("deepseek"));
    assert!(reg.providers.contains_key("anthropic"));
    assert_eq!(reg.get("deepseek").unwrap().api_type, ApiType::OpenAi);
    assert_eq!(reg.get("anthropic").unwrap().api_type, ApiType::Anthropic);
}

#[test]
fn roles_loaded_and_unknown_role_defaults_fast() {
    let reg = registry_with_env(&[]);
    assert_eq!(reg.tier_for_role("drafting"), Tier::Fast);
    assert_eq!(reg.tier_for_role("research"), Tier::Smart);
    assert_eq!(reg.tier_for_role("nonexistent"), Tier::Fast);
}

#[test]
fn available_empty_when_no_keys() {
    let reg = registry_with_env(&[]);
    assert!(reg.available().is_empty());
}

#[test]
fn available_detects_set_keys() {
    let reg = registry_with_env(&[("DEEPSEEK_API_KEY", "sk-test")]);
    assert_eq!(reg.available(), vec!["deepseek".to_string()]);
}

#[test]
fn get_unknown_provider_errors() {
    let reg = registry_with_env(&[]);
    match reg.get("openai") {
        Err(ProviderError::UnknownProvider { name, available }) => {
            assert_eq!(name, "openai");
            assert!(available.contains("anthropic"), "available={available}");
            assert!(available.contains("deepseek"), "available={available}");
        }
        other => panic!("expected UnknownProvider, got {other:?}"),
    }
}

#[test]
fn resolve_provider_prefer_wins() {
    let reg = registry_with_env(&[
        ("DEEPSEEK_API_KEY", "sk-ds"),
        ("ANTHROPIC_API_KEY", "sk-ant"),
    ]);
    let (cfg, model) = reg
        .resolve_provider(Tier::Smart, Some("anthropic"), None)
        .unwrap();
    assert_eq!(cfg.name, "anthropic");
    assert_eq!(model, "claude-opus-4-20250514");
}

#[test]
fn resolve_provider_exclude_skips_provider() {
    let reg = registry_with_env(&[
        ("DEEPSEEK_API_KEY", "sk-ds"),
        ("ANTHROPIC_API_KEY", "sk-ant"),
    ]);
    let (cfg, _model) = reg
        .resolve_provider(Tier::Fast, None, Some("deepseek"))
        .unwrap();
    assert_eq!(cfg.name, "anthropic");
}

#[test]
fn resolve_provider_exclude_all_errors() {
    let reg = registry_with_env(&[("DEEPSEEK_API_KEY", "sk-ds")]);
    let err = reg
        .resolve_provider(Tier::Fast, None, Some("deepseek"))
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("excluding 'deepseek'"), "msg={msg}");
    assert!(msg.contains("tier='fast'"), "msg={msg}");
    assert!(msg.contains("at least two providers"), "msg={msg}");
}

#[test]
fn resolve_provider_no_available_errors() {
    let reg = registry_with_env(&[]);
    let err = reg.resolve_provider(Tier::Fast, None, None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("No available providers"), "msg={msg}");
    assert!(msg.contains("Set at least one provider API key."), "msg={msg}");
}

#[test]
fn prefer_unavailable_falls_back() {
    let reg = registry_with_env(&[("DEEPSEEK_API_KEY", "sk-ds")]);
    let (cfg, _model) = reg
        .resolve_provider(Tier::Fast, Some("anthropic"), None)
        .unwrap();
    assert_eq!(cfg.name, "deepseek");
}

#[test]
fn missing_config_file_errors() {
    let err = autoessay_providers::ProviderRegistry::from_env_with_path(
        "/nonexistent/providers.yaml",
        std::collections::HashMap::new(),
    )
    .unwrap_err();
    assert!(matches!(err, ProviderError::ConfigIo { .. }), "got {err:?}");
}

#[test]
fn invalid_yaml_missing_providers_key_errors() {
    let err = ProviderRegistry::from_yaml_str(
        "other_key:\n  - item1\n",
        std::collections::HashMap::new(),
        "<test>".into(),
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Invalid providers.yaml"), "msg={msg}");
    assert!(msg.contains("missing 'providers'"), "msg={msg}");
}

#[test]
fn embedded_default_config_is_openrouter_with_two_distinct_models() {
    // The embedded default is the shipped config/providers.yaml.
    assert_eq!(
        DEFAULT_PROVIDERS_YAML,
        include_str!("../config/providers.yaml")
    );
    let reg = registry_from(DEFAULT_PROVIDERS_YAML, &[("OPENROUTER_API_KEY", "sk-or")]);
    assert_eq!(reg.available(), vec!["openrouter".to_string()]);

    // One provider, two different model families: the writer and the evaluator.
    assert_eq!(reg.tier_for_role("drafting"), Tier::Fast);
    assert_eq!(reg.tier_for_role("evaluation"), Tier::Smart);
    let (draft_cfg, draft_model) = reg.resolve_provider(Tier::Fast, None, None).unwrap();
    let (eval_cfg, eval_model) = reg.resolve_provider(Tier::Smart, None, None).unwrap();
    assert_eq!(draft_cfg.name, "openrouter");
    assert_eq!(eval_cfg.name, "openrouter");
    assert_eq!(draft_model, "deepseek/deepseek-v4.1-flash");
    assert_eq!(eval_model, "anthropic/claude-sonnet-4.6");
    assert_ne!(
        draft_model, eval_model,
        "anti-self-review requires drafting and evaluation to use different models"
    );
}

#[test]
fn minimal_fixture_config_parses() {
    // Guard the test fixture itself.
    let reg = registry_from(MINIMAL_PROVIDERS_YAML, &[]);
    assert_eq!(reg.providers.len(), 2);
}
