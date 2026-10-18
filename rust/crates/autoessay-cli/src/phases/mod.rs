//! The five pipeline phases.
//!
//! Each module ports one Python pipeline/*.py file verbatim (prompt text
//! included). Every phase function takes "&impl LlmClient" so it is testable
//! offline against a MockLlm.

pub mod drafting;
pub mod evaluation;
pub mod outline;
pub mod research;
pub mod revision;

// Outline extraction helper used by the orchestrator.
pub mod outline_parse;

pub use drafting::{build_system_prompt, build_user_prompt, draft_section, SectionBrief};
pub use evaluation::{
    build_eval_system_prompt, build_eval_user_prompt, evaluate_section, extract_json,
    parse_eval_response,
};
pub use outline::generate_outline;
pub use outline_parse::{parse_outline, OutlineSection, ParsedOutline};
pub use research::generate_research;
pub use revision::{
    generate_revision_brief, mean_score, revise_section, revise_until_pass, score_plateau,
    ReviseConfig, ReviseOutcome,
};
