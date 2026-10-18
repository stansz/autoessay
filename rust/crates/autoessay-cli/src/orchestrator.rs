//! The end-to-end orchestrator — the piece the Python never had.
//!
//! Runs research -> outline -> per-section draft -> evaluate -> revise-until-pass
//! -> write out, persisting state.json and sources.json after every phase and
//! laying the project out in the "GitHub as review surface" format from
//! ARCHITECTURE.md:
//!
//! <project>/
//!   seed.md
//!   research.md
//!   outline.md
//!   sources.json
//!   state.json
//!   sections/sec_01.md ...
//!   revisions/ ...
//!
//! Git commits are a later task; this module only writes the files in that
//! layout and never shells out to git.

use std::path::{Path, PathBuf};

use autoessay_core::{
    EvaluationResult, Phase, ProjectState, Scores, SectionState, SectionStatus, SourcesFile,
    StyleProfile,
};

use crate::error::{CliError, CliResult};
use crate::llm::LlmClient;
use crate::phases::{
    draft_section, evaluate_section, generate_outline, generate_research, generate_revision_brief,
    mean_score, parse_outline, revise_section, revise_until_pass, ReviseConfig, SectionBrief,
};
use crate::sources::extract_sources;

/// Knobs for a full pipeline run.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// The seed topic / one-paragraph brief.
    pub seed: String,
    /// Optional user notes, angles, or materials.
    pub notes: Option<String>,
    /// Style profile name (e.g. "magazine").
    pub profile_name: String,
    /// Audience level 1-5 (general -> expert).
    pub audience_level: u8,
    /// Target length in words.
    pub target_words: u32,
    /// Where to write the project. Created if absent.
    pub project_dir: PathBuf,
    /// Print progress to stdout.
    pub verbose: bool,
    /// Revise-loop configuration (max cycles, plateau, anti-self-review).
    pub revise: ReviseConfig,
}

/// A summary of a completed (or partially completed) run.
#[derive(Debug, Clone)]
pub struct PipelineReport {
    pub project_dir: PathBuf,
    pub research_md: String,
    pub outline_md: String,
    pub sections: Vec<SectionReport>,
    /// The assembled markdown document written to `essay.md`.
    pub essay_md: String,
    pub state: ProjectState,
    pub sources: SourcesFile,
    pub plateau_detected: bool,
}

/// Per-section outcome.
#[derive(Debug, Clone)]
pub struct SectionReport {
    pub id: String,
    pub title: String,
    pub final_text: String,
    pub evaluations: Vec<EvaluationResult>,
    pub cycles: u32,
    pub passed: bool,
    pub plateau: bool,
}

/// Run the full pipeline against an injected LLM client.
pub async fn run_pipeline<C: LlmClient>(
    llm: &C,
    config: &PipelineConfig,
    style: &StyleProfile,
) -> CliResult<PipelineReport> {
    ensure_distinct_models(llm)?;

    let dir = &config.project_dir;
    std::fs::create_dir_all(dir)?;
    std::fs::create_dir_all(dir.join("sections"))?;
    std::fs::create_dir_all(dir.join("revisions"))?;

    let mut state = ProjectState {
        phase: Phase::Research,
        iteration: 0,
        style_profile: config.profile_name.clone(),
        audience_level: config.audience_level,
        target_words: config.target_words,
        sections: Vec::new(),
        overall_score: None,
        plateau_detected: false,
    };

    // ---- Phase 0: seed ----
    let seed_md = build_seed_md(config, style);
    write_file(&dir.join("seed.md"), &seed_md)?;

    // ---- Phase 1: research ----
    progress(config, "Phase 1: Research");
    let research_md =
        generate_research(llm, &config.seed, config.notes.as_deref(), Some(style)).await?;
    write_file(&dir.join("research.md"), &research_md)?;
    let sources = extract_sources(&research_md);
    write_file(&dir.join("sources.json"), &sources.to_json().map_err(map_core_err)?)?;
    state.phase = Phase::Research;
    state.iteration += 1;
    write_state(&state, dir)?;

    // ---- Phase 1b: outline ----
    progress(config, "Phase 1b: Outline");
    let outline_md = generate_outline(llm, &research_md, Some(style)).await?;
    write_file(&dir.join("outline.md"), &outline_md)?;
    let parsed = parse_outline(&outline_md);
    if parsed.sections.is_empty() {
        return Err(CliError::project(
            "outline contained no numbered sections under 'Argument Structure'; cannot draft",
        ));
    }
    state.sections = parsed
        .sections
        .iter()
        .enumerate()
        .map(|(i, s)| SectionState {
            id: section_id(i + 1),
            title: s.title.clone(),
            status: SectionStatus::Pending,
            scores: Scores::default(),
            revision_cycles: 0,
            hallucination_flags: Vec::new(),
        })
        .collect();
    state.phase = Phase::Outline;
    state.iteration += 1;
    write_state(&state, dir)?;

    // ---- Phases 2-4: per-section draft -> evaluate -> revise ----
    let mut reports: Vec<SectionReport> = Vec::new();
    let mut previous: Vec<String> = Vec::new();
    let mut plateau_any = false;
    let section_count = parsed.sections.len().max(1);
    let section_brief = section_brief(config.target_words, config.audience_level, section_count);

    for (idx, section) in parsed.sections.iter().enumerate() {
        let id = section_id(idx + 1);
        progress(
            config,
            &format!("Phase 2-4: Section {id} — {}", section.title),
        );

        let draft = draft_section(
            llm,
            &section.title,
            &section.purpose,
            &research_md,
            style,
            Some(&section.sources),
            if previous.is_empty() {
                None
            } else {
                Some(&previous)
            },
            section_brief,
        )
        .await?;

        // Persist the draft before evaluation: if the evaluation call fails the
        // paid-for draft is still on disk and the run can be inspected.
        let draft_body = format!("## {}\n\n{}", section.title, draft);
        write_file(&dir.join("sections").join(format!("{id}.md")), &draft_body)?;

        let mut revise_config = config.revise.clone();
        revise_config.brief = section_brief;

        let outcome = revise_until_pass(llm, &draft, &research_md, style, &revise_config).await?;

        // Write the final section file (title heading + body).
        let body = format!("## {}\n\n{}", section.title, outcome.final_section);
        write_file(&dir.join("sections").join(format!("{id}.md")), &body)?;

        // Write revision artifacts when there was more than one cycle.
        if outcome.history.len() > 1 {
            let rev_path = dir
                .join("revisions")
                .join(format!("{id}-v{}.md", outcome.history.len()));
            write_file(&rev_path, &body)?;
        }

        // Update state for this section.
        if let Some(entry) = state.sections.iter_mut().find(|s| s.id == id) {
            entry.status = if outcome.passed {
                SectionStatus::Complete
            } else {
                SectionStatus::Drafted
            };
            entry.revision_cycles = outcome.cycles_run;
            if let Some(last) = outcome.history.last() {
                entry.scores = Scores {
                    accuracy: last.accuracy,
                    coherence: last.coherence,
                    style: last.style,
                    source_integrity: last.source_integrity,
                };
                entry.hallucination_flags = last.hallucination_flags.clone();
            }
        }

        if outcome.plateau_detected {
            plateau_any = true;
        }

        previous.push(body.clone());
        reports.push(SectionReport {
            id,
            title: section.title.clone(),
            final_text: body,
            evaluations: outcome.history,
            cycles: outcome.cycles_run,
            passed: outcome.passed,
            plateau: outcome.plateau_detected,
        });

        state.phase = Phase::Drafting;
        state.iteration += 1;
        write_state(&state, dir)?;
    }

    // ---- Assemble the finished document ----
    let essay_md = assemble_essay(config, &reports);
    write_file(&dir.join("essay.md"), &essay_md)?;

    // ---- Final state ----
    state.plateau_detected = plateau_any;
    let means: Vec<f64> = reports
        .iter()
        .filter_map(|r| r.evaluations.last().map(mean_score))
        .collect();
    if !means.is_empty() {
        state.overall_score = Some(means.iter().sum::<f64>() / means.len() as f64);
    }
    state.phase = Phase::Complete;
    state.iteration = reports.iter().map(|r| r.cycles).sum();
    write_state(&state, dir)?;

    progress(config, "Pipeline complete");

    Ok(PipelineReport {
        project_dir: dir.clone(),
        research_md,
        outline_md,
        sections: reports,
        essay_md,
        state,
        sources,
        plateau_detected: plateau_any,
    })
}

/// Evaluate a single already-drafted section in an existing project (the
/// "autoessay evaluate --section" command).
pub async fn evaluate_project_section<C: LlmClient>(
    llm: &C,
    project_dir: &Path,
    section_id: &str,
    style: &StyleProfile,
) -> CliResult<EvaluationResult> {
    let research = read_file(&project_dir.join("research.md"))?;
    let section_body =
        read_file(&project_dir.join("sections").join(format!("{section_id}.md")))?;
    let section_text = strip_leading_heading(&section_body);
    evaluate_section(llm, &section_text, &research, style, None).await
}

/// Revise a single section in an existing project with extra human feedback
/// folded into the revision brief (the "autoessay revise --section" command).
pub async fn revise_project_section<C: LlmClient>(
    llm: &C,
    project_dir: &Path,
    section_id: &str,
    feedback: &str,
    style: &StyleProfile,
) -> CliResult<SectionReport> {
    let research = read_file(&project_dir.join("research.md"))?;
    let section_path = project_dir.join("sections").join(format!("{section_id}.md"));
    let section_body = read_file(&section_path)?;
    let section_text = strip_leading_heading(&section_body);
    let title = leading_heading(&section_body).unwrap_or_else(|| section_id.to_string());

    let evaluation = evaluate_section(llm, &section_text, &research, style, None).await?;

    // Fold the human feedback into the brief request.
    let mut brief = generate_revision_brief(llm, &section_text, &evaluation, style).await?;
    if !feedback.is_empty() {
        brief.push_str("\n\n## Additional Human Feedback\n\n");
        brief.push_str(feedback);
    }

    // Keep the previous version so `revise` never silently destroys a draft.
    let archive = project_dir
        .join("revisions")
        .join(format!("{section_id}-pre-revise.md"));
    write_file(&archive, &section_body)?;

    let revised = revise_section(llm, &section_text, &brief, &research, style, None).await?;
    let body = format!("## {title}\n\n{revised}");
    write_file(&section_path, &body)?;

    // Refresh state.json for this section.
    let state_path = project_dir.join("state.json");
    if state_path.exists() {
        if let Ok(json) = read_file(&state_path) {
            if let Ok(mut state) = ProjectState::from_json(&json) {
                if let Some(entry) = state.section_mut(section_id) {
                    entry.status = SectionStatus::Drafted;
                    entry.revision_cycles += 1;
                    entry.scores = Scores {
                        accuracy: evaluation.accuracy,
                        coherence: evaluation.coherence,
                        style: evaluation.style,
                        source_integrity: evaluation.source_integrity,
                    };
                    entry.hallucination_flags = evaluation.hallucination_flags.clone();
                }
                write_state(&state, project_dir)?;
            }
        }
    }

    Ok(SectionReport {
        id: section_id.to_string(),
        title,
        final_text: body,
        evaluations: vec![evaluation],
        cycles: 1,
        passed: false,
        plateau: false,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Refuse to run when drafting and evaluation resolve to the same model.
///
/// This is the production form of the anti-self-review rule: with a single
/// provider (OpenRouter), the two roles must still land on different models.
/// Mocks that cannot report routes (offline tests) skip the check.
fn ensure_distinct_models<C: LlmClient>(llm: &C) -> CliResult<()> {
    let drafting = llm.route_for_role("drafting");
    let evaluation = llm.route_for_role("evaluation");
    match (drafting, evaluation) {
        (Some(drafting), Some(evaluation)) if drafting == evaluation => {
            Err(CliError::invalid_input(format!(
                "anti-self-review check failed: drafting and evaluation both resolve to '{}' via '{}'. \
                 Set a different tier/model for the evaluation role in providers.yaml.",
                drafting.model, drafting.provider
            )))
        }
        _ => Ok(()),
    }
}

/// Per-section word budget and audience for prompts.
fn section_brief(
    target_words: u32,
    audience_level: u8,
    section_count: usize,
) -> Option<SectionBrief> {
    if target_words == 0 {
        return None;
    }
    let count = section_count.max(1) as u32;
    let per_section = (target_words / count).max(150);
    Some(SectionBrief {
        target_words: per_section,
        audience_level,
    })
}

/// Join the section files into one markdown document.
fn assemble_essay(config: &PipelineConfig, reports: &[SectionReport]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", config.seed.trim()));
    for report in reports {
        out.push_str(report.final_text.trim_end());
        out.push_str("\n\n");
    }
    out.trim_end().to_string() + "\n"
}

/// Build seed.md from the run configuration.
fn build_seed_md(config: &PipelineConfig, style: &StyleProfile) -> String {
    let mut out = String::new();
    out.push_str("# Seed\n\n");
    out.push_str(&format!("**Topic:** {}\n\n", config.seed));
    if let Some(notes) = &config.notes {
        out.push_str("**User notes:**\n\n");
        out.push_str(notes);
        out.push_str("\n\n");
    }
    out.push_str(&format!(
        "**Style profile:** {} ({})\n\n",
        style.name, style.use_case
    ));
    out.push_str(&format!(
        "**Audience level:** {}/5\n\n",
        config.audience_level
    ));
    out.push_str(&format!("**Target words:** {}\n", config.target_words));
    out
}

fn section_id(n: usize) -> String {
    format!("sec_{n:02}")
}

fn strip_leading_heading(body: &str) -> String {
    let mut lines = body.lines();
    if let Some(first) = lines.next() {
        if first.trim_start().starts_with('#') {
            return lines.collect::<Vec<_>>().join("\n").trim().to_string();
        }
    }
    body.to_string()
}

/// The text of the first heading line, if the document starts with one.
fn leading_heading(body: &str) -> Option<String> {
    let first = body.lines().next()?;
    let trimmed = first.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let title = trimmed.trim_start_matches('#').trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn write_state(state: &ProjectState, dir: &Path) -> CliResult<()> {
    let json = state.to_json().map_err(map_core_err)?;
    write_file(&dir.join("state.json"), &json)
}

fn write_file(path: &Path, contents: &str) -> CliResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

fn read_file(path: &Path) -> CliResult<String> {
    std::fs::read_to_string(path)
        .map_err(|e| CliError::project(format!("failed to read {}: {e}", path.display())))
}

fn progress(config: &PipelineConfig, msg: &str) {
    if config.verbose {
        println!("[autoessay] {msg}");
    }
}

fn map_core_err(e: autoessay_core::CoreError) -> CliError {
    CliError::Project(e.to_string())
}
