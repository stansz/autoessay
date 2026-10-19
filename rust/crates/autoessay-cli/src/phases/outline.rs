//! Phase 1b — Outline: research notes -> thesis + argument structure + section map.
//!
//! Ports Python pipeline/outline.py verbatim, including the prompt text.

use crate::domains::StyleProfile;
use crate::docs::load_program_section;
use crate::error::CliResult;
use crate::llm::{ChatMessage, ChatRequest, LlmClient};

/// Fallback outline instructions if program.md is unavailable.
/// Verbatim from Python _default_outline_program.
pub const DEFAULT_OUTLINE_PROGRAM: &str = "You are a structural editor. Given research notes, produce a thesis and section map.

Output format:
## Thesis
One-sentence thesis statement.

## Argument Structure
1. **Section Title** — One-sentence purpose | Sources: [src_01, src_02]
2. ...

## Counterargument Section
...

## Conclusion Arc
...

Rules:
- Thesis must be debatable, not a statement of fact
- Every section must reference sources from the research notes
- Counterarguments get their own section, not tucked into footnotes
- Non-fiction structure: intro/thesis → body → counter → conclusion";

/// Generate a thesis and section map from research notes.
///
/// Calls the LLM (smart tier) with outline program instructions. Returns a
/// structured markdown outline with thesis statement, argument structure,
/// counterargument section, and conclusion arc.
pub async fn generate_outline<C: LlmClient>(
    llm: &C,
    research_notes: &str,
    style: Option<&StyleProfile>,
) -> CliResult<String> {
    let mut program = load_program_section("Phase 1b: Outline (gen_outline.py)");
    if program.is_empty() {
        program = DEFAULT_OUTLINE_PROGRAM.to_string();
    }

    let mut system_prompt = program;
    if let Some(style) = style {
        system_prompt.push_str("\n\nTarget style: ");
        system_prompt.push_str(&style.use_case);
        system_prompt.push('\n');
        system_prompt.push_str(&style.prompt_instructions);
    }

    let user_prompt = format!(
        "Based on the following research notes, produce a thesis statement and a detailed section map:\n\n{research_notes}"
    );

    let request = ChatRequest::for_role("outline", vec![ChatMessage::user(user_prompt)])
        .system(system_prompt)
        .temperature(0.5);

    let resp = super::chat_with_continuation(llm, request).await?;
    Ok(resp.content)
}
