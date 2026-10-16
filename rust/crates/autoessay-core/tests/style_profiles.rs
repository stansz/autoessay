//! Tests: parse all bundled style YAMLs and assert their behaviour fields.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


use autoessay_core::style::{
    builtin_styles, load_profile_bytes, CitationVerification, Contractions, FirstPerson,
    HallucinationGate, PassiveVoice, StyleProfile,
};

#[test]
fn all_five_bundled_profiles_parse() {
    let profiles = match builtin_styles() {
        Ok(p) => p,
        Err(e) => {
            panic!("builtin profiles failed to parse: {e}");
        }
    };
    assert_eq!(profiles.len(), 5);
    let mut names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        vec!["academic", "magazine", "personal-essay", "policy-brief", "technical"]
    );
}

#[test]
fn academic_profile_is_strict_and_cited() {
    let yaml = match load_profile_bytes("academic") {
        Some(y) => y,
        None => {
            panic!("academic profile missing from bundle");
        }
    };
    let p = match StyleProfile::from_yaml_str(yaml) {
        Ok(p) => p,
        Err(e) => {
            panic!("parse failed: {e}");
        }
    };

    assert_eq!(p.name, "academic");
    assert_eq!(p.citation_verification, CitationVerification::Crossref);
    assert!(p.source_deduplication);
    assert_eq!(p.hallucination_gate, HallucinationGate::Strict);
    assert!(p.require_citation_per_claim);
    assert!(!p.allow_unsourced_opinion);

    assert_eq!(p.accuracy_threshold, Some(8.5));
    assert_eq!(p.coherence_threshold, 7.5);
    assert_eq!(p.style_threshold, 7.5);
    assert_eq!(p.source_integrity_threshold, Some(9.0));

    assert_eq!(p.first_person, FirstPerson::False);
    assert_eq!(p.passive_voice, PassiveVoice::Allowed);
    assert_eq!(p.contractions, Contractions::False);

    assert!(p.hallucination_enabled());
    assert!(p.citations_required());
    assert!(!p.prompt_instructions.is_empty());
}

#[test]
fn personal_essay_gate_is_off_and_accuracy_unscored() {
    let yaml = match load_profile_bytes("personal-essay") {
        Some(y) => y,
        None => {
            panic!("personal-essay profile missing from bundle");
        }
    };
    let p = match StyleProfile::from_yaml_str(yaml) {
        Ok(p) => p,
        Err(e) => {
            panic!("parse failed: {e}");
        }
    };

    assert_eq!(p.name, "personal-essay");
    assert_eq!(p.citation_verification, CitationVerification::Off);
    assert!(!p.source_deduplication);
    assert_eq!(p.hallucination_gate, HallucinationGate::Off);
    assert!(!p.require_citation_per_claim);
    assert!(p.allow_unsourced_opinion);

    assert_eq!(p.accuracy_threshold, None);
    assert_eq!(p.source_integrity_threshold, None);
    assert_eq!(p.coherence_threshold, 6.0);
    assert_eq!(p.style_threshold, 9.0);

    assert_eq!(p.first_person, FirstPerson::True);
    assert_eq!(p.passive_voice, PassiveVoice::Discouraged);
    assert_eq!(p.contractions, Contractions::True);

    assert!(!p.hallucination_enabled());
    assert!(!p.citations_required());
}

#[test]
fn magazine_uses_lenient_gate_and_situational_voice() {
    let yaml = match load_profile_bytes("magazine") {
        Some(y) => y,
        None => {
            panic!("magazine profile missing from bundle");
        }
    };
    let p = match StyleProfile::from_yaml_str(yaml) {
        Ok(p) => p,
        Err(e) => {
            panic!("parse failed: {e}");
        }
    };

    assert_eq!(p.hallucination_gate, HallucinationGate::Lenient);
    assert_eq!(p.first_person, FirstPerson::Situational);
    assert_eq!(p.contractions, Contractions::Situational);
    assert_eq!(p.style_threshold, 8.5);
    assert_eq!(p.source_integrity_threshold, Some(6.0));
    assert!(p.hallucination_enabled());
    assert!(!p.citations_required());
}

#[test]
fn technical_and_policy_brief_are_strict() {
    for name in ["technical", "policy-brief"] {
        let yaml = match load_profile_bytes(name) {
            Some(y) => y,
            None => {
                panic!("profile {name} missing from bundle");
            }
        };
        let p = match StyleProfile::from_yaml_str(yaml) {
            Ok(p) => p,
            Err(e) => {
                panic!("parse failed for {name}: {e}");
            }
        };
        assert_eq!(p.hallucination_gate, HallucinationGate::Strict, "{name}");
        assert!(p.citations_required(), "{name}");
        assert!(p.hallucination_enabled(), "{name}");
    }
}

#[test]
fn unknown_profile_name_returns_error() {
    let err = StyleProfile::from_yaml_str("thresholds: {}\n");
    assert!(err.is_err(), "missing name field should error");
}
