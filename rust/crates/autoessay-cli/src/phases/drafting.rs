//! Phase 2 — Drafting: section brief -> written section with style and source context.
//!
//! Ports Python pipeline/drafting.py verbatim, including the prompt text and the
//! _load_anti_slop behavior (disk copy preferred, bundled fallback).

use crate::domains::StyleProfile;
use crate::docs::load_anti_slop;
use crate::error::CliResult;
use crate::llm::{ChatMessage, ChatRequest, LlmClient};

/// Assemble the full system prompt: program + style + anti-slop.
/// Mirrors Python _build_system_prompt.
pub fn build_system_prompt(style: &StyleProfile) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Style instructions (the primary voice guide)
    parts.push(style.prompt_instructions.clone());

    // Anti-slop rules — loaded from ANTI-SLOP.md
    let anti_slop = load_anti_slop();
    if !anti_slop.is_empty() {
        parts.push("\n\n---\n\n## Writing Rules\n\n".to_string());
        parts.push(anti_slop);
    }

    parts.concat()
}

/// Assemble the user prompt with section brief, research, and sources.
/// Mirrors Python _build_user_prompt.
pub fn build_user_prompt(
    title: &str,
    purpose: &str,
    research: &str,
    sources: Option<&[String]>,
    previous_sections: Option<&[String]>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    parts.push(format!(
        "Write the following section:\n\n## {title}\n\n{purpose}"
    ));

    // Source references for this section
    if let Some(srcs) = sources {
        if !srcs.is_empty() {
            let src_list = srcs.join(", ");
            parts.push(format!("\n\nUse these sources: {src_list}"));
        }
    }

    // Research context (trim to relevant parts if too long)
    parts.push(format!("\n\n## Research Notes\n\n{research}"));

    // Previous sections for continuity
    if let Some(prev_sections) = previous_sections {
        if !prev_sections.is_empty() {
            let prev = prev_sections.join("\n\n---\n\n");
            parts.push(format!(
                "\n\n## Previous Sections (for continuity)\n\n{prev}"
            ));
        }
    }

    parts.push(
        "\n\nWrite only the section content. Do not repeat the section title as a heading — it will be added separately."
            .to_string(),
    );

    parts.join("\n")
}

/// Draft a single section of the essay.
///
/// Injects the style profile's prompt instructions, anti-slop rules, research
/// context, and source materials into the LLM call. Returns the drafted section
/// as markdown.
pub async fn draft_section<C: LlmClient>(
    llm: &C,
    section_title: &str,
    section_purpose: &str,
    research_notes: &str,
    style: &StyleProfile,
    sources: Option<&[String]>,
    previous_sections: Option<&[String]>,
) -> CliResult<String> {
    let system_prompt = build_system_prompt(style);
    let user_prompt = build_user_prompt(
        section_title,
        section_purpose,
        research_notes,
        sources,
        previous_sections,
    );

    let request = ChatRequest::for_role("drafting", vec![ChatMessage::user(user_prompt)])
        .prefer("deepseek")
        .system(system_prompt)
        .temperature(0.7)
        .max_tokens(4096);

    let resp = llm.chat(request).await?;
    Ok(resp.content)
}
