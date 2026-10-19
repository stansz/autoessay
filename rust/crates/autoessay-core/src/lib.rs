//! autoessay-core: shared domain types for the autoessay pipeline.
//!
//! This crate holds the data model that every other crate depends on:
//!
//! * style - style profiles and the YAML loader (port of style/profile.py)
//! * eval - evaluation results, claim support, outline reviews
//! * metrics - mechanical style metrics (readability, hedges, nominalizations)
//! * state - state.json and sources.json shapes (the on-disk contract)
//! * program - program.md section extraction
//! * error - the single CoreError type
//!
//! Design rule for the whole project: there is no unwrap/expect/panic! outside
//! of tests. Fallible operations return Result<T, CoreError> so that failure is
//! a value the compiler forces callers to handle.

pub mod error;
pub mod eval;
pub mod metrics;
pub mod program;
pub mod state;
pub mod style;

pub use error::{CoreError, CoreResult};
pub use eval::{
    ClaimStatus, ClaimSupport, EvaluationResult, HallucinationFlag, OutlineReview, Severity,
};
pub use metrics::StyleMetrics;
pub use program::{load_program_file, load_program_section};
pub use state::{
    Phase, ProjectState, Scores, SectionState, SectionStatus, Source, SourcesFile,
};
pub use style::{
    builtin_styles, load_all_profiles, load_profile, load_profile_bytes, CitationVerification,
    Contractions, FirstPerson, HallucinationGate, PassiveVoice, StyleProfile,
};
