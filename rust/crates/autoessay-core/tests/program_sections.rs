//! Tests: program.md section extraction.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


use autoessay_core::program::load_program_section;

const FIXTURE: &str = "# Program\n\nIntro text.\n\n## Phase 1: Research (gen_research.py)\n\nDo the research.\nLine two.\n\n## Phase 2: Outline (gen_outline.py)\n\nMake an outline.\n\n## Phase 3: Evaluation (evaluate.py)\n\nScore it.\n";

/// The real program.md headings wrap the script name in backticks. Lookups use
/// the unquoted form, so matching must ignore markdown formatting.
const BACKTICKED_FIXTURE: &str = "# Program\n\n## Phase 1: Research (`gen_research.py`)\n\nDo the research.\n\n## Phase 2: Drafting (`draft_section.py`)\n\nWrite well.\n";

#[test]
fn extracts_middle_section_up_to_next_heading() {
    let got = load_program_section(FIXTURE, "Phase 1: Research (gen_research.py)");
    assert_eq!(got, Some("Do the research.\nLine two.".to_string()));
}

#[test]
fn extracts_first_section() {
    let got = load_program_section(FIXTURE, "Phase 1: Research (gen_research.py)");
    assert!(got.is_some());
    let body = got.unwrap_or_default();
    assert!(body.contains("Do the research."));
    assert!(!body.contains("##"));
}

#[test]
fn extracts_last_section_to_end_of_doc() {
    let got = load_program_section(FIXTURE, "Phase 3: Evaluation (evaluate.py)");
    assert_eq!(got, Some("Score it.".to_string()));
}

#[test]
fn missing_heading_returns_none() {
    let got = load_program_section(FIXTURE, "Phase 9: Nonexistent");
    assert!(got.is_none());
}

#[test]
fn empty_document_returns_none() {
    assert!(load_program_section("", "Anything").is_none());
}

#[test]
fn does_not_cross_into_next_heading() {
    let got = load_program_section(FIXTURE, "Phase 2: Outline (gen_outline.py)");
    let body = got.unwrap_or_default();
    assert!(body.contains("Make an outline."));
    assert!(!body.contains("Score it."));
}

#[test]
fn matches_heading_whose_script_name_is_backticked() {
    let got = load_program_section(BACKTICKED_FIXTURE, "Phase 1: Research (gen_research.py)");
    assert_eq!(got, Some("Do the research.".to_string()));
}

#[test]
fn matches_backticked_heading_case_insensitively() {
    let got = load_program_section(
        BACKTICKED_FIXTURE,
        "phase 2: drafting (draft_section.py)",
    );
    assert_eq!(got, Some("Write well.".to_string()));
}

#[test]
fn does_not_match_a_level_three_heading() {
    let doc = "# Program\n\n### Phase 1: Research (gen_research.py)\n\nNot a section.\n";
    assert!(load_program_section(doc, "Phase 1: Research (gen_research.py)").is_none());
}
