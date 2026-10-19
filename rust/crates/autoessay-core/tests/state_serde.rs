//! Tests: state.json / sources.json round-trips and exact field names.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


use autoessay_core::eval::{HallucinationFlag, Severity};
use autoessay_core::state::{
    Phase, ProjectState, Scores, SectionState, SectionStatus, Source, SourcesFile,
};

#[test]
fn project_state_round_trips_with_exact_field_names() {
    let state = ProjectState {
        phase: Phase::Drafting,
        iteration: 3,
        style_profile: "magazine".to_string(),
        audience_level: 3,
        target_words: 3000,
        sections: vec![SectionState {
            id: "sec_01".to_string(),
            title: "Introduction".to_string(),
            status: SectionStatus::Complete,
            scores: Scores {
                accuracy: 8.2,
                coherence: 7.5,
                style: 8.9,
                source_integrity: 9.1,
            },
            revision_cycles: 2,
            hallucination_flags: vec![HallucinationFlag {
                claim: "made up stat".to_string(),
                severity: Severity::Minor,
                note: "needs a source".to_string(),
            }],
            claims: vec![],
            style_metrics: None,
            last_notes: String::new(),
        }],
        overall_score: None,
        word_count: None,
        plateau_detected: false,
    };

    let json = match state.to_json() {
        Ok(j) => j,
        Err(e) => {
            panic!("serialize failed: {e}");
        }
    };

    // Exact field names from ARCHITECTURE.md must appear verbatim.
    assert!(json.contains("\"phase\""));
    assert!(json.contains("\"iteration\""));
    assert!(json.contains("\"style_profile\""));
    assert!(json.contains("\"audience_level\""));
    assert!(json.contains("\"target_words\""));
    assert!(json.contains("\"sections\""));
    assert!(json.contains("\"overall_score\""));
    assert!(json.contains("\"plateau_detected\""));
    assert!(json.contains("\"revision_cycles\""));
    assert!(json.contains("\"hallucination_flags\""));
    assert!(json.contains("\"source_integrity\""));

    // Round-trip.
    let back = match ProjectState::from_json(&json) {
        Ok(b) => b,
        Err(e) => {
            panic!("deserialize failed: {e}");
        }
    };
    assert_eq!(back, state);
    assert_eq!(back.phase, Phase::Drafting);
    assert_eq!(back.sections[0].revision_cycles, 2);
    assert_eq!(back.sections[0].hallucination_flags.len(), 1);
    assert_eq!(back.sections[0].hallucination_flags[0].severity, Severity::Minor);
}

#[test]
fn project_state_reads_the_documented_example_verbatim() {
    let raw = r#"{
      "phase": "drafting",
      "iteration": 3,
      "style_profile": "magazine",
      "audience_level": 3,
      "target_words": 3000,
      "sections": [
        {
          "id": "sec_01",
          "title": "Introduction",
          "status": "complete",
          "scores": {
            "accuracy": 8.2,
            "coherence": 7.5,
            "style": 8.9,
            "source_integrity": 9.1
          },
          "revision_cycles": 2,
          "hallucination_flags": []
        }
      ],
      "overall_score": null,
      "plateau_detected": false
    }"#;

    let state = match ProjectState::from_json(raw) {
        Ok(s) => s,
        Err(e) => {
            panic!("documented example failed to parse: {e}");
        }
    };
    assert_eq!(state.phase, Phase::Drafting);
    assert_eq!(state.iteration, 3);
    assert_eq!(state.style_profile, "magazine");
    assert_eq!(state.audience_level, 3);
    assert_eq!(state.target_words, 3000);
    assert_eq!(state.sections.len(), 1);
    assert_eq!(state.sections[0].id, "sec_01");
    assert_eq!(state.sections[0].title, "Introduction");
    assert_eq!(state.sections[0].status, SectionStatus::Complete);
    assert_eq!(state.sections[0].revision_cycles, 2);
    assert!(state.sections[0].hallucination_flags.is_empty());
    assert!(state.sections[0].claims.is_empty());
    assert!(state.sections[0].style_metrics.is_none());
    assert_eq!(state.overall_score, None);
    assert_eq!(state.word_count, None);
    assert!(!state.plateau_detected);
}

#[test]
fn sources_file_round_trips_with_type_field() {
    let sources = SourcesFile {
        sources: vec![Source {
            id: "src_01".to_string(),
            source_type: "academic".to_string(),
            title: "Title of the paper".to_string(),
            authors: vec!["Author Name".to_string()],
            year: Some(2025),
            doi: Some("10.1234/example".to_string()),
            url: Some("https://example.com".to_string()),
            key_quote: Some("The relevant excerpt".to_string()),
            embedding_vector: Some(vec![0.123, 0.456]),
            verified: true,
            verification_source: Some("crossref".to_string()),
            verification_note: Some("title match 0.94".to_string()),
        }],
    };

    let json = match sources.to_json() {
        Ok(j) => j,
        Err(e) => {
            panic!("serialize failed: {e}");
        }
    };
    // The Rust field is source_type but the JSON key is literally "type".
    assert!(json.contains("\"type\": \"academic\""), "json was: {json}");
    assert!(json.contains("\"key_quote\""));
    assert!(json.contains("\"embedding_vector\""));
    assert!(json.contains("\"verification_source\""));

    let back = match SourcesFile::from_json(&json) {
        Ok(b) => b,
        Err(e) => {
            panic!("deserialize failed: {e}");
        }
    };
    assert_eq!(back, sources);
    assert_eq!(back.sources[0].source_type, "academic");
}

#[test]
fn section_state_defaults_revision_cycles_to_zero() {
    let raw = r#"{
      "id": "sec_02",
      "title": "Body",
      "status": "drafted"
    }"#;
    let s = match serde_json::from_str::<SectionState>(raw) {
        Ok(s) => s,
        Err(e) => {
            panic!("parse failed: {e}");
        }
    };
    assert_eq!(s.revision_cycles, 0);
    assert_eq!(s.status, SectionStatus::Drafted);
    assert!(s.hallucination_flags.is_empty());
    assert!(s.claims.is_empty());
    assert!(s.style_metrics.is_none());
    assert_eq!(s.scores, Scores::default());
}
