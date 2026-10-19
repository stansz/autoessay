//! Review-side phases that happen outside the drafting loop:
//!
//! * `propose_prompt_improvements` — the GEIS "evaluation → permanent patch"
//!   loop: turn recurring evaluation findings into concrete, reviewable edits
//!   for `program.md` / `ANTI-SLOP.md` / style profiles. It never edits those
//!   files itself; it writes a proposal for a human to accept.
//! * `run_reader_panel` — the three-persona reader panel from the original
//!   architecture, anchored to the finished essay instead of a single section.

use autoessay_core::StyleProfile;

use crate::error::CliResult;
use crate::llm::{ChatMessage, ChatRequest, LlmClient};

/// One persona's review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelReview {
    /// Persona name ("Domain expert", "General reader", "Editor").
    pub persona: String,
    /// Markdown review.
    pub review: String,
}

/// Ask a smart-tier model to turn project findings into prompt patches.
pub async fn propose_prompt_improvements<C: LlmClient>(
    llm: &C,
    project_context: &str,
    style: &StyleProfile,
) -> CliResult<String> {
    let system = format!(
        "You are improving an AI essay-writing pipeline, not the essay itself.\n\
         Given the project state and evaluation findings below, identify recurring \
         failure patterns and propose concrete, minimal patches to the pipeline's \
         prompt assets: program.md phase instructions, ANTI-SLOP.md rules, and the \
         '{}' style profile's prompt_instructions.\n\n\
         Output markdown with:\n\
         1. Recurring failures (with evidence from the project data)\n\
         2. Proposed patch per file: quote the current rule, then the replacement\n\
         3. Why the patch should help, and how to verify it on the next run\n\n\
         Be specific and conservative: prefer 2-4 high-value edits over a rewrite. \
         Do not invent findings that are not in the provided data.",
        style.name
    );
    let user = format!(
        "## Project findings\n\n{project_context}\n\n## Target style profile\n\n{}\n\n{}",
        style.name, style.prompt_instructions
    );

    let request = ChatRequest::for_role("revision_brief", vec![ChatMessage::user(user)])
        .system(system)
        .temperature(0.4)
        .max_tokens(2400);

    let response = super::chat_with_continuation(llm, request).await?;
    Ok(response.content)
}

/// Ask three personas to review the finished essay.
pub async fn run_reader_panel<C: LlmClient>(
    llm: &C,
    essay: &str,
    style: &StyleProfile,
) -> CliResult<Vec<PanelReview>> {
    let personas: [(&str, &str); 3] = [
        (
            "Domain expert",
            "You are a demanding domain expert. Check factual depth, nuance, whether \
             claims would survive expert scrutiny, and what is missing. Be specific \
             and cite paragraphs.",
        ),
        (
            "General reader",
            "You are an educated general reader with no special background. Judge \
             clarity, engagement, and whether every concept is explained. Point to \
             the exact places you got lost or bored.",
        ),
        (
            "Editor",
            "You are a line editor. Judge structure, flow, transitions, pacing, and \
             whether the piece earns its word count. Cite paragraphs and suggest \
             concrete cuts or rewrites.",
        ),
    ];

    let mut reviews = Vec::new();
    for (persona, instruction) in personas {
        let system = format!(
            "{instruction}\n\nTarget style: {}.\n\nReturn a concise markdown review \
             (under 300 words) with a verdict and 3-5 concrete, paragraph-anchored \
             observations. Do not rewrite the essay.",
            style.name
        );
        let request = ChatRequest::for_role("reader_panel", vec![ChatMessage::user(format!(
            "## Essay\n\n{essay}"
        ))])
        .system(system)
        .temperature(0.4)
        .max_tokens(1200);

        let response = super::chat_with_continuation(llm, request).await?;
        reviews.push(PanelReview {
            persona: persona.to_string(),
            review: response.content,
        });
    }
    Ok(reviews)
}

/// Render a panel result as one markdown document.
pub fn render_panel(reviews: &[PanelReview]) -> String {
    let mut out = String::from("# Reader panel\n\n");
    for review in reviews {
        out.push_str(&format!("## {}\n\n{}\n\n", review.persona, review.review.trim()));
    }
    out.trim_end().to_string() + "\n"
}
