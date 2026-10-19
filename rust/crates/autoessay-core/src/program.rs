//! program.md section extraction.
//!
//! Port of _load_program_section from autoessay/src/autoessay/pipeline/_utils.py,
//! but hardened: heading matching ignores markdown formatting (backticks around
//! script names) and whitespace, so the bundled program.md is actually found.
//! The pure function takes the document text as a parameter so it is trivially
//! unit-testable; a thin file wrapper is provided separately.

use std::path::Path;

use crate::error::CoreResult;

/// The literal separator the Python loader uses to mark the end of a section.
const NEXT_HEADING_PREFIX: &str = "## ";

/// Extract a section of a program document by "## heading", up to the next
/// "## " heading.
///
/// Matching is formatting-insensitive: a document heading
/// ``## Phase 1: Research (`gen_research.py`)`` matches the lookup key
/// `Phase 1: Research (gen_research.py)`. Backticks, markdown emphasis markers,
/// and repeated whitespace are stripped from both sides before comparison.
///
/// Returns None when the heading is not present. The returned text is the body
/// under the heading (excluding the heading line itself), trimmed.
pub fn load_program_section(doc: &str, heading: &str) -> Option<String> {
    let target = normalize_heading(heading);

    // Find the opening "## " line whose normalized text equals the target.
    let mut start = None;
    let mut offset = 0usize;
    for line in doc.split_inclusive('\n') {
        let text = line.trim_end_matches(['\r', '\n']);
        if is_h2(text) && normalize_heading(text) == target {
            start = Some(offset + line.len());
            break;
        }
        offset += line.len();
    }
    let start = start?;

    // Stop at the next "## " heading line.
    let body = &doc[start..];
    let mut end = body.len();
    offset = 0;
    for line in body.split_inclusive('\n') {
        let text = line.trim_end_matches(['\r', '\n']);
        if is_h2(text) {
            end = offset;
            break;
        }
        offset += line.len();
    }

    Some(body[..end].trim().to_string())
}

/// True when the line is a level-2 markdown heading ("## ", not "### ").
fn is_h2(line: &str) -> bool {
    line.starts_with(NEXT_HEADING_PREFIX) && !line.starts_with("### ")
}

/// Normalize a heading for comparison: drop leading hashes, backticks, markdown
/// emphasis markers, and collapse whitespace. Case is folded so "Phase 1" and
/// "phase 1" match.
fn normalize_heading(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '`' | '*' | '_'))
        .collect::<String>()
        .trim()
        .trim_start_matches('#')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Read a program.md file from disk and extract the named section.
pub fn load_program_file(path: &Path, heading: &str) -> CoreResult<Option<String>> {
    let text = std::fs::read_to_string(path)?;
    Ok(load_program_section(&text, heading))
}
