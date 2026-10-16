//! Parse the outline markdown produced by Phase 1b into a structured section map.
//!
//! The outline follows program.md's format:
//!
//! ## Thesis
//! ...
//! ## Argument Structure
//! 1. **Section Title** — One-sentence purpose | Sources: [src_01, src_02]
//! ## Counterargument Section
//! ...
//! ## Conclusion Arc
//! ...
//!
//! The orchestrator needs the numbered section list to know what to draft. This
//! parser is deliberately forgiving: an unparseable outline yields an empty
//! section list (the caller reports a clear error), never a panic.

use regex::Regex;

/// One section in the argument structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineSection {
    pub title: String,
    pub purpose: String,
    pub sources: Vec<String>,
}

/// The structured view of an outline document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedOutline {
    pub thesis: String,
    pub sections: Vec<OutlineSection>,
}

/// Parse outline markdown into a structured section map.
pub fn parse_outline(outline: &str) -> ParsedOutline {
    let thesis = extract_heading_body(outline, "Thesis");
    let argument = extract_heading_body(outline, "Argument Structure");

    let item_re = match Regex::new(r"(?m)^\s*\d+\.\s+(.*)$") {
        Ok(re) => re,
        Err(_) => {
            return ParsedOutline {
                thesis,
                sections: Vec::new(),
            }
        }
    };

    let mut sections = Vec::new();
    for caps in item_re.captures_iter(&argument) {
        let line = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
        if let Some(section) = parse_outline_item(line) {
            sections.push(section);
        }
    }

    ParsedOutline { thesis, sections }
}

/// Parse one numbered outline item:
/// "**Section Title** — One-sentence purpose | Sources: [src_01, src_02]"
fn parse_outline_item(line: &str) -> Option<OutlineSection> {
    // Title: text between the first pair of ** ** markers.
    let title_re = Regex::new(r"^\*\*([^*]+)\*\*").ok()?;
    let title = title_re
        .captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())?;

    // Sources: "Sources: [a, b]" or "Sources: [a]" anywhere in the line.
    let sources_re = Regex::new(r"Sources:\s*\[([^\]]*)\]").ok()?;
    let sources: Vec<String> = sources_re
        .captures(line)
        .and_then(|c| c.get(1))
        .map(|m| {
            m.as_str()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Purpose: everything after the title, minus the "| Sources: ..." tail.
    // Use char-boundary-safe slicing: the title/purpose often contain multi-byte
    // characters (em dashes) that would panic a byte-index slice.
    let mut purpose = line.to_string();
    if let Some(caps) = title_re.captures(line) {
        if let Some(m) = caps.get(1) {
            // remove the leading **title** token; +4 for the two "**" pairs
            let end = m.end() + 4;
            purpose = slice_at(line, end);
        }
    }
    if let Some(idx) = purpose.find('|') {
        purpose = slice_at(&purpose, idx);
    }
    let purpose = purpose
        .trim()
        .trim_start_matches('—')
        .trim_start_matches('-')
        .trim()
        .to_string();

    Some(OutlineSection {
        title,
        purpose,
        sources,
    })
}

/// Slice `s` at byte index `idx`, clamping to the nearest char boundary so a
/// multi-byte character never panics a byte-index slice.
fn slice_at(s: &str, idx: usize) -> String {
    if idx >= s.len() {
        return String::new();
    }
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    s[i..].to_string()
}

/// Extract the body under a heading (any level) up to the next heading.
fn extract_heading_body(text: &str, heading: &str) -> String {
    // Find a markdown heading line whose text equals or contains the heading.
    let heading_re = match Regex::new(&format!(r"(?m)^#{{1,6}}\s+{}\s*$", regex::escape(heading)))
    {
        Ok(re) => re,
        Err(_) => return String::new(),
    };
    let m = match heading_re.find(text) {
        Some(m) => m,
        None => return String::new(),
    };

    let start = m.end();
    let body = &text[start..];

    // Stop at the next heading line.
    let next_re = match Regex::new(r"(?m)^#{1,6}\s+") {
        Ok(re) => re,
        Err(_) => return body.trim().to_string(),
    };
    match next_re.find(body) {
        Some(next) => body[..next.start()].trim().to_string(),
        None => body.trim().to_string(),
    }
}
