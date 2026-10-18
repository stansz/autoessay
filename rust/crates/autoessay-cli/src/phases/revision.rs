//! Phase 4 — Revision: evaluate -> revise -> tighten loop until quality gates pass.
//!
//! Ports Python pipeline/revision.py, including the prompt text and the
//! revise_until_pass loop. The Python loop stopped on pass or max_cycles; the
//! task also asks for the plateau logic from ARCHITECTURE.md ("Loop until scores
//! stabilize or max cycles reached"), so this port adds an explicit plateau
//! detector alongside the max-cycles cap.

use autoessay_core::{EvaluationResult, Severity, StyleProfile};

use crate::docs::load_program_section;
use crate::error::CliResult;
use crate::llm::{ChatMessage, ChatRequest, LlmClient};

use super::drafting::SectionBrief;
use super::evaluation::{evaluate_section, format_score};

/// Fallback revision-brief instructions if program.md is unavailable.
/// Verbatim from Python _default_revision_brief_program.
pub const DEFAULT_REVISION_BRIEF_PROGRAM: &str = "You are a revising editor. Given evaluation feedback, produce a revision brief.

Output a numbered list of specific, actionable changes:
1. [Paragraph 2] Fix the unsupported claim about X — either source it or remove it.
2. [Structure] The transition between paragraphs 3 and 4 is abrupt. Add a bridging sentence.
3. [Style] Paragraph 1 uses passive voice 4 times. Rewrite in active voice.

Rules:
- Every item must reference a specific location (paragraph, sentence, claim).
- Prioritize critical issues (hallucinations, factual errors) first.
- Don't suggest vague improvements (\"make it better\"). Be specific.
- Preserve what already works — only list what needs changing.";

/// Fallback revision instructions if program.md is unavailable.
/// Verbatim from Python _default_revise_program.
pub const DEFAULT_REVISE_PROGRAM: &str = "You are a revising editor. Rewrite the section following the revision brief.

Rules:
- Address every item in the revision brief.
- Preserve what already worked — don't rewrite the whole section if only paragraph 2 needs fixing.
- Respect the target style profile.
- If the revision brief asks you to remove a claim, remove it entirely — don't hedge.
- Return the complete revised section, not a diff.";

/// System prompt for revision — style instructions + revision rules.
/// Mirrors Python _build_revise_system_prompt.
pub fn build_revise_system_prompt(style: &StyleProfile) -> String {
    let mut program = load_program_section("Phase 4: Revision (gen_revision.py)");
    if program.is_empty() {
        program = DEFAULT_REVISE_PROGRAM.to_string();
    }
    format!("{}\n\n---\n\n{}", style.prompt_instructions, program)
}

/// Generate an actionable revision plan from evaluation feedback.
///
/// Uses the evaluation scores, hallucination flags, and editorial notes to
/// produce a concrete list of changes to make.
pub async fn generate_revision_brief<C: LlmClient>(
    llm: &C,
    section_text: &str,
    evaluation: &EvaluationResult,
    _style: &StyleProfile,
) -> CliResult<String> {
    let mut program = load_program_section("Phase 4: Revision (gen_revision.py)");
    if program.is_empty() {
        program = DEFAULT_REVISION_BRIEF_PROGRAM.to_string();
    }

    // Build a detailed prompt from evaluation data
    let flags_text = if evaluation.hallucination_flags.is_empty() {
        String::new()
    } else {
        evaluation
            .hallucination_flags
            .iter()
            .map(|f| format!("- [{}] {}: {}", severity_label(f.severity), f.claim, f.note))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let flags_display = if flags_text.is_empty() {
        "None".to_string()
    } else {
        flags_text
    };

    let user_prompt = format!(
        "## Original Section\n\n{section_text}\n\n## Evaluation Scores\n\n- Accuracy: {}/10\n- Coherence: {}/10\n- Style: {}/10\n- Source integrity: {}/10\n- Overall: {}\n\n## Hallucination Flags\n\n{}\n\n## Editor's Notes\n\n{}\n\nProduce a concrete, numbered revision brief. Each item must reference a specific paragraph or claim. Return only the revision brief.",
        format_score(evaluation.accuracy),
        format_score(evaluation.coherence),
        format_score(evaluation.style),
        format_score(evaluation.source_integrity),
        evaluation.overall,
        flags_display,
        evaluation.notes,
    );

    let request = ChatRequest::for_role("revision_brief", vec![ChatMessage::user(user_prompt)])
        .system(program)
        .temperature(0.5);

    let resp = llm.chat(request).await?;
    Ok(resp.content)
}

/// Rewrite a section following a revision brief.
///
/// Preserves what scored well and addresses every item in the brief. Returns the
/// complete revised section.
pub async fn revise_section<C: LlmClient>(
    llm: &C,
    section_text: &str,
    revision_brief: &str,
    research_notes: &str,
    style: &StyleProfile,
    brief: Option<SectionBrief>,
) -> CliResult<String> {
    let system_prompt = build_revise_system_prompt(style);

    let mut user_prompt = format!(
        "## Original Section\n\n{section_text}\n\n## Revision Brief\n\n{revision_brief}\n\n## Research Notes\n\n{research_notes}"
    );
    if let Some(brief) = brief {
        user_prompt.push_str(&brief.prompt_block());
    }
    user_prompt.push_str(
        "\n\nRewrite the section following the revision brief. Preserve what works. Fix what doesn't. Return only the revised section text — no preamble.",
    );

    let request = ChatRequest::for_role("revision", vec![ChatMessage::user(user_prompt)])
        .system(system_prompt)
        .temperature(0.6);

    let resp = llm.chat(request).await?;
    Ok(resp.content)
}

/// Configuration for the revise-until-pass loop.
#[derive(Debug, Clone)]
pub struct ReviseConfig {
    /// Maximum revision cycles before giving up.
    pub max_cycles: u32,
    /// Provider to exclude from evaluation (anti-self-review).
    ///
    /// The MVP leaves this unset: anti-self-review is enforced by resolving
    /// drafting and evaluation to different models (see providers.yaml).
    pub exclude_provider: Option<String>,
    /// Length/audience constraints passed to each rewrite.
    pub brief: Option<SectionBrief>,
    /// Stop early when the score plateau is smaller than this (0 disables).
    pub plateau_epsilon: f64,
    /// Number of consecutive non-improving evaluations that counts as a plateau.
    pub plateau_patience: u32,
}

impl Default for ReviseConfig {
    fn default() -> Self {
        ReviseConfig {
            max_cycles: 3,
            exclude_provider: None,
            brief: None,
            plateau_epsilon: 0.05,
            plateau_patience: 2,
        }
    }
}

/// The outcome of a revise-until-pass run.
#[derive(Debug, Clone)]
pub struct ReviseOutcome {
    /// The final section text (best effort if it never passed).
    pub final_section: String,
    /// Full evaluation history, most recent last.
    pub history: Vec<EvaluationResult>,
    /// Whether the loop stopped because scores plateaued.
    pub plateau_detected: bool,
    /// Whether the final evaluation passed.
    pub passed: bool,
    /// Number of revision cycles actually performed.
    pub cycles_run: u32,
}

/// Run the evaluate -> brief -> revise loop until the section passes, scores
/// plateau, or max_cycles is reached.
pub async fn revise_until_pass<C: LlmClient>(
    llm: &C,
    section_text: &str,
    research_notes: &str,
    style: &StyleProfile,
    config: &ReviseConfig,
) -> CliResult<ReviseOutcome> {
    let mut current = section_text.to_string();
    let mut history: Vec<EvaluationResult> = Vec::new();
    let mut cycles_run = 0u32;
    let mut plateau_detected = false;

    for _cycle in 1..=config.max_cycles {
        let exclude = config.exclude_provider.as_deref();
        let evaluation = evaluate_section(llm, &current, research_notes, style, exclude).await?;
        history.push(evaluation.clone());

        if evaluation.passed() {
            return Ok(ReviseOutcome {
                final_section: current,
                history,
                plateau_detected,
                passed: true,
                cycles_run,
            });
        }

        // Plateau detection: scores stopped improving.
        if score_plateau(&history, config.plateau_epsilon, config.plateau_patience) {
            plateau_detected = true;
            return Ok(ReviseOutcome {
                final_section: current,
                history,
                plateau_detected,
                passed: false,
                cycles_run,
            });
        }

        let brief = generate_revision_brief(llm, &current, &evaluation, style).await?;
        current =
            revise_section(llm, &current, &brief, research_notes, style, config.brief).await?;
        cycles_run += 1;
    }

    Ok(ReviseOutcome {
        final_section: current,
        history,
        plateau_detected,
        passed: false,
        cycles_run,
    })
}

/// Detect whether the recent evaluations have stopped improving overall.
///
/// A "score" here is the mean of the four dimensions. Walking the history from
/// the front, we track the best mean seen so far; a plateau is declared once the
/// trailing run of evaluations that failed to beat the previous best (by more
/// than epsilon) reaches patience in length.
pub fn score_plateau(history: &[EvaluationResult], epsilon: f64, patience: u32) -> bool {
    if patience == 0 {
        return false;
    }
    let needed = patience as usize;
    if history.len() < needed {
        return false;
    }

    let mut best_so_far = f64::MIN;
    let mut non_improving = 0usize;
    for eval in history {
        let mean = mean_score(eval);
        if mean > best_so_far + epsilon {
            best_so_far = mean;
            non_improving = 0;
        } else {
            non_improving += 1;
            if non_improving >= needed {
                return true;
            }
        }
    }
    false
}

/// The mean of the four scoring dimensions.
pub fn mean_score(eval: &EvaluationResult) -> f64 {
    (eval.accuracy + eval.coherence + eval.style + eval.source_integrity) / 4.0
}

/// The label for a severity in the revision-brief prompt.
fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "critical",
        Severity::Major => "major",
        Severity::Minor => "minor",
    }
}
