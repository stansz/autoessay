// Tests are the one place where unwrap/expect/panic! are allowed by the
// project design rule (no unwrap/expect/panic! outside tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CLI surface tests: clap parsing, --help, styles listing, and dry-run.
//! These run against a MockLlm so no command touches the network.

use clap::Parser;

use autoessay_cli::cli::{run_with, Cli, Command};
use autoessay_cli::llm::MockLlm;

#[test]
fn parses_run_command() {
    let cli = Cli::try_parse_from([
        "autoessay",
        "run",
        "--seed",
        "climate policy",
        "--profile",
        "magazine",
        "--audience",
        "3",
        "--words",
        "2500",
    ])
    .expect("parse run");
    match cli.command {
        Command::Run(args) => {
            assert_eq!(args.seed, "climate policy");
            assert_eq!(args.profile, "magazine");
            assert_eq!(args.audience, 3);
            assert_eq!(args.words, 2500);
            assert!(!args.dry_run);
        }
        _ => panic!("expected Run"),
    }
}

#[test]
fn parses_global_config_override() {
    let cli = Cli::try_parse_from(["autoessay", "--config", "/tmp/providers.yaml", "styles"])
        .expect("parse with --config");
    assert_eq!(
        cli.config,
        Some(std::path::PathBuf::from("/tmp/providers.yaml"))
    );

    // global flags are accepted after the subcommand too
    let cli = Cli::try_parse_from(["autoessay", "styles", "--config", "/tmp/p.yaml"])
        .expect("global flag after subcommand");
    assert_eq!(cli.config, Some(std::path::PathBuf::from("/tmp/p.yaml")));
}

#[test]
fn parses_evaluate_command() {
    let cli = Cli::try_parse_from([
        "autoessay",
        "evaluate",
        "--section",
        "sec_02",
        "--project-dir",
        "/tmp/proj",
    ])
    .expect("parse evaluate");
    match cli.command {
        Command::Evaluate(args) => {
            assert_eq!(args.section, "sec_02");
            assert_eq!(args.project_dir, std::path::PathBuf::from("/tmp/proj"));
        }
        _ => panic!("expected Evaluate"),
    }
}

#[test]
fn parses_revise_command_with_feedback() {
    let cli = Cli::try_parse_from([
        "autoessay",
        "revise",
        "--section",
        "sec_02",
        "--feedback",
        "weaker than sec_01",
        "--project-dir",
        "/tmp/proj",
    ])
    .expect("parse revise");
    match cli.command {
        Command::Revise(args) => {
            assert_eq!(args.feedback, "weaker than sec_01");
        }
        _ => panic!("expected Revise"),
    }
}

#[test]
fn parses_styles_command() {
    let cli = Cli::try_parse_from(["autoessay", "styles"]).expect("parse styles");
    assert!(matches!(cli.command, Command::Styles(_)));
}

#[tokio::test]
async fn rejects_bad_audience_on_run() {
    let mock = MockLlm::always("unused");
    let cli = Cli::try_parse_from([
        "autoessay",
        "run",
        "--seed",
        "x",
        "--audience",
        "9",
        "--dry-run",
    ])
    .expect("parse");
    let code = run_with(&cli, &mock).await;
    // audience 9 is out of range -> error exit code 1
    assert_eq!(code, 1);
}

#[test]
fn help_succeeds_and_lists_subcommands() {
    let result = Cli::try_parse_from(["autoessay", "--help"]);
    assert!(result.is_err(), "--help returns a clap DisplayHelp error");
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("run"));
    assert!(msg.contains("evaluate"));
    assert!(msg.contains("revise"));
    assert!(msg.contains("styles"));
}

#[tokio::test]
async fn styles_lists_profiles_without_llm() {
    let mock = MockLlm::always("unused");
    let cli = Cli::try_parse_from(["autoessay", "styles"]).expect("parse");
    let code = run_with(&cli, &mock).await;
    assert_eq!(code, 0);
    assert_eq!(mock.call_count(), 0, "styles must not call the LLM");
}

#[tokio::test]
async fn dry_run_makes_no_llm_calls() {
    let mock = MockLlm::always("unused");
    let cli = Cli::try_parse_from(["autoessay", "run", "--seed", "climate policy", "--dry-run"])
        .expect("parse");
    let code = run_with(&cli, &mock).await;
    assert_eq!(code, 0);
    assert_eq!(mock.call_count(), 0, "dry-run must not call the LLM");
}

// ═══════════════════════════════════════════════════════════════════════════
// New pipeline flags and review commands
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parses_pipeline_feature_flags() {
    let cli = Cli::try_parse_from([
        "autoessay",
        "run",
        "--seed",
        "x",
        "--no-verify-sources",
        "--no-outline-review",
        "--double-judge",
    ])
    .expect("parse run flags");
    match cli.command {
        Command::Run(args) => {
            assert!(args.no_verify_sources);
            assert!(args.no_outline_review);
            assert!(args.double_judge);
        }
        _ => panic!("expected Run"),
    }
}

#[test]
fn parses_panel_and_improve_commands() {
    let cli = Cli::try_parse_from(["autoessay", "panel", "--project-dir", "/tmp/proj"])
        .expect("parse panel");
    assert!(matches!(cli.command, Command::Panel(_)));

    let cli = Cli::try_parse_from(["autoessay", "improve", "--project-dir", "/tmp/proj"])
        .expect("parse improve");
    assert!(matches!(cli.command, Command::Improve(_)));
}

fn temp_project() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "autoessay-cli-cmdtest-{}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create project dir");
    std::fs::write(
        dir.join("essay.md"),
        "# Test essay\n\nA short body with a claim and a conclusion.",
    )
    .expect("write essay");
    dir
}

#[tokio::test]
async fn panel_command_writes_panel_md() {
    let dir = temp_project();
    let mock = MockLlm::sequence(vec![
        "Domain expert: fine.".to_string(),
        "General reader: fine.".to_string(),
        "Editor: fine.".to_string(),
    ]);
    let cli = Cli::try_parse_from([
        "autoessay",
        "panel",
        "--project-dir",
        dir.to_str().unwrap(),
    ])
    .expect("parse");
    let code = run_with(&cli, &mock).await;
    assert_eq!(code, 0);
    let panel = std::fs::read_to_string(dir.join("panel.md")).expect("panel.md");
    assert!(panel.contains("# Reader panel"));
    assert!(panel.contains("## Domain expert"));
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn improve_command_writes_improvements_md() {
    let dir = temp_project();
    std::fs::write(
        dir.join("state.json"),
        r#"{"phase":"complete","iteration":1,"style_profile":"magazine","audience_level":3,"target_words":1000,"sections":[],"overall_score":7.0,"plateau_detected":false}"#,
    )
    .expect("write state");
    let mock = MockLlm::always("## Proposed patches\n\n1. Tighten the lede rule.");
    let cli = Cli::try_parse_from([
        "autoessay",
        "improve",
        "--project-dir",
        dir.to_str().unwrap(),
    ])
    .expect("parse");
    let code = run_with(&cli, &mock).await;
    assert_eq!(code, 0);
    let improvements =
        std::fs::read_to_string(dir.join("improvements.md")).expect("improvements.md");
    assert!(improvements.contains("Proposed patches"));
    std::fs::remove_dir_all(&dir).ok();
}
