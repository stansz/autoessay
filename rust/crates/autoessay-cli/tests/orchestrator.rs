// Tests are the one place where unwrap/expect/panic! are allowed by the
// project design rule (no unwrap/expect/panic! outside tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! End-to-end orchestrator test against a scripted MockLlm.
//!
//! Asserts the project directory gets the expected files, state.json is
//! well-formed, and sources.json is populated — all without HTTP.

use std::path::PathBuf;

use autoessay_cli::llm::MockLlm;
use autoessay_cli::orchestrator::{run_pipeline, PipelineConfig};
use autoessay_cli::phases::ReviseConfig;
use autoessay_cli::styles::load_profile;

fn temp_dir() -> PathBuf {
    let base = std::env::temp_dir().join("autoessay-cli-test").join(format!(
        "run-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&base).expect("create temp dir");
    base
}

const RESEARCH_MD: &str = "## Remote Work

### Key Claims
- Claim: Remote work increased productivity by 13% | Source: src_01 | Confidence: high

### Sources
- [src_01] Stanford WFH Study, Alice Smith, 2023, https://example.com/stanford, Productivity rose 13% across the sample
- [src_02] Harvard Business Review, Bob Jones, 2024, https://example.com/hbr, Collaboration networks shrank";

const OUTLINE_MD: &str = "## Thesis
Remote work changed culture rather than killing it.

## Argument Structure
1. **The Promise** — Set up the 2019 expectation | Sources: [src_01]
2. **What We Lost** — Show the collaboration cost | Sources: [src_02]

## Counterargument Section
Some companies thrived remote-first.

## Conclusion Arc
Culture moved, it did not die.";

const PASS_JSON: &str = r#"{"accuracy": 9.0, "coherence": 9.0, "style": 9.0, "source_integrity": 9.0, "overall": "pass", "hallucination_flags": [], "notes": "Strong."}"#;

/// Script the mock so the pipeline can run end to end:
/// research, outline, then per section: draft, evaluate(pass).
fn scripted_mock(section_count: usize) -> MockLlm {
    let mut contents: Vec<String> = Vec::new();
    contents.push(RESEARCH_MD.to_string());
    contents.push(OUTLINE_MD.to_string());
    for _ in 0..section_count {
        contents.push("Drafted section body.".to_string()); // draft
        contents.push(PASS_JSON.to_string()); // evaluate -> pass immediately
    }
    MockLlm::sequence(contents)
}

#[tokio::test]
async fn orchestrator_end_to_end_produces_expected_files() {
    let dir = temp_dir();
    let style = load_profile("magazine", None).expect("magazine");
    let mock = scripted_mock(2);

    let config = PipelineConfig {
        seed: "remote work and culture".to_string(),
        notes: Some("Focus on 2020-2024 data".to_string()),
        profile_name: "magazine".to_string(),
        audience_level: 3,
        target_words: 2500,
        project_dir: dir.clone(),
        verbose: false,
        revise: ReviseConfig {
            max_cycles: 3,
            exclude_provider: Some("deepseek".to_string()),
            ..ReviseConfig::default()
        },
    };

    let report = run_pipeline(&mock, &config, &style)
        .await
        .expect("pipeline should succeed");

    // Expected files.
    for name in [
        "seed.md",
        "research.md",
        "outline.md",
        "state.json",
        "sources.json",
    ] {
        assert!(dir.join(name).exists(), "missing {name}");
    }
    assert!(dir.join("sections").join("sec_01.md").exists());
    assert!(dir.join("sections").join("sec_02.md").exists());

    // state.json well-formed.
    let state_json = std::fs::read_to_string(dir.join("state.json")).expect("read state");
    let state: serde_json::Value = serde_json::from_str(&state_json).expect("state is valid JSON");
    assert_eq!(state["phase"], "complete");
    assert_eq!(state["style_profile"], "magazine");
    assert_eq!(state["audience_level"], 3);
    assert_eq!(state["target_words"], 2500);
    let sections = state["sections"].as_array().expect("sections array");
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0]["id"], "sec_01");
    assert_eq!(sections[0]["status"], "complete");
    assert!(state["overall_score"].is_number());

    // sources.json populated from research.
    let sources_json = std::fs::read_to_string(dir.join("sources.json")).expect("read sources");
    let sources: serde_json::Value = serde_json::from_str(&sources_json).expect("sources valid");
    let srcs = sources["sources"].as_array().expect("sources array");
    assert_eq!(srcs.len(), 2);
    assert_eq!(srcs[0]["id"], "src_01");
    assert!(srcs[0]["title"].as_str().unwrap_or("").contains("Stanford"));

    // Section content includes the heading.
    let sec =
        std::fs::read_to_string(dir.join("sections").join("sec_01.md")).expect("read sec");
    assert!(sec.contains("## The Promise"));

    assert_eq!(report.sections.len(), 2);
    assert!(report.sections.iter().all(|s| s.passed));

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn orchestrator_writes_revisions_when_multiple_cycles() {
    let dir = temp_dir();
    let style = load_profile("magazine", None).expect("magazine");

    // research, outline, draft, fail, brief, revise, pass  => 1 revision
    let fail_json = r#"{"accuracy": 5.0, "coherence": 6.0, "style": 7.0, "source_integrity": 8.0, "overall": "fail", "hallucination_flags": [], "notes": "Weak."}"#;
    let mock = MockLlm::sequence(vec![
        RESEARCH_MD.to_string(),
        OUTLINE_MD.to_string(),
        "Drafted body.".to_string(),
        fail_json.to_string(),
        "1. Fix the lede.".to_string(),
        "Revised body.".to_string(),
        PASS_JSON.to_string(),
    ]);

    let config = PipelineConfig {
        seed: "remote work".to_string(),
        notes: None,
        profile_name: "magazine".to_string(),
        audience_level: 3,
        target_words: 2500,
        project_dir: dir.clone(),
        verbose: false,
        revise: ReviseConfig {
            max_cycles: 3,
            exclude_provider: None,
            plateau_patience: 0,
            ..ReviseConfig::default()
        },
    };

    let report = run_pipeline(&mock, &config, &style).await.expect("pipeline");
    assert_eq!(report.sections.len(), 2);

    // revisions/ should hold an artifact for the section that needed a cycle.
    let revisions = dir.join("revisions");
    let has_revision = std::fs::read_dir(&revisions)
        .map(|rd| rd.flatten().count() > 0)
        .unwrap_or(false);
    assert!(has_revision, "expected a revision artifact");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn orchestrator_errors_on_outline_without_sections() {
    let dir = temp_dir();
    let style = load_profile("magazine", None).expect("magazine");
    let mock = MockLlm::sequence(vec![
        RESEARCH_MD.to_string(),
        "## Thesis\n\nNo argument structure here.".to_string(),
    ]);

    let config = PipelineConfig {
        seed: "x".to_string(),
        notes: None,
        profile_name: "magazine".to_string(),
        audience_level: 3,
        target_words: 500,
        project_dir: dir.clone(),
        verbose: false,
        revise: ReviseConfig::default(),
    };

    let result = run_pipeline(&mock, &config, &style).await;
    assert!(result.is_err(), "should error on empty outline sections");

    std::fs::remove_dir_all(&dir).ok();
}
