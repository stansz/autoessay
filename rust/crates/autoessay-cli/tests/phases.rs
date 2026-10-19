// Tests are the one place where unwrap/expect/panic! are allowed by the
// project design rule (no unwrap/expect/panic! outside tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Phase-level tests mirroring the Python test_pipeline.py / test_drafting.py /
//! test_revision.py mock-patching style, but with clean dependency injection:
//! every phase takes &impl LlmClient, and here we pass a recording MockLlm.

use autoessay_cli::llm::MockLlm;
use autoessay_cli::phases::{
    build_system_prompt, build_user_prompt, draft_section, generate_outline, generate_research,
    generate_revision_brief, revise_section, revise_until_pass, ReviseConfig, SectionBrief,
};
use autoessay_cli::styles::load_profile;
use autoessay_core::{EvaluationResult, StyleProfile};

fn magazine() -> StyleProfile {
    load_profile("magazine", None).expect("magazine profile")
}

const RESEARCH: &str = "## Remote Work\n\n### Key Claims\n- Claim: Remote work increased productivity by 13% | Source: src_01\n\n### Sources\n- [src_01] Stanford WFH Study, 2023";

// ═══════════════════════════════════════════════════════════════════════════
// Research
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn research_returns_markdown() {
    let mock = MockLlm::always("## Research\n\n### Key Claims\n- Claim: Test");
    let out = generate_research(&mock, "climate policy", None, None)
        .await
        .expect("research");
    assert!(out.contains("## Research"));
    assert!(out.contains("Key Claims"));
}

#[tokio::test]
async fn research_uses_research_role_and_low_temp() {
    let mock = MockLlm::always("ok");
    generate_research(&mock, "test topic", None, None)
        .await
        .expect("research");
    let req = mock.last_request().expect("a request");
    assert_eq!(req.role, "research");
    assert!(
        req.prefer.is_none(),
        "provider selection now comes from providers.yaml roles"
    );
    assert!((req.temperature - 0.4).abs() < f32::EPSILON);
}

#[tokio::test]
async fn research_includes_topic_and_seed() {
    let mock = MockLlm::always("ok");
    generate_research(
        &mock,
        "AI safety",
        Some("Focus on alignment research from 2024-2025"),
        None,
    )
    .await
    .expect("research");
    let text = mock.last_request().expect("request").all_text();
    assert!(text.contains("AI safety"));
    assert!(text.contains("alignment research"));
    assert!(text.contains("2024-2025"));
}

#[tokio::test]
async fn research_injects_style_profile_into_system() {
    let mock = MockLlm::always("ok");
    generate_research(&mock, "urban design", None, Some(&magazine()))
        .await
        .expect("research");
    let system = mock
        .last_request()
        .expect("request")
        .system
        .unwrap_or_default();
    assert!(system.contains("Show, don't tell") || system.to_lowercase().contains("magazine"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Outline
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn outline_returns_markdown() {
    let mock = MockLlm::always("## Thesis\n\nTest thesis.");
    let out = generate_outline(&mock, "## Research\n\nClaims", None)
        .await
        .expect("outline");
    assert!(out.contains("## Thesis"));
}

#[tokio::test]
async fn outline_uses_outline_role_and_temp() {
    let mock = MockLlm::always("ok");
    generate_outline(&mock, "research notes", None)
        .await
        .expect("outline");
    let req = mock.last_request().expect("request");
    assert_eq!(req.role, "outline");
    assert!(req.prefer.is_none());
    assert!((req.temperature - 0.5).abs() < f32::EPSILON);
}

#[tokio::test]
async fn outline_receives_research_notes() {
    let mock = MockLlm::always("ok");
    generate_outline(&mock, "Water is wet", None)
        .await
        .expect("outline");
    assert!(mock
        .last_request()
        .expect("request")
        .all_text()
        .contains("Water is wet"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Drafting
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn drafting_uses_drafting_role_creative_temp_and_max_tokens() {
    let mock = MockLlm::always("ok");
    draft_section(&mock, "Title", "Purpose", RESEARCH, &magazine(), None, None, None)
        .await
        .expect("draft");
    let req = mock.last_request().expect("request");
    assert_eq!(req.role, "drafting");
    assert!(req.prefer.is_none());
    assert!((req.temperature - 0.7).abs() < f32::EPSILON);
    assert_eq!(req.max_tokens, 4096);
}

#[tokio::test]
async fn drafting_system_includes_style_and_anti_slop() {
    let mock = MockLlm::always("ok");
    draft_section(&mock, "T", "P", RESEARCH, &magazine(), None, None, None)
        .await
        .expect("draft");
    let system = mock
        .last_request()
        .expect("request")
        .system
        .unwrap_or_default();
    assert!(system.contains("Show, don't tell"));
    assert!(system.contains("Writing Rules"));
    assert!(system.to_lowercase().contains("delve into"));
}

#[tokio::test]
async fn drafting_user_prompt_has_title_purpose_research_sources_previous() {
    let mock = MockLlm::always("ok");
    let prev = vec!["Section 1 was about the rise.".to_string()];
    let sources = vec!["src_01".to_string(), "src_02".to_string()];
    draft_section(
        &mock,
        "The Hybrid Compromise",
        "Argue hybrid is worst of both worlds.",
        RESEARCH,
        &magazine(),
        Some(&sources),
        Some(&prev),
        None,
    )
    .await
    .expect("draft");
    let text = mock.last_request().expect("request").all_text();
    assert!(text.contains("## The Hybrid Compromise"));
    assert!(text.contains("worst of both worlds"));
    assert!(text.contains("Stanford WFH Study"));
    assert!(text.contains("src_01"));
    assert!(text.contains("src_02"));
    assert!(text.contains("Section 1 was about"));
    assert!(text.contains("Write only the section content"));
}

#[test]
fn drafting_omits_sources_and_previous_when_empty() {
    let prompt = build_user_prompt("T", "P", RESEARCH, None, None, None);
    assert!(!prompt.contains("Use these sources"));
    assert!(!prompt.contains("Previous Sections"));
    assert!(prompt.contains("Write only the section content"));
}

#[test]
fn build_system_prompt_includes_style_and_anti_slop() {
    let prompt = build_system_prompt(&magazine());
    assert!(prompt.contains("Show, don't tell"));
    assert!(prompt.contains("Writing Rules"));
}

#[test]
fn build_system_prompt_injects_program_md_phase_2() {
    // Regression: program.md headings are backticked, so a naive lookup returned
    // "" and every phase silently used its fallback. The real Phase 2 text must
    // now reach the drafting prompt.
    let prompt = build_system_prompt(&magazine());
    assert!(
        prompt.contains("You are a writer"),
        "Phase 2 program.md instructions missing from drafting system prompt"
    );
}

#[tokio::test]
async fn drafting_prompt_includes_length_and_audience() {
    let mock = MockLlm::always("ok");
    let brief = SectionBrief {
        target_words: 600,
        audience_level: 4,
    };
    draft_section(
        &mock,
        "Title",
        "Purpose",
        RESEARCH,
        &magazine(),
        None,
        None,
        Some(brief),
    )
    .await
    .expect("draft");
    let text = mock.last_request().expect("request").all_text();
    assert!(text.contains("about 600 words"));
    assert!(text.contains("Audience level: 4/5"));
}

#[tokio::test]
async fn revise_prompt_includes_length_and_audience() {
    let mock = MockLlm::always("ok");
    let brief = SectionBrief {
        target_words: 500,
        audience_level: 2,
    };
    revise_section(
        &mock,
        "Original section",
        "1. Tighten the lede.",
        RESEARCH,
        &magazine(),
        Some(brief),
    )
    .await
    .expect("revise");
    let text = mock.last_request().expect("request").all_text();
    assert!(text.contains("about 500 words"));
    assert!(text.contains("Audience level: 2/5"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Revision — brief + revise
// ═══════════════════════════════════════════════════════════════════════════

fn failed_eval() -> EvaluationResult {
    EvaluationResult {
        accuracy: 5.0,
        coherence: 6.0,
        style: 7.0,
        source_integrity: 8.0,
        overall: "fail".to_string(),
        hallucination_flags: vec![],
        notes: "Paragraph 1 is strong but needs more evidence. Lede is weak.".to_string(),
    }
}

#[tokio::test]
async fn revision_brief_includes_scores_and_notes() {
    let mock = MockLlm::always("1. Fix X");
    generate_revision_brief(&mock, "Section text", &failed_eval(), &magazine())
        .await
        .expect("brief");
    let req = mock.last_request().expect("request");
    assert_eq!(req.role, "revision_brief");
    assert!(req.prefer.is_none());
    let text = req.all_text();
    assert!(text.contains("5.0"), "accuracy score in prompt");
    assert!(text.contains("6.0"), "coherence score in prompt");
    assert!(text.contains("Lede is weak"));
}

#[tokio::test]
async fn revise_section_includes_original_brief_and_style() {
    let mock = MockLlm::always("Revised section text here.");
    let out = revise_section(
        &mock,
        "The original is fragile here.",
        "1. [Paragraph 1] Strengthen the lede.",
        RESEARCH,
        &magazine(),
        None,
    )
    .await
    .expect("revise");
    assert!(out.contains("Revised section"));
    let req = mock.last_request().expect("request");
    assert_eq!(req.role, "revision");
    assert!(req.prefer.is_none());
    let text = req.all_text();
    assert!(text.contains("fragile"));
    assert!(text.contains("Paragraph 1"));
    assert!(text.contains("Show, don't tell"));
}

// ═══════════════════════════════════════════════════════════════════════════
// revise_until_pass — the core loop
// ═══════════════════════════════════════════════════════════════════════════

fn pass_json() -> String {
    r#"{"accuracy": 9.0, "coherence": 9.0, "style": 9.0, "source_integrity": 9.0, "overall": "pass", "hallucination_flags": [], "notes": "Good."}"#.to_string()
}
fn fail_json() -> String {
    r#"{"accuracy": 5.0, "coherence": 6.0, "style": 7.0, "source_integrity": 8.0, "overall": "fail", "hallucination_flags": [], "notes": "Weak lede."}"#.to_string()
}

#[tokio::test]
async fn revise_until_pass_stops_immediately_when_already_passing() {
    let mock = MockLlm::always(pass_json());
    let cfg = ReviseConfig {
        max_cycles: 3,
        exclude_provider: None,
        ..ReviseConfig::default()
    };
    let outcome = revise_until_pass(&mock, "Section", RESEARCH, &magazine(), &cfg)
        .await
        .expect("loop");
    assert_eq!(outcome.history.len(), 1);
    assert!(outcome.passed);
    assert_eq!(outcome.final_section, "Section"); // unchanged
}

#[tokio::test]
async fn revise_until_pass_loops_until_pass() {
    // Sequence: fail (eval) -> brief -> revise -> pass (eval)
    let mock = MockLlm::sequence(vec![
        fail_json(),
        "1. Fix the lede.".to_string(),
        "Revised section text.".to_string(),
        pass_json(),
    ]);
    let cfg = ReviseConfig {
        max_cycles: 3,
        exclude_provider: None,
        plateau_patience: 0, // disable plateau for this test
        ..ReviseConfig::default()
    };
    let outcome = revise_until_pass(&mock, "Section", RESEARCH, &magazine(), &cfg)
        .await
        .expect("loop");
    assert_eq!(outcome.history.len(), 2);
    assert!(!outcome.history[0].passed());
    assert!(outcome.history[1].passed());
    assert_eq!(outcome.final_section, "Revised section text.");
    assert!(outcome.passed);
}

#[tokio::test]
async fn revise_until_pass_gives_up_after_max_cycles() {
    // Every eval fails; every revise returns the same text.
    let mock = MockLlm::sequence(vec![fail_json(), "Revised v2.".to_string()]);
    let cfg = ReviseConfig {
        max_cycles: 2,
        exclude_provider: None,
        plateau_patience: 0,
        ..ReviseConfig::default()
    };
    let outcome = revise_until_pass(&mock, "Section", RESEARCH, &magazine(), &cfg)
        .await
        .expect("loop");
    assert_eq!(outcome.history.len(), 2);
    assert!(outcome.history.iter().all(|h| !h.passed()));
    assert_eq!(outcome.final_section, "Revised v2.");
    assert!(!outcome.passed);
    assert_eq!(outcome.cycles_run, 2);
}

#[tokio::test]
async fn revise_until_pass_forwards_exclude_provider() {
    let mock = MockLlm::always(pass_json());
    let cfg = ReviseConfig {
        max_cycles: 3,
        exclude_provider: Some("deepseek".to_string()),
        ..ReviseConfig::default()
    };
    revise_until_pass(&mock, "Section", RESEARCH, &magazine(), &cfg)
        .await
        .expect("loop");
    let req = mock.last_request().expect("request");
    assert_eq!(req.exclude.as_deref(), Some("deepseek"));
}

#[tokio::test]
async fn revise_until_pass_detects_plateau_and_stops_early() {
    let mock = MockLlm::sequence(vec![
        fail_json(),
        "brief 1".to_string(),
        "revise 1".to_string(),
        fail_json(),
        "brief 2".to_string(),
        "revise 2".to_string(),
        fail_json(),
    ]);
    let cfg = ReviseConfig {
        max_cycles: 5,
        exclude_provider: None,
        brief: None,
        plateau_epsilon: 0.05,
        plateau_patience: 2,
    };
    let outcome = revise_until_pass(&mock, "Section", RESEARCH, &magazine(), &cfg)
        .await
        .expect("loop");
    assert!(outcome.plateau_detected, "expected plateau");
    assert!(!outcome.passed);
    // patience=2 means the plateau is declared once two evaluations fail to
    // beat the running best: eval #1 sets the best, #2 and #3 fail to beat it,
    // so the loop stops after 3 evaluations (well before max_cycles=5).
    assert_eq!(outcome.history.len(), 3);
    assert!(
        outcome.history.len() < cfg.max_cycles as usize,
        "stopped early"
    );
}
