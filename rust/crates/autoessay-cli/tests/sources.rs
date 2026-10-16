// Tests are the one place where unwrap/expect/panic! are allowed by the
// project design rule (no unwrap/expect/panic! outside tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-extraction tests (research notes -> sources.json).

use autoessay_cli::sources::{extract_sources, registry_from_json, registry_to_json};

const RESEARCH: &str = "## Remote Work

### Key Claims
- Claim: Productivity rose 13% | Source: src_01 | Confidence: high

### Sources
- [src_01] Stanford WFH Study, Alice Smith, 2023, https://example.com/stanford, Productivity rose 13% across a large sample
- [src_02] Harvard Business Review, Bob Jones, 2024, https://example.com/hbr, Collaboration networks shrank by a quarter
- [src_03] Short Title, 2020";

#[test]
fn extracts_all_source_lines() {
    let registry = extract_sources(RESEARCH);
    assert_eq!(registry.sources.len(), 3);
    assert_eq!(registry.sources[0].id, "src_01");
    assert_eq!(registry.sources[1].id, "src_02");
    assert_eq!(registry.sources[2].id, "src_03");
}

#[test]
fn parses_url_year_and_title() {
    let registry = extract_sources(RESEARCH);
    let s0 = &registry.sources[0];
    assert!(s0.title.contains("Stanford"));
    assert_eq!(s0.year, Some(2023));
    assert_eq!(s0.url.as_deref(), Some("https://example.com/stanford"));
}

#[test]
fn source_without_url_year_still_parses() {
    let registry = extract_sources(RESEARCH);
    let s2 = &registry.sources[2];
    assert_eq!(s2.year, Some(2020));
    assert!(s2.url.is_none());
}

#[test]
fn non_source_lines_are_ignored() {
    let registry = extract_sources("- Claim: something\n- Evidence: else");
    assert!(registry.sources.is_empty());
}

#[test]
fn round_trips_through_json() {
    let registry = extract_sources(RESEARCH);
    let json = registry_to_json(&registry).expect("serialize");
    let back = registry_from_json(&json).expect("deserialize");
    assert_eq!(registry, back);
}

#[test]
fn handles_empty_input() {
    let registry = extract_sources("");
    assert!(registry.sources.is_empty());
}
