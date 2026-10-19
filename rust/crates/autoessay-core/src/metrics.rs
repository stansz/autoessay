//! Mechanical style metrics for a section or finished essay.
//!
//! These are the non-LLM half of style evaluation: readability and surface
//! features that an LLM judge tends to be inconsistent about. The 2026 evidence
//! on AI-heavy prose (denser, more nominalized, harder to read) makes these a
//! useful regression check on top of the evaluator's opinion.
//!
//! Heuristics are deliberately simple and dependency-free; they are signals for
//! a human and for the evaluator prompt, not scientific stylometry.

use serde::{Deserialize, Serialize};

/// Surface statistics for one section or document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StyleMetrics {
    /// Word count (tokens containing at least one alphanumeric character).
    pub words: u32,
    /// Sentence count (rough, split on . ! ?).
    pub sentences: u32,
    /// Mean words per sentence.
    pub avg_sentence_length: f64,
    /// Standard deviation of sentence lengths (rhythm / burstiness).
    pub sentence_length_stddev: f64,
    /// Flesch Reading Ease (higher = easier; 60-70 is plain English).
    pub flesch_reading_ease: f64,
    /// Fraction of sentences containing a passive-voice heuristic.
    pub passive_ratio: f64,
    /// Hedge words/phrases ("arguably", "somewhat", "perhaps", ...).
    pub hedge_count: u32,
    /// Nominalizations (words ending in -tion/-ment/-ness/-ity/...).
    pub nominalization_count: u32,
    /// First-person pronouns (I/me/my/we/us/our/...).
    pub first_person_count: u32,
    /// Contractions ("don't", "it's", ...).
    pub contraction_count: u32,
    /// Adverbs ending in -ly.
    pub adverb_ly_count: u32,
    /// Unique lowercased words / words (lexical variety).
    pub unique_word_ratio: f64,
}

const HEDGES: &[&str] = &[
    "arguably",
    "somewhat",
    "relatively",
    "generally",
    "perhaps",
    "possibly",
    "seemingly",
    "apparently",
    "might",
    "maybe",
    "tends",
    "typically",
];

const FIRST_PERSON: &[&str] = &["i", "me", "my", "mine", "we", "us", "our", "ours"];

const PASSIVE_AUX: &[&str] = &["is", "are", "was", "were", "be", "been", "being", "am"];

impl StyleMetrics {
    /// Measure a block of markdown/text.
    pub fn measure(text: &str) -> StyleMetrics {
        let words: Vec<String> = text
            .split_whitespace()
            .map(clean_word)
            .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
            .collect();
        let word_count = words.len() as u32;
        if word_count == 0 {
            return StyleMetrics::default();
        }

        let sentences = split_sentences(text);
        let sentence_word_counts: Vec<u32> = sentences
            .iter()
            .map(|s| {
                s.split_whitespace()
                    .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
                    .count() as u32
            })
            .filter(|n| *n > 0)
            .collect();
        let sentence_count = sentence_word_counts.len().max(1) as u32;

        let avg_sentence_length = word_count as f64 / sentence_count as f64;
        let stddev = stddev(&sentence_word_counts);

        let syllables: u32 = words.iter().map(|w| syllable_count(w)).sum();
        let flesch = if word_count > 0 {
            206.835 - 1.015 * avg_sentence_length - 84.6 * (syllables as f64 / word_count as f64)
        } else {
            0.0
        };

        let passive_sentences = sentences
            .iter()
            .filter(|sentence| has_passive_voice(sentence))
            .count();
        let passive_ratio = passive_sentences as f64 / sentence_count as f64;

        let unique: std::collections::HashSet<&str> =
            words.iter().map(String::as_str).collect();

        StyleMetrics {
            words: word_count,
            sentences: sentence_count,
            avg_sentence_length,
            sentence_length_stddev: stddev,
            flesch_reading_ease: round2(flesch),
            passive_ratio: round2(passive_ratio),
            hedge_count: words.iter().filter(|w| HEDGES.contains(&w.as_str())).count() as u32,
            nominalization_count: words
                .iter()
                .filter(|w| is_nominalization(w))
                .count() as u32,
            first_person_count: words
                .iter()
                .filter(|w| FIRST_PERSON.contains(&w.as_str()))
                .count() as u32,
            contraction_count: text
                .split_whitespace()
                .filter(|w| is_contraction(w))
                .count() as u32,
            adverb_ly_count: words.iter().filter(|w| is_adverb_ly(w)).count() as u32,
            unique_word_ratio: round2(unique.len() as f64 / word_count as f64),
        }
    }

    /// A compact human-readable summary for prompts and CLI output.
    pub fn summary(&self) -> String {
        format!(
            "{} words, {} sentences, avg {:.1} words/sentence (sd {:.1}), Flesch {:.1}, passive {:.0}%, hedges {}, nominalizations {}, first-person {}, contractions {}, -ly adverbs {}",
            self.words,
            self.sentences,
            self.avg_sentence_length,
            self.sentence_length_stddev,
            self.flesch_reading_ease,
            self.passive_ratio * 100.0,
            self.hedge_count,
            self.nominalization_count,
            self.first_person_count,
            self.contraction_count,
            self.adverb_ly_count,
        )
    }
}

fn clean_word(raw: &str) -> String {
    raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .to_lowercase()
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        sentences.push(trimmed.to_string());
    }
    sentences
}

fn stddev(values: &[u32]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().map(|v| *v as f64).sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|v| {
            let diff = *v as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    round2(variance.sqrt())
}

/// Rough English syllable count: vowel groups, with a silent trailing "e".
fn syllable_count(word: &str) -> u32 {
    let mut count = 0u32;
    let mut previous_was_vowel = false;
    for ch in word.chars() {
        let is_vowel = matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
        if is_vowel && !previous_was_vowel {
            count += 1;
        }
        previous_was_vowel = is_vowel;
    }
    if word.ends_with('e') && !word.ends_with("le") && count > 1 {
        count -= 1;
    }
    count.max(1)
}

fn has_passive_voice(sentence: &str) -> bool {
    let tokens: Vec<&str> = sentence
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphabetic()))
        .filter(|w| !w.is_empty())
        .collect();
    for (i, token) in tokens.iter().enumerate() {
        if PASSIVE_AUX.contains(&token.to_lowercase().as_str()) {
            for following in tokens.iter().skip(i + 1).take(3) {
                let lower = following.to_lowercase();
                if lower.ends_with("ed") || lower.ends_with("en") {
                    return true;
                }
            }
        }
    }
    false
}

fn is_nominalization(word: &str) -> bool {
    if word.len() < 6 {
        return false;
    }
    ["tion", "sion", "ment", "ness", "ity", "ance", "ence", "ism"]
        .iter()
        .any(|suffix| word.ends_with(suffix))
}

fn is_contraction(raw: &str) -> bool {
    let token = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'');
    token.contains('\'')
        && token
            .split('\'')
            .nth(1)
            .is_some_and(|suffix| {
                matches!(
                    suffix.to_lowercase().as_str(),
                    "t" | "s" | "re" | "ve" | "ll" | "d" | "m"
                )
            })
}

fn is_adverb_ly(word: &str) -> bool {
    word.len() > 4
        && word.ends_with("ly")
        && !matches!(
            word,
            "only" | "early" | "family" | "likely" | "friendly" | "lonely" | "ugly"
        )
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Remote work did not kill company culture. It moved it. \
        The office was never the culture; the rituals were. Teams that thrived \
        built explicit rituals, written norms, and deliberate gatherings. That is \
        arguably harder than leasing a floor, but it is not impossible.";

    #[test]
    fn measures_words_and_sentences() {
        let m = StyleMetrics::measure(SAMPLE);
        assert!(m.words > 30, "words={}", m.words);
        assert_eq!(m.sentences, 5);
        assert!(m.avg_sentence_length > 5.0);
    }

    #[test]
    fn detects_first_person_contractions_and_nominalizations() {
        let text = "I don't think the operationalization of culture is easy. We can't measure it.";
        let m = StyleMetrics::measure(text);
        assert_eq!(m.first_person_count, 2);
        assert_eq!(m.contraction_count, 2);
        assert!(m.nominalization_count >= 1);
    }

    #[test]
    fn flesch_score_is_plausible() {
        let m = StyleMetrics::measure(SAMPLE);
        assert!(
            m.flesch_reading_ease > 20.0 && m.flesch_reading_ease < 100.0,
            "flesch={}",
            m.flesch_reading_ease
        );
    }

    #[test]
    fn empty_text_is_zeroed() {
        assert_eq!(StyleMetrics::measure(""), StyleMetrics::default());
    }
}
