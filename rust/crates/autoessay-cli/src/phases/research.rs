//! Phase 1 — Research: topic -> structured research notes with source tracking.
//!
//! Ports Python pipeline/research.py verbatim, including the prompt text.

use crate::domains::StyleProfile;
use crate::docs::load_program_section;
use crate::error::CliResult;
use crate::llm::{ChatMessage, ChatRequest, LlmClient};

/// Fallback research instructions if program.md is unavailable.
/// Verbatim from Python _default_research_program.
pub const DEFAULT_RESEARCH_PROGRAM: &str = "You are a research assistant. Given a topic, produce structured research notes.

Output format:
## <Topic>

### Key Claims
- Claim: ... | Source: <identifier> | Confidence: high/medium/low

### Supporting Evidence
- Evidence: ... | Source: <identifier>

### Counterarguments
- Counter: ... | Source: <identifier>

### Sources
- [src_01] Title, Author, Year, URL, Key Quote

Rules:
- Every claim must link to a source identifier
- Do not fabricate sources — if you don't have one, say \"unsourced claim\"
- Prefer primary sources over secondary
- Flag speculative claims explicitly";

/// Generate structured research notes for a topic.
///
/// Calls the LLM (smart tier) with research program instructions and optional
/// style context. Returns markdown with key claims, supporting evidence,
/// counterarguments, and a tracked source list.
pub async fn generate_research<C: LlmClient>(
    llm: &C,
    topic: &str,
    seed: Option<&str>,
    style: Option<&StyleProfile>,
) -> CliResult<String> {
    let mut program = load_program_section("Phase 1: Research (gen_research.py)");
    if program.is_empty() {
        program = DEFAULT_RESEARCH_PROGRAM.to_string();
    }

    let mut system_prompt = program;
    if let Some(style) = style {
        system_prompt.push_str(
            "\n\nStyle context (for reference — research output is style-agnostic):",
        );
        system_prompt.push('\n');
        system_prompt.push_str(&style.prompt_instructions);
    }

    let mut user_prompt = format!("Research the following topic:\n\n{topic}");
    if let Some(seed) = seed {
        if !seed.is_empty() {
            user_prompt.push_str("\n\nAdditional context from the user:\n");
            user_prompt.push_str(seed);
        }
    }

    let request = ChatRequest::for_role("research", vec![ChatMessage::user(user_prompt)])
        .system(system_prompt)
        .temperature(0.4); // lower temp for factual research

    let resp = llm.chat(request).await?;
    Ok(resp.content)
}
