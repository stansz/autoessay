//! Style profile loading — delegated to autoessay-core.
//!
//! Thin wrappers so the CLI and phases can resolve profiles without importing
//! autoessay-core directly, and so the CLI's error type stays CliError.

use std::path::Path;

use crate::domains::StyleProfile;
use crate::error::{CliError, CliResult};

/// Load a style profile by name (disk copy preferred, then bundled).
pub fn load_profile(name: &str, styles_dir: Option<&Path>) -> CliResult<StyleProfile> {
    autoessay_core::load_profile(name, styles_dir)
        .map_err(|e| CliError::profile_not_found(format!("{name}: {e}")))
}

/// List available profile names, sorted.
pub fn list_profiles(styles_dir: Option<&Path>) -> Vec<String> {
    autoessay_core::style::list_profiles(styles_dir)
}
