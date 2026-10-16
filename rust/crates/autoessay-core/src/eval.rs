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
}
