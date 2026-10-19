//! Evaluation result types.
//!
//! Port of the data model in autoessay/src/autoessay/pipeline/evaluation.py.

use serde::{Deserialize, Serialize};

/// Severity of a flagged hallucination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Fabricated or contradicting claim - auto-fails under a strict gate.
    Critical,
    /// Unsupported or overstated claim.
    Major,
    /// Minor imprecision worth noting.
    #[default]
    Minor,
}

/// A flagged claim that may be unsupported or fabricated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HallucinationFlag {
    /// The claim text that was flagged.
    #[serde(default)]
    pub claim: String,

    /// How serious the flag is.
    #[serde(default)]
    pub severity: Severity,

    /// Why it was flagged.
    #[serde(default)]
    pub note: String,
}

/// How well a claim is supported by the cited source / research notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ClaimStatus {
    /// The source clearly supports the claim.
    Supported,
    /// The source supports part of the claim, or supports it only weakly.
    Partial,
    /// Nothing in the provided material supports the claim.
    #[default]
    Unsupported,
}

/// One row of the claim-level support matrix.
///
/// The evaluator decomposes the section into claims and marks each against the
/// research notes. This is deliberately finer-grained than the holistic
/// `hallucination_flags`: it makes "which sentence is unsupported" inspectable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ClaimSupport {
    /// The claim as it appears in the section.
    #[serde(default)]
    pub claim: String,
    /// Source id the claim cites, when it cites one.
    #[serde(default)]
    pub source_id: Option<String>,
    /// Support verdict.
    #[serde(default)]
    pub status: ClaimStatus,
    /// Why the claim got this verdict.
    #[serde(default)]
    pub note: String,
}

/// Structured evaluation of a single drafted section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EvaluationResult {
    /// Factual fidelity to sources (1-10).
    #[serde(default)]
    pub accuracy: f64,

    /// Logical flow and counterargument engagement (1-10).
    #[serde(default)]
    pub coherence: f64,

    /// Adherence to the style profile (1-10).
    #[serde(default)]
    pub style: f64,

    /// Citation verifiability and source diversity (1-10).
    #[serde(default)]
    pub source_integrity: f64,

    /// "pass" or "fail".
    #[serde(default = "default_overall")]
    pub overall: String,

    /// Claims flagged as potentially hallucinated.
    #[serde(default)]
    pub hallucination_flags: Vec<HallucinationFlag>,

    /// Claim-level support matrix (empty when the evaluator did not run one).
    #[serde(default)]
    pub claims: Vec<ClaimSupport>,

    /// Free-form editorial feedback.
    #[serde(default)]
    pub notes: String,
}

fn default_overall() -> String {
    String::from("fail")
}

impl EvaluationResult {
    /// Whether this section meets all quality thresholds.
    ///
    /// Equivalent to the Python EvaluationResult.passed property.
    pub fn passed(&self) -> bool {
        self.overall == "pass"
    }

    /// Counts of the claim-support verdicts.
    pub fn claim_counts(&self) -> (usize, usize, usize) {
        let supported = self
            .claims
            .iter()
            .filter(|c| c.status == ClaimStatus::Supported)
            .count();
        let partial = self
            .claims
            .iter()
            .filter(|c| c.status == ClaimStatus::Partial)
            .count();
        let unsupported = self
            .claims
            .iter()
            .filter(|c| c.status == ClaimStatus::Unsupported)
            .count();
        (supported, partial, unsupported)
    }
}

/// Structured review of the outline, before any section is drafted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OutlineReview {
    /// Is the thesis specific, debatable, and one sentence? (1-10)
    #[serde(default)]
    pub thesis: f64,
    /// Does the argument structure build logically? (1-10)
    #[serde(default)]
    pub structure: f64,
    /// Does every section map to sources / evidence? (1-10)
    #[serde(default)]
    pub source_coverage: f64,
    /// Is a counterargument engaged explicitly? (1-10)
    #[serde(default)]
    pub counterargument: f64,
    /// "pass" or "fail".
    #[serde(default = "default_overall")]
    pub overall: String,
    /// Concrete problems to fix.
    #[serde(default)]
    pub issues: Vec<String>,
    /// Editorial notes.
    #[serde(default)]
    pub notes: String,
}

impl OutlineReview {
    /// A review passes when the model says pass and every dimension clears 6/10.
    pub fn passed(&self) -> bool {
        self.overall == "pass"
            && self.thesis >= 6.0
            && self.structure >= 6.0
            && self.source_coverage >= 6.0
            && self.counterargument >= 6.0
    }

    /// The first few issues, for progress logs and prompts.
    pub fn issue_summary(&self) -> String {
        if self.issues.is_empty() {
            "no specific issues".to_string()
        } else {
            self.issues.join("; ")
        }
    }
}
