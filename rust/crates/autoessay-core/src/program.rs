//! program.md section extraction.
//!
//! Port of _load_program_section from autoessay/src/autoessay/pipeline/_utils.py.
//! The pure function takes the document text as a parameter so it is trivially
//! unit-testable; a thin file wrapper is provided separately.

use std::path::Path;

use crate::error::CoreResult;

/// The literal separator the Python loader uses to mark the end of a section.
const NEXT_HEADING: &str = concat!("\n", "## ");

/// Extract a section of a program document by "## heading", up to the next
/// "## " heading.
///
/// Returns None when the heading is not present. The returned text is the body
/// under the heading (excluding the heading line itself), trimmed.
///
/// This mirrors the Python _load_program_section behaviour exactly: the marker
/// is the literal string "## <heading>", the body starts immediately after the
/// marker, and it stops at the next occurrence of a newline followed by "## ".
pub fn load_program_section(doc: &str, heading: &str) -> Option<String> {
    let marker = format!("## {heading}");
    let idx = doc.find(&marker)?;

    let start = idx + marker.len();
    let mut body = &doc[start..];

    if let Some(next_h2) = body.find(NEXT_HEADING) {
        body = &body[..next_h2];
    }

    Some(body.trim().to_string())
}

/// Read a program.md file from disk and extract the named section.
pub fn load_program_file(path: &Path, heading: &str) -> CoreResult<Option<String>> {
    let text = std::fs::read_to_string(path)?;
    Ok(load_program_section(&text, heading))
}
