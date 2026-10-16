//! Program-document and anti-slop rule loading.
//!
//! The program.md parser is delegated to autoessay-core (load_program_section is
//! a pure function over the document text). This module supplies the document:
//! a disk copy when present, otherwise the copy bundled with this crate. The
//! ANTI-SLOP.md loader is local to the drafting phase and mirrors Python
//! drafting.py _load_anti_slop (disk copy preferred, bundled fallback).

use std::path::PathBuf;

/// The bundled program.md shipped with this crate, used when no disk copy is
/// found.
pub const BUNDLED_PROGRAM: &str = include_str!("../assets/program.md");

/// The bundled ANTI-SLOP.md (non-fiction edition) shipped with the crate.
pub const BUNDLED_ANTI_SLOP: &str = include_str!("../assets/ANTI-SLOP.md");

/// Resolve a program.md section by heading. Prefers a disk copy of program.md,
/// then the bundled copy. Returns "" when the section is absent so callers can
/// fall back to their own default program text.
pub fn load_program_section(heading: &str) -> String {
    let doc = read_disk_program().unwrap_or_else(|| BUNDLED_PROGRAM.to_string());
    autoessay_core::program::load_program_section(&doc, heading).unwrap_or_default()
}

/// Load ANTI-SLOP.md content. Prefers a disk copy, falls back to the bundled
/// copy. Mirrors Python _load_anti_slop.
pub fn load_anti_slop() -> String {
    if let Some(path) = disk_anti_slop_path() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    BUNDLED_ANTI_SLOP.trim().to_string()
}

fn read_disk_program() -> Option<String> {
    let path = disk_program_path()?;
    std::fs::read_to_string(path).ok()
}

fn disk_program_path() -> Option<PathBuf> {
    doc_search_paths("program.md")
        .into_iter()
        .find(|candidate| candidate.exists())
}

fn disk_anti_slop_path() -> Option<PathBuf> {
    doc_search_paths("ANTI-SLOP.md")
        .into_iter()
        .find(|candidate| candidate.exists())
}

/// Ordered candidate locations for a repo-level doc file: the current directory,
/// then the autoessay repo root when running from a checkout.
fn doc_search_paths(filename: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.push(PathBuf::from(filename));
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join(filename));
        out.push(cwd.join("..").join("..").join("autoessay").join(filename));
    }
    out
}
