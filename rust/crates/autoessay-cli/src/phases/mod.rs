//! The five pipeline phases.
//!
//! Each module ports one Python pipeline/*.py file verbatim (prompt text
//! included). Every phase function takes "&impl LlmClient" so it is testable
//! offline against a MockLlm.

pub mod drafting;
pub mod evaluation;
pub mod outline;
pub mod outline_review;
pub mod research;
pub mod review;
pub mod revision;

// Outline extraction helper used by the orchestrator.
pub mod outline_parse;

use crate::error::{CliError, CliResult};
use crate::llm::{ChatMessage, ChatRequest, LlmClient, LlmResponse};

pub use drafting::{build_system_prompt, build_user_prompt, draft_section, SectionBrief};
pub use evaluation::{
    build_eval_system_prompt, build_eval_user_prompt, eval_response_format, evaluate_section,
    evaluate_section_ensemble, extract_json, merge_evaluations, parse_eval_response,
};
pub use outline::generate_outline;
pub use outline_parse::{parse_outline, OutlineSection, ParsedOutline};
pub use outline_review::{
    build_outline_review_system_prompt, build_outline_review_user_prompt,
    outline_review_response_format, parse_outline_review, revise_outline, review_outline,
};
pub use research::generate_research;
pub use review::{propose_prompt_improvements, render_panel, run_reader_panel, PanelReview};
pub use revision::{
    generate_revision_brief, mean_score, revise_section, revise_until_pass, score_plateau,
    ReviseConfig, ReviseOutcome,
};

/// How many additional calls may be made to finish a truncated output.
pub const MAX_CONTINUATIONS: u32 = 2;

const CONTINUE_PROMPT: &str =
    "Continue exactly where the text stopped. Do not repeat any text, do not add a heading, and do not summarize.";

/// Run a chat request, continuing automatically when the model stops because it
/// hit the token limit (`finish_reason == "length"` / `"max_tokens"`).
///
/// Long-form writing is the one place where silent truncation is unacceptable:
/// a section that silently loses its ending passes no human review, and the
/// pipeline would otherwise treat it as complete. Continuations are appended
/// verbatim; the caller still owns any length validation.
pub async fn chat_with_continuation<C: LlmClient>(
    llm: &C,
    request: ChatRequest,
) -> CliResult<LlmResponse> {
    let mut request = request;
    let mut content = String::new();
    let mut last: Option<LlmResponse> = None;

    for attempt in 0..=MAX_CONTINUATIONS {
        let response = llm.chat(request.clone()).await?;
        content.push_str(&response.content);
        let truncated = matches!(response.finish_reason.as_str(), "length" | "max_tokens");
        let merged = LlmResponse {
            content: content.clone(),
            model: response.model,
            provider: response.provider,
            usage: response.usage,
            finish_reason: response.finish_reason,
        };
        last = Some(merged);
        if !truncated || attempt == MAX_CONTINUATIONS {
            break;
        }
        request.messages.push(ChatMessage::assistant(response.content));
        request
            .messages
            .push(ChatMessage::user(CONTINUE_PROMPT));
    }

    last.ok_or_else(|| CliError::Llm("continuation loop produced no response".to_string()))
}
