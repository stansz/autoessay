//! Source extraction from research notes -> sources.json.
//!
//! The Python reference tracked sources in a registry but never parsed them out
//! of the model's research output; the orchestrator here is the first thing to
//! do so. Research notes follow program.md's format:
//!
//! ### Sources
//! - [src_01] Title, Author, Year, URL, Key Quote
//!
//! This module extracts those lines into autoessay_core::Source records. It is
//! deliberately forgiving: a line that does not match is skipped, never a panic.

use regex::Regex;

use crate::domains::{Source, SourcesFile};

/// Extract a source registry from research-notes markdown.
pub fn extract_sources(research_notes: &str) -> SourcesFile {
    let re = match Regex::new(r"(?m)^\s*-\s*\[([^\]]+)\]\s*(.*)$") {
        Ok(re) => re,
        // The pattern above is a compile-time constant; a failure here is a bug,
        // not user input. Treat it as "no sources" rather than panicking.
        Err(_) => return SourcesFile::default(),
    };

    let mut sources = Vec::new();
    for caps in re.captures_iter(research_notes) {
        let id = caps
            .get(1)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let rest = caps
            .get(2)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        sources.push(parse_source_line(&id, &rest));
    }

    SourcesFile { sources }
}

/// Parse the free-form tail of a source line: "Title, Author, Year, URL, Key Quote".
///
/// Fields are separated by commas; we pull out a URL (http/https/doi) and a
/// trailing 4-digit year when present, and treat the first comma-segment as the
/// title. Everything optional stays None/default when it cannot be identified.
fn parse_source_line(id: &str, rest: &str) -> Source {
    let segments: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();

    let title = segments
        .first()
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Find a URL anywhere in the tail.
    let url = Regex::new(r"https?://\S+")
        .ok()
        .and_then(|re| re.find(rest))
        .map(|m| m.as_str().trim_end_matches(',').to_string());

    // Find a standalone 4-digit year in [1500, 2100].
    let mut year = None;
    if let Ok(re) = Regex::new(r"\b(\d{4})\b") {
        for cap in re.captures_iter(rest) {
            if let Some(m) = cap.get(1) {
                if let Ok(y) = m.as_str().parse::<u32>() {
                    if (1500..=2100).contains(&y) {
                        year = Some(y);
                        break;
                    }
                }
            }
        }
    }

    // DOI heuristic: "10.xxxx/..." token.
    let doi = Regex::new(r"\b10\.\d{4,9}/\S+")
        .ok()
        .and_then(|re| re.find(rest))
        .map(|m| m.as_str().trim_end_matches(',').to_string());

    // Authors: up to two segments after the title that are not a URL/year.
    let authors: Vec<String> = segments
        .iter()
        .skip(1)
        .take(2)
        .filter(|s| !s.is_empty())
        .filter(|s| !s.starts_with("http"))
        .filter(|s| s.parse::<u32>().is_err())
        .map(|s| s.to_string())
        .collect();

    // Key quote: the final segment when there are 3+ segments and it is prose.
    let key_quote = if segments.len() >= 3 {
        segments
            .last()
            .filter(|s| s.len() > 20 && !s.starts_with("http"))
            .map(|s| s.to_string())
    } else {
        None
    };

    Source {
        id: id.to_string(),
        source_type: "academic".to_string(),
        title,
        authors,
        year,
        doi,
        url,
        key_quote,
        embedding_vector: None,
        verified: false,
        verification_source: None,
    }
}

/// Serialize a source registry to pretty JSON for sources.json.
pub fn registry_to_json(registry: &SourcesFile) -> crate::error::CliResult<String> {
    registry
        .to_json()
        .map_err(|e| crate::error::CliError::Project(format!("sources serialize failed: {e}")))
}

/// Parse sources.json back into a registry.
pub fn registry_from_json(json: &str) -> crate::error::CliResult<SourcesFile> {
    SourcesFile::from_json(json)
        .map_err(|e| crate::error::CliError::Project(format!("sources parse failed: {e}")))
}
