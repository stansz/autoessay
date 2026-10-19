//! Phase 2 — Drafting: section brief -> written section with style and source context.
//!
//! Ports Python pipeline/drafting.py, including the prompt text and the
//! _load_anti_slop behavior (disk copy preferred, bundled fallback). Adds the
//! runtime program.md Phase 2 instructions and the length/audience controls the
//! CLI advertises.

use crate::domains::StyleProfile;
use crate::docs::{load_anti_slop, load_program_section};
use crate::error::CliResult;
use crate::llm::{ChatMessage, ChatRequest, LlmClient};

/// Heading of the drafting instructions in program.md.
const DRAFTING_PROGRAM_HEADING: &str = "Phase 2: Drafting (draft_section.py)";

/// Length and audience constraints for one section's prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionBrief {
    /// Approximate word budget for this section (0 disables the hint).
    pub target_words: u32,
    /// Audience level 1-5 (1 = general reader, 5 = specialist).
    pub audience_level: u8,
}

impl SectionBrief {
    /// The prompt block describing length and audience, or "" when no target is
    /// set.
    pub fn prompt_block(&self) -> String {
        if self.target_words == 0 {
            return String::new();
        }
        format!(
            "\n\n## Length and Audience\n\n- Target length for this section: about {} words. Stay close to it — do not pad.\n- Audience level: {}/5 (1 = general reader, 5 = specialist). Match vocabulary and assumed knowledge to that level.",
            self.target_words, self.audience_level
        )
    }
}

/// Assemble the full system prompt: style + program.md Phase 2 + anti-slop.
/// Mirrors Python _build_system_prompt, plus the runtime program section.
pub fn build_system_prompt(style: &StyleProfile) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Style instructions (the primary voice guide)
    parts.push(style.prompt_instructions.clone());

    // Phase 2 instructions from program.md (loaded at runtime; disk edits win).
    let program = load_program_section(DRAFTING_PROGRAM_HEADING);
    if !program.is_empty() {
        parts.push("\n\n---\n\n".to_string());
        parts.push(program);
    }

    // Anti-slop rules — loaded from ANTI-SLOP.md
    let anti_slop = load_anti_slop();
    if !anti_slop.is_empty() {
        parts.push("\n\n---\n\n## Writing Rules\n\n".to_string());
        parts.push(anti_slop);
    }

    parts.concat()
}

/// Assemble the user prompt with research, sources, and section brief.
///
/// Ordering is deliberate: the research notes are identical for every section in
/// a run, so they come first to maximise the provider's prompt-cache prefix hit;
/// only the section-specific material follows.
pub fn build_user_prompt(
    title: &str,
    purpose: &str,
    research: &str,
    sources: Option<&[String]>,
    previous_sections: Option<&[String]>,
    brief: Option<SectionBrief>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Research context (stable prefix across all sections).
    parts.push(format!("## Research Notes (source of truth)\n\n{research}"));

    // Source references for this section
    if let Some(srcs) = sources {
        if !srcs.is_empty() {
            let src_list = srcs.join(", ");
            parts.push(format!("\n\n## Sources for This Section\n\nUse these sources: {src_list}"));
        }
    }

    // Previous sections for continuity
    if let Some(prev_sections) = previous_sections {
        if !prev_sections.is_empty() {
            let prev = prev_sections.join("\n\n---\n\n");
            parts.push(format!(
                "\n\n## Previous Sections (for continuity)\n\n{prev}"
            ));
        }
    }

    // Section-specific brief goes last so the shared prefix stays cacheable.
    parts.push(format!(
        "\n\n## Section Brief\n\n## {title}\n\n{purpose}"
    ));

    if let Some(brief) = brief {
        parts.push(brief.prompt_block());
    }

    parts.push(
        "\n\nWrite only the section content. Do not repeat the section title as a heading — it will be added separately."
            .to_string(),
    );

    parts.join("\n")
}

/// Draft a single section of the essay.
///
/// Injects the style profile's prompt instructions, program.md Phase 2
/// instructions, anti-slop rules, research context, sources, and length/audience
/// constraints into the LLM call. Returns the drafted section as markdown.
// The signature mirrors the Python phase arguments one-for-one; grouping them
// into a struct here would only obscure the call sites.
#[allow(clippy::too_many_arguments)]
pub async fn draft_section<C: LlmClient>(
    llm: &C,
    section_title: &str,
    section_purpose: &str,
    research_notes: &str,
    style: &StyleProfile,
    sources: Option<&[String]>,
    previous_sections: Option<&[String]>,
    brief: Option<SectionBrief>,
) -> CliResult<String> {
    let system_prompt = build_system_prompt(style);
    let user_prompt = build_user_prompt(
        section_title,
        section_purpose,
        research_notes,
        sources,
        previous_sections,
        brief,
    );

    let request = ChatRequest::for_role("drafting", vec![ChatMessage::user(user_prompt)])
        .system(system_prompt)
        .temperature(0.7)
        .max_tokens(4096);

    let resp = super::chat_with_continuation(llm, request).await?;
    Ok(resp.content)
}
