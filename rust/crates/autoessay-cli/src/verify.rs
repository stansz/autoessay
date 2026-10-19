//! Source verification — check that cited works actually exist and match.
//!
//! The evaluator can only check a claim against the model's own research notes;
//! it cannot know whether a cited paper exists. This module adds the independent
//! leg: resolve a source's DOI (or search by title) against Crossref and compare
//! the returned title with the source's claimed title.
//!
//! The dangerous failure is not "the DOI does not resolve" but "the DOI resolves
//! to a different paper" (the CiteAudit finding). So a mismatch is reported
//! separately from a not-found, and a source is only `verified = true` when the
//! titles actually agree.
//!
//! HTTP is behind [`MetadataResolver`] so the decision logic is unit-tested
//! offline; [`CrossrefResolver`] is the production implementation.

use std::collections::HashSet;
use std::time::Duration;

use autoessay_core::{Source, SourcesFile};

/// Minimum title similarity for a match.
pub const TITLE_MATCH_THRESHOLD: f64 = 0.6;

/// The verdict for one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationOutcome {
    /// Resolves and the title matches.
    Verified,
    /// Resolves, but to a different work.
    Mismatch,
    /// A well-formed identifier that Crossref does not know.
    NotFound,
    /// No DOI and no title match could be found.
    NoIdentifier,
    /// The lookup itself failed (network, rate limit, bad response).
    Error,
}

impl VerificationOutcome {
    /// Config-file / state.json label.
    pub fn label(self) -> &'static str {
        match self {
            VerificationOutcome::Verified => "verified",
            VerificationOutcome::Mismatch => "mismatch",
            VerificationOutcome::NotFound => "not_found",
            VerificationOutcome::NoIdentifier => "no_identifier",
            VerificationOutcome::Error => "error",
        }
    }
}

/// Counts from a verification pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VerificationSummary {
    /// Sources examined.
    pub total: usize,
    /// Verified matches.
    pub verified: usize,
    /// DOI resolved to a different work.
    pub mismatched: usize,
    /// Identifier does not resolve.
    pub not_found: usize,
    /// No identifier and no title match.
    pub no_identifier: usize,
    /// Lookup errors (network and friends).
    pub errors: usize,
}

impl VerificationSummary {
    /// One-line summary for CLI output.
    pub fn summary_line(&self) -> String {
        format!(
            "{} sources: {} verified, {} mismatch, {} not found, {} no identifier, {} errors",
            self.total,
            self.verified,
            self.mismatched,
            self.not_found,
            self.no_identifier,
            self.errors
        )
    }

    /// Fraction of sources that verified.
    pub fn verified_ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.verified as f64 / self.total as f64
        }
    }

    fn record(&mut self, outcome: VerificationOutcome) {
        match outcome {
            VerificationOutcome::Verified => self.verified += 1,
            VerificationOutcome::Mismatch => self.mismatched += 1,
            VerificationOutcome::NotFound => self.not_found += 1,
            VerificationOutcome::NoIdentifier => self.no_identifier += 1,
            VerificationOutcome::Error => self.errors += 1,
        }
    }
}

/// Minimal bibliographic record returned by a resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkMeta {
    /// Canonical title.
    pub title: String,
    /// Publication year, when known.
    pub year: Option<u32>,
}

/// Metadata source (Crossref in production, a fake in tests).
///
/// `Send` futures keep this usable from the async orchestrator.
pub trait MetadataResolver {
    /// Resolve a DOI to a work, or `None` when the DOI does not exist.
    fn resolve_doi(
        &self,
        doi: &str,
    ) -> impl std::future::Future<Output = Result<Option<WorkMeta>, String>> + Send;

    /// Search for a work by title; `Ok(None)` means "no candidate".
    fn search_title(
        &self,
        title: &str,
    ) -> impl std::future::Future<Output = Result<Option<WorkMeta>, String>> + Send;

    /// Polite delay between requests (zero for fakes/tests).
    fn request_delay(&self) -> Duration {
        Duration::ZERO
    }
}

/// Verify every source in the registry, mutating its verification fields.
///
/// Errors are recorded per source, never fatal: a run should degrade to
/// "unverified" rather than abort because Crossref hiccuped.
pub async fn verify_sources<R: MetadataResolver>(
    sources: &mut SourcesFile,
    resolver: &R,
) -> VerificationSummary {
    let mut summary = VerificationSummary::default();
    let total = sources.sources.len();
    for index in 0..total {
        if index > 0 {
            let delay = resolver.request_delay();
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
        }
        let snapshot = sources.sources[index].clone();
        let (outcome, note) = verify_one(&snapshot, resolver).await;
        apply_verdict(&mut sources.sources[index], outcome, &note);
        summary.total += 1;
        summary.record(outcome);
    }
    summary
}

fn apply_verdict(source: &mut Source, outcome: VerificationOutcome, note: &str) {
    source.verified = outcome == VerificationOutcome::Verified;
    source.verification_source = Some(outcome.label().to_string());
    source.verification_note = Some(note.to_string());
}

async fn verify_one<R: MetadataResolver>(
    source: &Source,
    resolver: &R,
) -> (VerificationOutcome, String) {
    let title = source.title.trim();
    if let Some(doi) = source.doi.as_deref().filter(|d| !d.trim().is_empty()) {
        return match resolver.resolve_doi(doi).await {
            Ok(None) => (
                VerificationOutcome::NotFound,
                format!("DOI {doi} does not resolve in Crossref"),
            ),
            Ok(Some(work)) => {
                let similarity = title_similarity(title, &work.title);
                if similarity >= TITLE_MATCH_THRESHOLD {
                    (
                        VerificationOutcome::Verified,
                        format!("DOI {doi} matches title ({similarity:.2})"),
                    )
                } else {
                    (
                        VerificationOutcome::Mismatch,
                        format!(
                            "DOI {doi} resolves to a different work: {:?} ({similarity:.2})",
                            work.title
                        ),
                    )
                }
            }
            Err(error) => (
                VerificationOutcome::Error,
                format!("Crossref lookup failed for DOI {doi}: {error}"),
            ),
        };
    }

    if title.chars().count() < 12 {
        return (
            VerificationOutcome::NoIdentifier,
            "no DOI and title too short to search".to_string(),
        );
    }

    match resolver.search_title(title).await {
        Ok(Some(work)) => {
            let similarity = title_similarity(title, &work.title);
            if similarity >= TITLE_MATCH_THRESHOLD {
                (
                    VerificationOutcome::Verified,
                    format!("Crossref title match: {:?} ({similarity:.2})", work.title),
                )
            } else {
                (
                    VerificationOutcome::NoIdentifier,
                    format!(
                        "best Crossref title match is different: {:?} ({similarity:.2})",
                        work.title
                    ),
                )
            }
        }
        Ok(None) => (
            VerificationOutcome::NoIdentifier,
            "no DOI and no Crossref title match".to_string(),
        ),
        Err(error) => (
            VerificationOutcome::Error,
            format!("Crossref title search failed: {error}"),
        ),
    }
}

/// A prompt block describing source verification, or "" when there are no sources.
pub fn verification_context(sources: &SourcesFile) -> String {
    if sources.sources.is_empty() {
        return String::new();
    }
    let mut out = String::from("## Source Verification (Crossref)\n");
    for source in &sources.sources {
        let status = match source.verification_source.as_deref() {
            Some(label) => label.replace('_', " "),
            None => "unverified".to_string(),
        };
        let note = source.verification_note.as_deref().unwrap_or("");
        out.push_str(&format!(
            "- [{}] {} — {status}{}{}\n",
            source.id,
            source.title,
            if note.is_empty() { "" } else { ": " },
            note
        ));
    }
    out.push_str(
        "\nTreat mismatched or not-found sources as weak evidence; an unsupported or \
         unverifiable citation must be flagged, not trusted.\n",
    );
    out
}

/// Jaccard similarity over normalized title words.
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let left = title_tokens(a);
    let right = title_tokens(b);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn title_tokens(title: &str) -> HashSet<String> {
    title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 1)
        .filter(|word| {
            !matches!(
                *word,
                "the" | "and" | "for" | "with" | "from" | "into" | "that" | "this" | "via"
            )
        })
        .map(str::to_string)
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// Production resolver
// ═══════════════════════════════════════════════════════════════════════════

/// Crossref-backed resolver with a polite delay between requests.
pub struct CrossrefResolver {
    client: reqwest::Client,
    mailto: Option<String>,
}

impl CrossrefResolver {
    /// Build a resolver with a 15s timeout per request.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        CrossrefResolver {
            client,
            mailto: std::env::var("CROSSREF_EMAIL").ok().filter(|v| !v.is_empty()),
        }
    }

    fn user_agent(&self) -> String {
        match &self.mailto {
            Some(email) => format!("autoessay/0.1 (mailto:{email})"),
            None => "autoessay/0.1 (https://github.com/stansz/autoessay)".to_string(),
        }
    }

    async fn get_json(&self, url: reqwest::Url) -> Result<Option<serde_json::Value>, String> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, self.user_agent())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }
        response
            .json::<serde_json::Value>()
            .await
            .map(Some)
            .map_err(|e| e.to_string())
    }
}

impl Default for CrossrefResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataResolver for CrossrefResolver {
    async fn resolve_doi(&self, doi: &str) -> Result<Option<WorkMeta>, String> {
        let url = reqwest::Url::parse(&format!("https://api.crossref.org/works/{doi}"))
            .map_err(|e| e.to_string())?;
        let Some(json) = self.get_json(url).await? else {
            return Ok(None);
        };
        Ok(json.get("message").and_then(work_from_crossref))
    }

    async fn search_title(&self, title: &str) -> Result<Option<WorkMeta>, String> {
        let mut url = reqwest::Url::parse("https://api.crossref.org/works")
            .map_err(|e| e.to_string())?;
        url.query_pairs_mut()
            .append_pair("query.bibliographic", title)
            .append_pair("rows", "3")
            .append_pair("select", "title,issued,DOI");
        let Some(json) = self.get_json(url).await? else {
            return Ok(None);
        };
        Ok(json
            .pointer("/message/items/0")
            .and_then(work_from_crossref))
    }

    fn request_delay(&self) -> Duration {
        Duration::from_millis(150)
    }
}

fn work_from_crossref(item: &serde_json::Value) -> Option<WorkMeta> {
    let title = item
        .pointer("/title/0")
        .and_then(serde_json::Value::as_str)?
        .trim()
        .to_string();
    if title.is_empty() {
        return None;
    }
    let year = item
        .pointer("/issued/date-parts/0/0")
        .and_then(serde_json::Value::as_u64)
        .map(|y| y as u32);
    Some(WorkMeta { title, year })
}
