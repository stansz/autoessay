//! Outline review gate — score the outline before spending tokens on drafting.
//!
//! Long-form generation research keeps finding the outline is the central lever
//! for coherence (and that models happily draft from a weak one). This phase
//! asks the editor model to review the outline against a small rubric and, when
//! it fails, asks for one regeneration pass. The orchestrator treats a failed
//! second review as a warning rather than an abort: the outline is still
//! structurally parseable, and the user is told what is weak.

use autoessay_core::{OutlineReview, StyleProfile};

use crate::docs::load_program_section;
use crate::error::CliResult;
use crate::llm::{ChatMessage, ChatRequest, LlmClient};

use super::evaluation::extract_json;

const OUTLINE_REVIEW_SCHEMA: &str = "outline_review";

/// Build the system prompt for outline review.
pub fn build_outline_review_system_prompt(style: &StyleProfile) -> String {
    format!(
        "You are a structural editor reviewing an essay outline before drafting begins.\n\
         Score each dimension from 1-10:\n\
         - thesis: is the thesis one sentence, specific, and genuinely debatable?\n\
         - structure: does the argument build logically, section by section?\n\
         - source_coverage: is every section associated with evidence/sources?\n\
         - counterargument: is an opposing view engaged explicitly rather than dodged?\n\
         Return ONLY a JSON object with keys: thesis, structure, source_coverage, \
         counterargument (numbers), overall (\"pass\" or \"fail\"), issues (array of \
         concrete strings), notes (string).\n\
         Fail the outline if any dimension is below 6, if it has no counterargument \
         section, or if the thesis states a fact instead of an argument.\n\n\
         Target style: {}",
        style.name
    )
}

/// Build the user prompt for outline review.
pub fn build_outline_review_user_prompt(outline_md: &str, research_md: &str) -> String {
    format!(
        "## Research Notes (source of truth)\n\n{research_md}\n\n## Outline to Review\n\n{outline_md}"
    )
}

/// Review the outline. Falls back to a failed review when the model returns
/// unusable output (never panics; the caller may still proceed).
pub async fn review_outline<C: LlmClient>(
    llm: &C,
    outline_md: &str,
    research_md: &str,
    style: &StyleProfile,
) -> CliResult<OutlineReview> {
    let request = ChatRequest::for_role(
        "outline_review",
        vec![ChatMessage::user(build_outline_review_user_prompt(
            outline_md,
            research_md,
        ))],
    )
    .system(build_outline_review_system_prompt(style))
    .temperature(0.2)
    .max_tokens(1200)
    .response_format(outline_review_response_format());

    let response = llm.chat(request).await?;
    Ok(parse_outline_review(&response.content))
}

/// Parse an outline review from model output, salvaging JSON from fences/prose.
pub fn parse_outline_review(raw: &str) -> OutlineReview {
    let json = extract_json(raw);
    match serde_json::from_str::<OutlineReview>(&json) {
        Ok(review) => review,
        Err(_) => OutlineReview {
            overall: "fail".to_string(),
            issues: vec!["outline review returned unparseable output".to_string()],
            notes: format!("Raw output: {}", truncate(raw, 200)),
            ..OutlineReview::default()
        },
    }
}

/// Ask for a revised outline that addresses the review's issues.
pub async fn revise_outline<C: LlmClient>(
    llm: &C,
    outline_md: &str,
    review: &OutlineReview,
    research_md: &str,
    style: &StyleProfile,
) -> CliResult<String> {
    let mut program = load_program_section("Phase 1b: Outline (gen_outline.py)");
    if program.is_empty() {
        program = String::from(
            "You are a structural editor. Given research notes, produce a thesis and section map.",
        );
    }
    let system = format!(
        "{}\n\nTarget style: {}\n\nYou are revising an outline that failed review. Fix every issue, keep what already works, and return ONLY the full revised outline in the same markdown format.",
        program, style.use_case
    );
    let user = format!(
        "## Research Notes\n\n{research_md}\n\n## Current Outline\n\n{outline_md}\n\n## Review\n\nScores: thesis {:.1}, structure {:.1}, source coverage {:.1}, counterargument {:.1}\nOverall: {}\nIssues:\n- {}\nNotes: {}\n\nReturn the full revised outline.",
        review.thesis,
        review.structure,
        review.source_coverage,
        review.counterargument,
        review.overall,
        review.issues.join("\n- "),
        review.notes
    );

    let request = ChatRequest::for_role("outline", vec![ChatMessage::user(user)])
        .system(system)
        .temperature(0.5);

    let response = super::chat_with_continuation(llm, request).await?;
    Ok(response.content)
}

/// JSON Schema for the outline review response, sent as `response_format`.
pub fn outline_review_response_format() -> autoessay_providers::ResponseFormat {
    autoessay_providers::ResponseFormat::JsonSchema {
        name: OUTLINE_REVIEW_SCHEMA.to_string(),
        strict: true,
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "thesis": {"type": "number"},
                "structure": {"type": "number"},
                "source_coverage": {"type": "number"},
                "counterargument": {"type": "number"},
                "overall": {"type": "string", "enum": ["pass", "fail"]},
                "issues": {"type": "array", "items": {"type": "string"}},
                "notes": {"type": "string"}
            },
            "required": ["thesis", "structure", "source_coverage", "counterargument", "overall", "issues", "notes"],
            "additionalProperties": false
        }),
    }
}

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
