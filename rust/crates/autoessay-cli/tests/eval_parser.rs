// Tests are the one place where unwrap/expect/panic! are allowed by the
// project design rule (no unwrap/expect/panic! outside tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Fixture-driven tests for the evaluation response parser.
//!
//! The Python _parse_eval_response is the fragile part; these tests hammer the
//! Rust port with realistic (and malformed) model outputs to prove it is robust
//! and never panics.

use autoessay_cli::phases::evaluation::{
    build_eval_system_prompt, build_eval_user_prompt, extract_json, parse_eval_response,
};
use autoessay_cli::styles::load_profile;
use autoessay_core::{EvaluationResult, Severity, StyleProfile};

fn magazine() -> StyleProfile {
    load_profile("magazine", None).expect("magazine profile should load")
}
fn academic() -> StyleProfile {
    load_profile("academic", None).expect("academic profile should load")
}
fn personal() -> StyleProfile {
    load_profile("personal-essay", None).expect("personal-essay profile should load")
}

const VALID_JSON: &str = r#"{
  "accuracy": 8.5,
  "coherence": 8.0,
  "style": 9.0,
  "source_integrity": 8.0,
  "overall": "pass",
  "hallucination_flags": [],
  "notes": "Strong opening. Paragraph 2 needs a smoother transition."
}"#;

// ═══════════════════════════════════════════════════════════════════════════
// Extraction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn extracts_bare_json() {
    assert!(extract_json(VALID_JSON).trim_start().starts_with('{'));
}

#[test]
fn extracts_from_json_code_fence() {
    let raw = format!("```json\n{VALID_JSON}\n```");
    let out = extract_json(&raw);
    assert!(out.trim_start().starts_with('{'), "got: {out}");
    assert!(out.contains("accuracy"));
}

#[test]
fn extracts_from_unlabeled_code_fence() {
    let raw = format!("```\n{VALID_JSON}\n```");
    let out = extract_json(&raw);
    assert!(out.trim_start().starts_with('{'), "got: {out}");
}

#[test]
fn extracts_json_from_wrapping_prose() {
    let raw = format!("Here is the evaluation:\n\n{VALID_JSON}\n\nHope that helps.");
    let out = extract_json(&raw);
    assert!(out.trim_start().starts_with('{'), "got: {out}");
    assert!(out.contains("accuracy"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Parsing — happy path
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parses_valid_json() {
    let result = parse_eval_response(VALID_JSON, &magazine());
    assert!(result.passed(), "expected pass: {:?}", result);
    assert!((result.accuracy - 8.5).abs() < f64::EPSILON);
    assert!((result.coherence - 8.0).abs() < f64::EPSILON);
    assert!((result.style - 9.0).abs() < f64::EPSILON);
    assert!((result.source_integrity - 8.0).abs() < f64::EPSILON);
    assert!(result.notes.contains("smoother transition"));
}

#[test]
fn parses_json_in_code_fence() {
    let raw = format!("```json\n{VALID_JSON}\n```");
    let result = parse_eval_response(&raw, &magazine());
    assert!(result.passed());
    assert!((result.accuracy - 8.5).abs() < f64::EPSILON);
}

#[test]
fn parses_json_with_trailing_prose() {
    let raw = format!("{VALID_JSON}\n\nNote: I also considered the source density.");
    let result = parse_eval_response(&raw, &magazine());
    assert!((result.accuracy - 8.5).abs() < f64::EPSILON);
}

#[test]
fn parses_integer_scores() {
    let raw = r#"{"accuracy": 9, "coherence": 9, "style": 9, "source_integrity": 9, "overall": "pass", "hallucination_flags": [], "notes": "ok"}"#;
    let result = parse_eval_response(raw, &magazine());
    assert!(result.passed());
    assert!((result.accuracy - 9.0).abs() < f64::EPSILON);
}

// ═══════════════════════════════════════════════════════════════════════════
// Parsing — hallucination flags
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn builds_hallucination_flags() {
    let raw = r#"{
      "accuracy": 8.5, "coherence": 8.0, "style": 9.0, "source_integrity": 8.0,
      "overall": "pass",
      "hallucination_flags": [
        {"claim": "Fake claim", "severity": "major", "note": "Not found in sources"}
      ],
      "notes": "n"
    }"#;
    let result = parse_eval_response(raw, &magazine());
    assert_eq!(result.hallucination_flags.len(), 1);
    assert_eq!(result.hallucination_flags[0].claim, "Fake claim");
    assert_eq!(result.hallucination_flags[0].severity, Severity::Major);
    assert_eq!(result.hallucination_flags[0].note, "Not found in sources");
}

#[test]
fn flag_defaults_to_minor_when_severity_missing() {
    let raw = r#"{"accuracy": 8.5, "coherence": 8.0, "style": 9.0, "source_integrity": 8.0,
      "overall": "pass", "hallucination_flags": [{"claim": "x"}], "notes": ""}"#;
    let result = parse_eval_response(raw, &magazine());
    assert_eq!(result.hallucination_flags[0].severity, Severity::Minor);
}

// ═══════════════════════════════════════════════════════════════════════════
// Threshold enforcement (mirrors Python TestThresholdEnforcement)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fails_below_accuracy_threshold() {
    let raw = r#"{"accuracy": 6.0, "coherence": 8.0, "style": 9.0, "source_integrity": 8.0,
      "overall": "fail", "hallucination_flags": [], "notes": ""}"#;
    let result = parse_eval_response(raw, &magazine()); // magazine accuracy 7.0
    assert!(!result.passed());
    assert!((result.accuracy - 6.0).abs() < f64::EPSILON);
}

#[test]
fn fails_below_coherence_threshold() {
    let raw = r#"{"accuracy": 9.0, "coherence": 4.0, "style": 9.0, "source_integrity": 9.0,
      "overall": "pass", "hallucination_flags": [], "notes": ""}"#;
    let result = parse_eval_response(raw, &magazine());
    assert!(!result.passed());
}

#[test]
fn passes_when_all_above_thresholds() {
    let raw = r#"{"accuracy": 9.0, "coherence": 9.0, "style": 9.0, "source_integrity": 9.0,
      "overall": "pass", "hallucination_flags": [], "notes": ""}"#;
    let result = parse_eval_response(raw, &magazine());
    assert!(result.passed());
}

#[test]
fn personal_essay_skips_accuracy_check() {
    // personal-essay has accuracy_threshold=None.
    let raw = r#"{"accuracy": 1.0, "coherence": 7.0, "style": 9.0, "source_integrity": 9.0,
      "overall": "pass", "hallucination_flags": [], "notes": ""}"#;
    let result = parse_eval_response(raw, &personal());
    assert!((result.accuracy - 1.0).abs() < f64::EPSILON);
    // coherence 6.0 threshold, score 7.0 -> pass
    assert!(result.passed());
}

// ═══════════════════════════════════════════════════════════════════════════
// Hallucination gate (mirrors Python TestHallucinationGate)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn strict_gate_critical_flag_auto_fails() {
    let raw = r#"{"accuracy": 9.0, "coherence": 9.0, "style": 9.0, "source_integrity": 9.0,
      "overall": "pass",
      "hallucination_flags": [{"claim": "Made-up stat", "severity": "critical", "note": "Not in sources"}],
      "notes": ""}"#;
    let result = parse_eval_response(raw, &academic()); // strict gate
    assert!(!result.passed());
    assert_eq!(result.hallucination_flags.len(), 1);
}

#[test]
fn lenient_gate_single_critical_passes() {
    let raw = r#"{"accuracy": 9.0, "coherence": 8.0, "style": 9.0, "source_integrity": 8.0,
      "overall": "pass",
      "hallucination_flags": [{"claim": "Minor issue", "severity": "critical", "note": "Borderline"}],
      "notes": ""}"#;
    let result = parse_eval_response(raw, &magazine()); // lenient gate
    assert!(result.passed());
}

#[test]
fn lenient_gate_two_critical_fails() {
    let raw = r#"{"accuracy": 9.0, "coherence": 8.0, "style": 9.0, "source_integrity": 8.0,
      "overall": "pass",
      "hallucination_flags": [
        {"claim": "Issue 1", "severity": "critical", "note": "a"},
        {"claim": "Issue 2", "severity": "critical", "note": "b"}
      ],
      "notes": ""}"#;
    let result = parse_eval_response(raw, &magazine());
    assert!(!result.passed());
}

#[test]
fn off_gate_ignores_flags() {
    let raw = r#"{"accuracy": 9.0, "coherence": 8.0, "style": 9.0, "source_integrity": 9.0,
      "overall": "pass",
      "hallucination_flags": [{"claim": "Made up", "severity": "critical", "note": "Fake"}],
      "notes": ""}"#;
    let result = parse_eval_response(raw, &personal()); // off gate
    assert!(result.passed());
}

// ═══════════════════════════════════════════════════════════════════════════
// Failure / missing-field tolerance — never panics
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn handles_invalid_json_gracefully() {
    let result = parse_eval_response("not json at all", &magazine());
    assert!(!result.passed());
    assert!(
        result
            .notes
            .to_lowercase()
            .contains("failed to produce valid json"),
        "notes: {}",
        result.notes
    );
}

#[test]
fn missing_fields_default_to_zero_not_panic() {
    let raw = r#"{"notes": "no scores here"}"#;
    let result = parse_eval_response(raw, &magazine());
    assert!((result.accuracy - 0.0).abs() < f64::EPSILON);
    assert!((result.coherence - 0.0).abs() < f64::EPSILON);
    assert!(!result.passed()); // 0 < thresholds
}

#[test]
fn missing_hallucination_flags_is_empty() {
    let raw = r#"{"accuracy": 9.0, "coherence": 9.0, "style": 9.0, "source_integrity": 9.0, "overall": "pass", "notes": ""}"#;
    let result = parse_eval_response(raw, &magazine());
    assert!(result.hallucination_flags.is_empty());
}

#[test]
fn empty_string_is_a_parse_failure_not_a_panic() {
    let result = parse_eval_response("", &magazine());
    assert!(!result.passed());
    assert!(matches!(result, EvaluationResult { .. }));
}

#[test]
fn json_array_not_object_is_a_parse_failure() {
    let result = parse_eval_response("[1,2,3]", &magazine());
    assert!(!result.passed());
}

#[test]
fn truncated_json_is_a_parse_failure() {
    let raw = r#"{"accuracy": 8.5, "coherence": 8.0, "sty"#;
    let result = parse_eval_response(raw, &magazine());
    assert!(!result.passed());
}

// ═══════════════════════════════════════════════════════════════════════════
// Prompt assembly
// ═══════════════════════════════════════════════════════════════════════════

const SECTION: &str = "Remote work didn't kill culture. According to a 2023 Stanford study, productivity rose 13% [src_01].";
const RESEARCH: &str = "## Remote Work\n\n### Sources\n- [src_01] Stanford WFH Study, 2023";

#[test]
fn user_prompt_includes_section_and_research() {
    let prompt = build_eval_user_prompt(SECTION, RESEARCH);
    assert!(prompt.contains("Stanford study"));
    assert!(prompt.contains("Section to Evaluate"));
    assert!(prompt.contains("Research Notes"));
}

#[test]
fn system_prompt_includes_thresholds_and_profile_name() {
    let prompt = build_eval_system_prompt(&magazine());
    assert!(prompt.to_lowercase().contains("magazine"));
    assert!(prompt.contains("7.0")); // magazine accuracy threshold
    assert!(prompt.contains("Quality Thresholds"));
}

#[test]
fn system_prompt_shows_na_for_null_thresholds() {
    let prompt = build_eval_system_prompt(&personal());
    assert!(prompt.contains("N/A"));
}

#[test]
fn system_prompt_includes_program_md_phase_3() {
    // Regression: the backticked program.md heading used to make the lookup miss
    // and silently fall back to the short default. The real Phase 3 text must
    // now be present, and it must still demand the JSON contract.
    let prompt = build_eval_system_prompt(&magazine());
    assert!(
        prompt.contains("You are an editor"),
        "Phase 3 program.md instructions missing from evaluator prompt"
    );
    // The real program.md shows the JSON contract inside a ```json fence rather
    // than spelling out "Return ONLY JSON", so match case-insensitively.
    assert!(
        prompt.to_lowercase().contains("json"),
        "prompt should demand a JSON object"
    );
}
