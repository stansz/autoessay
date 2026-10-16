//! Domain types for the pipeline + CLI.
//!
//! These are all re-exported from autoessay-core (the shared data model). This
//! module exists so the phases and orchestrator have a single import point and
//! so the crate's public surface does not scatter autoessay-core paths.

// Style profiles and their loader.
pub use autoessay_core::{
    builtin_styles, load_all_profiles, load_profile, load_profile_bytes, CitationVerification,
    Contractions, FirstPerson, HallucinationGate, PassiveVoice, StyleProfile,
};

// Evaluation results and hallucination flags.
pub use autoessay_core::{EvaluationResult, HallucinationFlag, Severity};

// Project state (state.json) and sources (sources.json).
pub use autoessay_core::{
    Phase, ProjectState, Scores, SectionState, SectionStatus, Source, SourcesFile,
};

// Program-document section extraction.
pub use autoessay_core::{load_program_file, load_program_section as core_load_program_section};

// The shared error type.
pub use autoessay_core::{CoreError, CoreResult};
