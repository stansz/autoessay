//! Tests for the out-of-loop review phases (improve / panel).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use autoessay_cli::llm::MockLlm;
use autoessay_cli::phases::{
    propose_prompt_improvements, render_panel, run_reader_panel, PanelReview,
};
use autoessay_cli::styles::load_profile;

fn magazine() -> autoessay_core::StyleProfile {
    load_profile("magazine", None).expect("magazine profile")
}

#[tokio::test]
async fn prompt_improvement_uses_revision_brief_role_and_project_context() {
    let mock = MockLlm::always("## Recurring failures\n- weak ledes\n\n## Proposed patches\n1. ...");
    let context = "# Project context\n\nstate.json: accuracy 6.5, flags: weak lede";
    let out = propose_prompt_improvements(&mock, context, &magazine())
        .await
        .expect("improve");
    assert!(out.contains("Proposed patches"));
    let request = mock.last_request().expect("request");
    assert_eq!(request.role, "revision_brief");
    let text = request.all_text();
    assert!(text.contains("state.json: accuracy 6.5"));
    assert!(text.contains("program.md"));
    assert!(text.contains("ANTI-SLOP.md"));
    assert!(text.contains("magazine"));
}

#[tokio::test]
async fn reader_panel_runs_three_personas() {
    let mock = MockLlm::sequence(vec![
        "Expert: solid on evidence, thin on counterarguments.".to_string(),
        "Reader: clear, but paragraph 3 lost me.".to_string(),
        "Editor: cut the second example in section 2.".to_string(),
    ]);
    let reviews = run_reader_panel(&mock, "# Essay\n\nBody text.", &magazine())
        .await
        .expect("panel");
    assert_eq!(reviews.len(), 3);
    assert_eq!(reviews[0].persona, "Domain expert");
    assert_eq!(reviews[1].persona, "General reader");
    assert_eq!(reviews[2].persona, "Editor");
    assert_eq!(mock.call_count(), 3);
    for request in mock.requests() {
        assert_eq!(request.role, "reader_panel");
        assert!(request.all_text().contains("Body text"));
    }
}

#[test]
fn render_panel_emits_one_section_per_persona() {
    let reviews = vec![
        PanelReview {
            persona: "Domain expert".to_string(),
            review: "Solid.".to_string(),
        },
        PanelReview {
            persona: "Editor".to_string(),
            review: "Tighten.".to_string(),
        },
    ];
    let markdown = render_panel(&reviews);
    assert!(markdown.starts_with("# Reader panel"));
    assert!(markdown.contains("## Domain expert"));
    assert!(markdown.contains("## Editor"));
    assert!(markdown.contains("Tighten."));
}
