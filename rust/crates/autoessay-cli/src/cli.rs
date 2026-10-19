//! CLI command surface (clap derive) and the dispatch that runs each command.
//!
//! The surface matches the CLI sketched in ARCHITECTURE.md:
//!   autoessay run --seed "..." --profile magazine --audience 3 --words 2500
//!   autoessay evaluate --section sec_02 --project-dir DIR
//!   autoessay revise --section sec_02 --feedback "..." --project-dir DIR
//!   autoessay styles

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};

use crate::domains::StyleProfile;
use crate::error::CliResult;
use crate::llm::LlmClient;
use crate::orchestrator::{
    evaluate_project_section, revise_project_section, run_pipeline, PipelineConfig,
};
use crate::phases::ReviseConfig;
use crate::styles::{list_profiles, load_profile};

/// autoessay — autonomous non-fiction essay pipeline.
#[derive(Debug, Parser)]
#[command(name = "autoessay", version, about = "Autonomous non-fiction essay pipeline", long_about = None)]
pub struct Cli {
    /// Path to providers.yaml (overrides AUTOESSAY_CONFIG and config discovery).
    ///
    /// Use this to switch models/providers without rebuilding the binary.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the full pipeline: research -> outline -> draft -> evaluate -> revise.
    Run(RunArgs),
    /// Evaluate one section in an existing project and print the scores.
    Evaluate(EvaluateArgs),
    /// Revise one section in an existing project with optional human feedback.
    Revise(ReviseArgs),
    /// Propose prompt-asset patches from a project's recurring evaluation findings.
    Improve(ImproveArgs),
    /// Run the three-persona reader panel over a finished essay.
    Panel(PanelArgs),
    /// List available style profiles.
    Styles(StylesArgs),
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// The seed topic (1 sentence to 1 paragraph).
    #[arg(long)]
    pub seed: String,

    /// Optional user notes / angles / materials.
    #[arg(long)]
    pub notes: Option<String>,

    /// Style profile name (e.g. magazine, academic).
    #[arg(long, default_value = "magazine")]
    pub profile: String,

    /// Directory to search for style profile YAML files (default config/styles).
    #[arg(long)]
    pub dir: Option<PathBuf>,

    /// Audience level 1-5 (general -> expert).
    #[arg(long, default_value_t = 3)]
    pub audience: u8,

    /// Target length in words.
    #[arg(long, default_value_t = 2500)]
    pub words: u32,

    /// Project output directory (created if absent).
    #[arg(long)]
    pub project_dir: Option<PathBuf>,

    /// Plan only: print what would run without calling any model.
    #[arg(long)]
    pub dry_run: bool,

    /// Maximum revision cycles per section.
    #[arg(long, default_value_t = 3)]
    pub max_cycles: u32,

    /// Skip Crossref source verification (on by default).
    #[arg(long)]
    pub no_verify_sources: bool,

    /// Skip the outline review / regeneration gate (on by default).
    #[arg(long)]
    pub no_outline_review: bool,

    /// Ask a second judge model to score sections and average with the editor.
    #[arg(long)]
    pub double_judge: bool,

    /// Suppress progress output.
    #[arg(long)]
    pub quiet: bool,
}

#[derive(Debug, Args)]
pub struct EvaluateArgs {
    /// Section id, e.g. sec_02.
    #[arg(long)]
    pub section: String,

    /// Project directory.
    #[arg(long)]
    pub project_dir: PathBuf,

    /// Style profile name (defaults to the one recorded in state.json).
    #[arg(long)]
    pub profile: Option<String>,
}

#[derive(Debug, Args)]
pub struct ReviseArgs {
    /// Section id, e.g. sec_02.
    #[arg(long)]
    pub section: String,

    /// Human feedback to fold into the revision brief.
    #[arg(long, default_value = "")]
    pub feedback: String,

    /// Project directory.
    #[arg(long)]
    pub project_dir: PathBuf,

    /// Style profile name (defaults to the one recorded in state.json).
    #[arg(long)]
    pub profile: Option<String>,
}

#[derive(Debug, Args)]
pub struct StylesArgs {
    /// Directory to search for style profiles.
    #[arg(long)]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ImproveArgs {
    /// Project directory produced by `autoessay run`.
    #[arg(long)]
    pub project_dir: PathBuf,

    /// Style profile name (defaults to the one recorded in state.json).
    #[arg(long)]
    pub profile: Option<String>,
}

#[derive(Debug, Args)]
pub struct PanelArgs {
    /// Project directory produced by `autoessay run`.
    #[arg(long)]
    pub project_dir: PathBuf,

    /// Style profile name (defaults to the one recorded in state.json).
    #[arg(long)]
    pub profile: Option<String>,
}

/// Exit code convention: 0 success, 1 error.
pub const EXIT_OK: i32 = 0;
pub const EXIT_ERROR: i32 = 1;

/// Parse argv and run the requested command against an injected LLM client.
/// Returns the process exit code.
pub async fn run_with<C: LlmClient>(cli: &Cli, llm: &C) -> i32 {
    match dispatch(cli, llm).await {
        Ok(()) => EXIT_OK,
        Err(e) => {
            eprintln!("error: {e}");
            EXIT_ERROR
        }
    }
}

async fn dispatch<C: LlmClient>(cli: &Cli, llm: &C) -> CliResult<()> {
    match &cli.command {
        Command::Run(args) => cmd_run(args, llm).await,
        Command::Evaluate(args) => cmd_evaluate(args, llm).await,
        Command::Revise(args) => cmd_revise(args, llm).await,
        Command::Improve(args) => cmd_improve(args, llm).await,
        Command::Panel(args) => cmd_panel(args, llm).await,
        Command::Styles(args) => cmd_styles(args),
    }
}

async fn cmd_run<C: LlmClient>(args: &RunArgs, llm: &C) -> CliResult<()> {
    if !(1..=5).contains(&args.audience) {
        return Err(crate::error::CliError::invalid_input(
            "--audience must be between 1 and 5",
        ));
    }

    let style = load_profile(&args.profile, args.dir.as_deref())?;
    let project_dir = args
        .project_dir
        .clone()
        .unwrap_or_else(|| default_project_dir(&args.seed));

    if args.dry_run {
        println!("[autoessay] dry run — no model calls will be made");
        println!("  seed:          {}", args.seed);
        println!("  profile:       {}", style.name);
        println!("  audience:      {}", args.audience);
        println!("  target words:  {}", args.words);
        println!("  project dir:   {}", project_dir.display());
        println!("  max cycles:    {}", args.max_cycles);
        println!("  verify sources: {}", !args.no_verify_sources);
        println!("  outline review: {}", !args.no_outline_review);
        println!("  second judge:   {}", args.double_judge);
        return Ok(());
    }

    let config = PipelineConfig {
        seed: args.seed.clone(),
        notes: args.notes.clone(),
        profile_name: style.name.clone(),
        audience_level: args.audience,
        target_words: args.words,
        project_dir: project_dir.clone(),
        verbose: !args.quiet,
        verify_sources: !args.no_verify_sources,
        review_outline: !args.no_outline_review,
        double_judge: args.double_judge,
        revise: ReviseConfig {
            max_cycles: args.max_cycles,
            ..ReviseConfig::default()
        },
    };

    llm.reset_usage();
    let report = run_pipeline(llm, &config, &style).await?;

    println!("project: {}", report.project_dir.display());
    println!("essay:   {}", report.project_dir.join("essay.md").display());
    println!(
        "sections: {} ({} passed)",
        report.sections.len(),
        report.sections.iter().filter(|s| s.passed).count()
    );
    if let Some(score) = report.state.overall_score {
        println!("overall score: {score:.2}");
    }
    let unsupported_claims: usize = report
        .sections
        .iter()
        .filter_map(|s| s.evaluations.last())
        .map(|e| e.claim_counts().2)
        .sum();
    if unsupported_claims > 0 {
        println!("unsupported claims: {unsupported_claims}");
    }
    println!("words: {} (target {})", report.word_count, args.words);
    println!("style: {}", report.style_metrics.summary());
    if report.verification.total > 0 {
        println!("sources: {}", report.verification.summary_line());
    }
    if let Some(review) = &report.outline_review {
        if !review.passed() {
            println!("outline review: weak — {}", review.issue_summary());
        }
    }
    println!("usage: {}", report.usage.summary());
    if report.plateau_detected {
        println!("note: score plateau detected in at least one section");
    }
    Ok(())
}

async fn cmd_evaluate<C: LlmClient>(args: &EvaluateArgs, llm: &C) -> CliResult<()> {
    let style = resolve_style(args.profile.as_deref(), &args.project_dir)?;
    let result =
        evaluate_project_section(llm, &args.project_dir, &args.section, &style).await?;

    println!("section:   {}", args.section);
    println!("accuracy:  {:.1}", result.accuracy);
    println!("coherence: {:.1}", result.coherence);
    println!("style:     {:.1}", result.style);
    println!("sources:   {:.1}", result.source_integrity);
    println!("overall:   {}", result.overall);
    if !result.hallucination_flags.is_empty() {
        println!("hallucination flags:");
        for flag in &result.hallucination_flags {
            println!("  [{:?}] {} — {}", flag.severity, flag.claim, flag.note);
        }
    }
    if !result.notes.is_empty() {
        println!("notes: {}", result.notes);
    }
    Ok(())
}

async fn cmd_revise<C: LlmClient>(args: &ReviseArgs, llm: &C) -> CliResult<()> {
    let style = resolve_style(args.profile.as_deref(), &args.project_dir)?;
    let report = revise_project_section(
        llm,
        &args.project_dir,
        &args.section,
        &args.feedback,
        &style,
    )
    .await?;
    println!("revised section: {}", report.id);
    println!("{}", report.final_text);
    Ok(())
}

fn cmd_styles(args: &StylesArgs) -> CliResult<()> {
    let names = list_profiles(args.dir.as_deref());
    if names.is_empty() {
        println!("no style profiles found");
        return Ok(());
    }
    for name in names {
        match load_profile(&name, args.dir.as_deref()) {
            Ok(profile) => println!("{}\t{}", profile.name, profile.use_case),
            Err(e) => println!("{name}\t<unreadable: {e}>"),
        }
    }
    Ok(())
}

/// Resolve a style profile, preferring the explicit flag, then state.json.
fn resolve_style(explicit: Option<&str>, project_dir: &Path) -> CliResult<StyleProfile> {
    if let Some(name) = explicit {
        return load_profile(name, None);
    }
    let state_path = project_dir.join("state.json");
    if let Ok(json) = std::fs::read_to_string(&state_path) {
        if let Ok(state) = crate::domains::ProjectState::from_json(&json) {
            return load_profile(&state.style_profile, None);
        }
    }
    load_profile("magazine", None)
}

fn default_project_dir(seed: &str) -> PathBuf {
    let slug: String = seed
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    let name = if slug.is_empty() {
        "project".to_string()
    } else {
        slug
    };
    PathBuf::from("projects").join(name)
}
