//! Source verification tests — metadata lookups are faked, so offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use autoessay_cli::verify::{
    title_similarity, verification_context, verify_sources, MetadataResolver, VerificationOutcome,
    WorkMeta,
};
use autoessay_core::{Source, SourcesFile};

/// A resolver with canned DOI and title responses.
struct FakeResolver {
    doi: HashMap<String, Result<Option<WorkMeta>, String>>,
    title: HashMap<String, Result<Option<WorkMeta>, String>>,
}

impl FakeResolver {
    fn new() -> Self {
        FakeResolver {
            doi: HashMap::new(),
            title: HashMap::new(),
        }
    }

    fn with_doi(mut self, doi: &str, work: Option<WorkMeta>) -> Self {
        self.doi.insert(doi.to_string(), Ok(work));
        self
    }

    fn with_doi_error(mut self, doi: &str, error: &str) -> Self {
        self.doi.insert(doi.to_string(), Err(error.to_string()));
        self
    }

    fn with_title(mut self, title: &str, work: Option<WorkMeta>) -> Self {
        self.title.insert(title.to_string(), Ok(work));
        self
    }
}

impl MetadataResolver for FakeResolver {
    async fn resolve_doi(&self, doi: &str) -> Result<Option<WorkMeta>, String> {
        match self.doi.get(doi) {
            Some(result) => result.clone(),
            None => Ok(None),
        }
    }

    async fn search_title(&self, title: &str) -> Result<Option<WorkMeta>, String> {
        match self.title.get(title) {
            Some(result) => result.clone(),
            None => Ok(None),
        }
    }
}

fn work(title: &str) -> Option<WorkMeta> {
    Some(WorkMeta {
        title: title.to_string(),
        year: Some(2024),
    })
}

fn source(id: &str, title: &str, doi: Option<&str>) -> Source {
    Source {
        id: id.to_string(),
        title: title.to_string(),
        doi: doi.map(str::to_string),
        ..Source::default()
    }
}

#[tokio::test]
async fn matching_doi_is_verified() {
    let resolver = FakeResolver::new().with_doi(
        "10.1234/abc",
        work("Remote work and company culture: evidence from 2020-2024"),
    );
    let mut sources = SourcesFile {
        sources: vec![source(
            "src_01",
            "Remote Work and Company Culture: Evidence from 2020-2024",
            Some("10.1234/abc"),
        )],
    };

    let summary = verify_sources(&mut sources, &resolver).await;
    assert_eq!(summary.verified, 1);
    assert_eq!(summary.total, 1);
    assert!(sources.sources[0].verified);
    assert_eq!(
        sources.sources[0].verification_source.as_deref(),
        Some("verified")
    );
}

#[tokio::test]
async fn doi_resolving_to_a_different_paper_is_a_mismatch() {
    let resolver = FakeResolver::new().with_doi("10.1234/abc", work("Deep learning for chess engines"));
    let mut sources = SourcesFile {
        sources: vec![source(
            "src_01",
            "Remote Work and Company Culture: Evidence from 2020-2024",
            Some("10.1234/abc"),
        )],
    };

    let summary = verify_sources(&mut sources, &resolver).await;
    assert_eq!(summary.mismatched, 1);
    assert!(!sources.sources[0].verified);
    assert_eq!(
        sources.sources[0].verification_source.as_deref(),
        Some("mismatch")
    );
    assert!(sources.sources[0]
        .verification_note
        .as_deref()
        .unwrap()
        .contains("different work"));
}

#[tokio::test]
async fn missing_doi_is_not_found() {
    let resolver = FakeResolver::new().with_doi("10.1234/missing", None);
    let mut sources = SourcesFile {
        sources: vec![source(
            "src_01",
            "A paper that does not exist anywhere",
            Some("10.1234/missing"),
        )],
    };

    let summary = verify_sources(&mut sources, &resolver).await;
    assert_eq!(summary.not_found, 1);
    assert_eq!(
        sources.sources[0].verification_source.as_deref(),
        Some("not_found")
    );
}

#[tokio::test]
async fn title_search_can_verify_without_a_doi() {
    let resolver = FakeResolver::new().with_title(
        "The Hybrid Trap: Why Hybrid Offices Fail",
        work("The Hybrid Trap: Why Hybrid Offices Fail"),
    );
    let mut sources = SourcesFile {
        sources: vec![source(
            "src_02",
            "The Hybrid Trap: Why Hybrid Offices Fail",
            None,
        )],
    };

    let summary = verify_sources(&mut sources, &resolver).await;
    assert_eq!(summary.verified, 1);
    assert!(sources.sources[0].verified);
    assert_eq!(
        sources.sources[0].verification_source.as_deref(),
        Some("verified")
    );
}

#[tokio::test]
async fn title_with_no_match_is_no_identifier() {
    let resolver = FakeResolver::new();
    let mut sources = SourcesFile {
        sources: vec![source(
            "src_03",
            "A Completely Unfindable Essay About Nothing",
            None,
        )],
    };

    let summary = verify_sources(&mut sources, &resolver).await;
    assert_eq!(summary.no_identifier, 1);
    assert_eq!(
        sources.sources[0].verification_source.as_deref(),
        Some("no_identifier")
    );
}

#[tokio::test]
async fn resolver_error_is_recorded_not_fatal() {
    let resolver = FakeResolver::new().with_doi_error("10.1234/boom", "network down");
    let mut sources = SourcesFile {
        sources: vec![
            source("src_01", "Some Real Paper Title Here", Some("10.1234/boom")),
            source("src_02", "Another Real Paper Title", None),
        ],
    };

    let summary = verify_sources(&mut sources, &resolver).await;
    assert_eq!(summary.errors, 1);
    assert_eq!(summary.total, 2);
    assert_eq!(
        sources.sources[0].verification_source.as_deref(),
        Some("error")
    );
}

#[test]
fn title_similarity_ignores_case_and_punctuation() {
    let a = "Remote Work and Company Culture: Evidence from 2020-2024";
    let b = "remote work and company culture evidence from 2020 2024";
    assert!(title_similarity(a, b) > 0.95, "similarity={}", title_similarity(a, b));
    let unrelated = title_similarity(a, "Quantum error correction with surface codes");
    assert!(unrelated < 0.2, "similarity={unrelated}");
}

#[test]
fn verification_context_lists_statuses() {
    let sources = SourcesFile {
        sources: vec![
            Source {
                verification_source: Some("verified".to_string()),
                verification_note: Some("DOI match".to_string()),
                ..source("src_01", "Verified Paper Title", None)
            },
            Source {
                verification_source: Some("mismatch".to_string()),
                verification_note: Some("resolves elsewhere".to_string()),
                ..source("src_02", "Mismatched Paper Title", None)
            },
        ],
    };
    let context = verification_context(&sources);
    assert!(context.contains("[src_01]"));
    assert!(context.contains("verified: DOI match"));
    assert!(context.contains("[src_02]"));
    assert!(context.contains("mismatch: resolves elsewhere"));
    assert!(context.contains("weak evidence"));
}

#[test]
fn outcome_labels_are_stable() {
    assert_eq!(VerificationOutcome::Verified.label(), "verified");
    assert_eq!(VerificationOutcome::Mismatch.label(), "mismatch");
    assert_eq!(VerificationOutcome::NotFound.label(), "not_found");
    assert_eq!(VerificationOutcome::NoIdentifier.label(), "no_identifier");
    assert_eq!(VerificationOutcome::Error.label(), "error");
}
