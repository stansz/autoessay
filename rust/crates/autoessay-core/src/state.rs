//! Persistent project state: state.json and sources.json.
//!
//! Field names are the exact external JSON contract documented in
//! autoessay/ARCHITECTURE.md "Data Model". They are spelled out explicitly in
//! serde attributes so the on-disk shape cannot drift by accident.

use serde::{Deserialize, Serialize};

use crate::eval::HallucinationFlag;
use crate::error::CoreResult;

// ---------------------------------------------------------------------------
// Section scores
// ---------------------------------------------------------------------------

/// The four quality scores recorded for a section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Scores {
    /// Factual fidelity to sources (1-10).
    #[serde(default)]
    pub accuracy: f64,
    /// Logical flow (1-10).
    #[serde(default)]
    pub coherence: f64,
    /// Style adherence (1-10).
    #[serde(default)]
    pub style: f64,
    /// Citation verifiability and source diversity (1-10).
    #[serde(default)]
    pub source_integrity: f64,
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

/// Lifecycle of a single section through the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SectionStatus {
    /// Not drafted yet.
    #[default]
    Pending,
    /// A draft exists but has not passed evaluation.
    Drafted,
    /// Evaluation passed.
    Complete,
}

/// One section of the essay, as persisted in state.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionState {
    /// Stable identifier, e.g. "sec_01".
    pub id: String,

    /// Human-readable section title.
    pub title: String,

    /// Current lifecycle status.
    pub status: SectionStatus,

    /// Latest evaluation scores.
    #[serde(default)]
    pub scores: Scores,

    /// How many revision cycles this section has been through.
    #[serde(default = "default_revision_cycles")]
    pub revision_cycles: u32,

    /// Claims flagged by the hallucination gate.
    #[serde(default)]
    pub hallucination_flags: Vec<HallucinationFlag>,
}

fn default_revision_cycles() -> u32 {
    0
}

impl Default for SectionState {
    fn default() -> Self {
        SectionState {
            id: String::new(),
            title: String::new(),
            status: SectionStatus::Pending,
            scores: Scores::default(),
            revision_cycles: 0,
            hallucination_flags: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Project state
// ---------------------------------------------------------------------------

/// Which pipeline phase the project is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Collecting research notes.
    #[default]
    Research,
    /// Building the outline.
    Outline,
    /// Writing section drafts.
    Drafting,
    /// Scoring drafts.
    Evaluation,
    /// Applying revision briefs.
    Revision,
    /// Everything passed.
    Complete,
}

/// The complete state.json document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectState {
    /// Current pipeline phase.
    pub phase: Phase,

    /// Pipeline iteration counter.
    #[serde(default)]
    pub iteration: u32,

    /// Name of the active style profile, e.g. "magazine".
    pub style_profile: String,

    /// Target audience level (1 = general, 5 = specialist).
    pub audience_level: u8,

    /// Target word count for the finished piece.
    pub target_words: u32,

    /// Per-section state, in document order.
    #[serde(default)]
    pub sections: Vec<SectionState>,

    /// Overall score once the piece is complete, else null.
    #[serde(default)]
    pub overall_score: Option<f64>,

    /// True when the revision loop stopped because scores had plateaued.
    #[serde(default = "default_plateau")]
    pub plateau_detected: bool,
}

fn default_plateau() -> bool {
    false
}

impl Default for ProjectState {
    fn default() -> Self {
        ProjectState {
            phase: Phase::default(),
            iteration: 0,
            style_profile: String::new(),
            audience_level: 3,
            target_words: 3000,
            sections: Vec::new(),
            overall_score: None,
            plateau_detected: false,
        }
    }
}

impl ProjectState {
    /// Look up a section by id.
    pub fn section(&self, id: &str) -> Option<&SectionState> {
        self.sections.iter().find(|s| s.id == id)
    }

    /// Mutable lookup of a section by id.
    pub fn section_mut(&mut self, id: &str) -> Option<&mut SectionState> {
        self.sections.iter_mut().find(|s| s.id == id)
    }

    /// Serialize to pretty JSON text.
    pub fn to_json(&self) -> CoreResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parse from JSON text.
    pub fn from_json(text: &str) -> CoreResult<ProjectState> {
        Ok(serde_json::from_str(text)?)
    }
}

// ---------------------------------------------------------------------------
// sources.json
// ---------------------------------------------------------------------------

/// One research source, as persisted in sources.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// Stable identifier, e.g. "src_01".
    pub id: String,

    /// Source kind, e.g. "academic", "web", "interview".
    #[serde(rename = "type")]
    pub source_type: String,

    /// Title of the work.
    pub title: String,

    /// Author names, in publication order.
    #[serde(default)]
    pub authors: Vec<String>,

    /// Publication year.
    #[serde(default)]
    pub year: Option<u32>,

    /// Digital object identifier, if any.
    #[serde(default)]
    pub doi: Option<String>,

    /// Canonical URL, if any.
    #[serde(default)]
    pub url: Option<String>,

    /// The excerpt used as evidence in the essay.
    #[serde(default)]
    pub key_quote: Option<String>,

    /// Optional embedding used for deduplication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_vector: Option<Vec<f64>>,

    /// Whether the source has been verified.
    #[serde(default)]
    pub verified: bool,

    /// How the source was verified, e.g. "crossref".
    #[serde(default)]
    pub verification_source: Option<String>,
}

impl Default for Source {
    fn default() -> Self {
        Source {
            id: String::new(),
            source_type: String::from("web"),
            title: String::new(),
            authors: Vec::new(),
            year: None,
            doi: None,
            url: None,
            key_quote: None,
            embedding_vector: None,
            verified: false,
            verification_source: None,
        }
    }
}

/// The complete sources.json document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SourcesFile {
    /// Every source gathered for the project.
    #[serde(default)]
    pub sources: Vec<Source>,
}

impl SourcesFile {
    /// Look up a source by id.
    pub fn source(&self, id: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// Serialize to pretty JSON text.
    pub fn to_json(&self) -> CoreResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parse from JSON text.
    pub fn from_json(text: &str) -> CoreResult<SourcesFile> {
        Ok(serde_json::from_str(text)?)
    }
}
