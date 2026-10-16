//! Style profile data model and YAML loader.
//!
//! Port of autoessay/src/autoessay/style/profile.py.
//!
//! A StyleProfile describes *how* a piece should be written and *how strict*
//! the pipeline must be while writing it. Profiles live as YAML documents
//! (see assets/styles/*.yaml) and are injected into every drafting prompt.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

// ---------------------------------------------------------------------------
// Voice enums
// ---------------------------------------------------------------------------

/// Whether first person ("I"/"we") may appear in the prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FirstPerson {
    /// Never use first person.
    #[default]
    False,
    /// First person is the default perspective.
    True,
    /// First person is acceptable in specific rhetorical situations.
    Situational,
}

/// How freely passive voice may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PassiveVoice {
    /// Passive voice is fine.
    Allowed,
    /// Minimize passive voice but do not ban it.
    #[default]
    Moderate,
    /// Avoid passive voice.
    Discouraged,
    /// Never use passive voice.
    Prohibited,
}

/// Whether contractions ("don't", "can't") may appear in the prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Contractions {
    /// Never use contractions.
    #[default]
    False,
    /// Contractions are the default register.
    True,
    /// Contractions are acceptable in specific rhetorical situations.
    Situational,
}

/// How strictly the hallucination gate polices unsupported claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HallucinationGate {
    /// Every unsupported claim is flagged critical and auto-fails the section.
    Strict,
    /// Only major issues block; minor flags still allow a pass.
    #[default]
    Lenient,
    /// Skip hallucination detection entirely.
    Off,
}

/// Which service verifies citations for this profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CitationVerification {
    /// CrossRef / Semantic Scholar API verification.
    Crossref,
    /// Light web verification.
    #[default]
    Web,
    /// No citation verification.
    Off,
}

// ---------------------------------------------------------------------------
// StyleProfile
// ---------------------------------------------------------------------------

/// A writing style profile - loaded from YAML and injected into LLM prompts.
///
/// Mirrors the structure of config/styles/<name>.yaml in the Python
/// implementation. The nested pipeline, thresholds and voice groups of the YAML
/// document are flattened onto this struct; from_yaml_doc and from_yaml_str
/// perform that mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleProfile {
    /// Profile name without extension (e.g. "magazine").
    pub name: String,

    /// One-line description of what the profile is for.
    #[serde(default)]
    pub use_case: String,

    /// Short bullet points describing the profile's characteristics.
    #[serde(default)]
    pub key_traits: Vec<String>,

    /// Which service verifies citations.
    #[serde(default)]
    pub citation_verification: CitationVerification,

    /// Whether near-duplicate sources are collapsed.
    #[serde(default = "default_true")]
    pub source_deduplication: bool,

    /// How strictly unsupported claims are policed.
    #[serde(default)]
    pub hallucination_gate: HallucinationGate,

    /// Whether every claim must carry a citation.
    #[serde(default)]
    pub require_citation_per_claim: bool,

    /// Whether opinion without a source is permitted.
    #[serde(default = "default_true")]
    pub allow_unsourced_opinion: bool,

    /// Factual fidelity threshold (1-10); None means not scored.
    #[serde(default)]
    pub accuracy_threshold: Option<f64>,

    /// Logical-flow threshold (1-10).
    #[serde(default = "default_coherence_threshold")]
    pub coherence_threshold: f64,

    /// Style-adherence threshold (1-10).
    #[serde(default = "default_style_threshold")]
    pub style_threshold: f64,

    /// Citation-verification threshold (1-10); None means not scored.
    #[serde(default)]
    pub source_integrity_threshold: Option<f64>,

    /// First-person policy.
    #[serde(default)]
    pub first_person: FirstPerson,

    /// Passive-voice policy.
    #[serde(default)]
    pub passive_voice: PassiveVoice,

    /// Contractions policy.
    #[serde(default)]
    pub contractions: Contractions,

    /// Free-form instructions injected into every drafting call.
    #[serde(default)]
    pub prompt_instructions: String,
}

fn default_true() -> bool {
    true
}

fn default_coherence_threshold() -> f64 {
    7.0
}

fn default_style_threshold() -> f64 {
    7.0
}

impl Default for StyleProfile {
    fn default() -> Self {
        StyleProfile {
            name: String::new(),
            use_case: String::new(),
            key_traits: Vec::new(),
            citation_verification: CitationVerification::default(),
            source_deduplication: true,
            hallucination_gate: HallucinationGate::default(),
            require_citation_per_claim: false,
            allow_unsourced_opinion: true,
            accuracy_threshold: Some(7.0),
            coherence_threshold: 7.0,
            style_threshold: 7.0,
            source_integrity_threshold: Some(7.0),
            first_person: FirstPerson::default(),
            passive_voice: PassiveVoice::default(),
            contractions: Contractions::default(),
            prompt_instructions: String::new(),
        }
    }
}

impl StyleProfile {
    /// Whether hallucination detection should run for this profile.
    ///
    /// Equivalent to the Python StyleProfile.hallucination_enabled property.
    pub fn hallucination_enabled(&self) -> bool {
        self.hallucination_gate != HallucinationGate::Off
    }

    /// Whether every claim must have a source citation.
    ///
    /// Equivalent to the Python StyleProfile.citations_required property.
    pub fn citations_required(&self) -> bool {
        self.require_citation_per_claim
    }

    /// Build a StyleProfile from a parsed YAML document.
    ///
    /// The YAML uses the nested pipeline / thresholds / voice groups from the
    /// Python config format; this maps them onto the flattened struct and
    /// validates the enum-valued fields.
    pub fn from_yaml_doc(raw: &serde_yaml::Value) -> CoreResult<StyleProfile> {
        let map = match raw.as_mapping() {
            Some(m) => m,
            None => {
                return Err(CoreError::parse(
                    "style profile YAML root must be a mapping",
                ))
            }
        };

        let get = |key: &str| -> Option<&serde_yaml::Value> {
            map.get(serde_yaml::Value::String(key.to_string()))
        };

        let name = match get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return Err(CoreError::parse(
                    "style profile is missing required field name",
                ))
            }
        };

        let group = |key: &str| -> Option<&serde_yaml::Mapping> {
            get(key).and_then(|v| v.as_mapping())
        };

        let group_get = |group_key: &str, field: &str| -> Option<&serde_yaml::Value> {
            group(group_key)
                .and_then(|m| m.get(serde_yaml::Value::String(field.to_string())))
        };

        let as_str = |v: Option<&serde_yaml::Value>| -> Option<String> {
            v.and_then(|x| x.as_str()).map(|s| s.to_string())
        };

        let as_bool = |v: Option<&serde_yaml::Value>| -> Option<bool> {
            v.and_then(|x| x.as_bool())
        };

        let as_f64 = |v: Option<&serde_yaml::Value>| -> Option<f64> {
            v.and_then(|x| x.as_f64())
        };

        let as_str_list = |v: Option<&serde_yaml::Value>| -> Vec<String> {
            v.and_then(|x| x.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|item| item.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default()
        };

        let first_person_raw = normalize_enum(
            "first_person",
            as_str(group_get("voice", "first_person")),
            "false",
            &["true", "false", "situational"],
        )?;
        let passive_voice_raw = normalize_enum(
            "passive_voice",
            as_str(group_get("voice", "passive_voice")),
            "moderate",
            &["allowed", "moderate", "discouraged", "prohibited"],
        )?;
        let contractions_raw = normalize_enum(
            "contractions",
            as_str(group_get("voice", "contractions")),
            "false",
            &["true", "false", "situational"],
        )?;
        let gate_raw = normalize_enum(
            "hallucination_gate",
            as_str(group_get("pipeline", "hallucination_gate")),
            "lenient",
            &["strict", "lenient", "off"],
        )?;
        let citation_raw = normalize_enum(
            "citation_verification",
            as_str(group_get("pipeline", "citation_verification")),
            "web",
            &["crossref", "web", "off"],
        )?;

        Ok(StyleProfile {
            name,
            use_case: as_str(get("use_case")).unwrap_or_default(),
            key_traits: as_str_list(get("key_traits")),
            citation_verification: match citation_raw.as_str() {
                "crossref" => CitationVerification::Crossref,
                "off" => CitationVerification::Off,
                _ => CitationVerification::Web,
            },
            source_deduplication: as_bool(group_get("pipeline", "source_deduplication"))
                .unwrap_or(true),
            hallucination_gate: match gate_raw.as_str() {
                "strict" => HallucinationGate::Strict,
                "off" => HallucinationGate::Off,
                _ => HallucinationGate::Lenient,
            },
            require_citation_per_claim: as_bool(
                group_get("pipeline", "require_citation_per_claim"),
            )
            .unwrap_or(false),
            allow_unsourced_opinion: as_bool(group_get("pipeline", "allow_unsourced_opinion"))
                .unwrap_or(true),
            accuracy_threshold: as_f64(group_get("thresholds", "accuracy")),
            coherence_threshold: as_f64(group_get("thresholds", "coherence")).unwrap_or(7.0),
            style_threshold: as_f64(group_get("thresholds", "style")).unwrap_or(7.0),
            source_integrity_threshold: as_f64(group_get("thresholds", "source_integrity")),
            first_person: match first_person_raw.as_str() {
                "true" => FirstPerson::True,
                "situational" => FirstPerson::Situational,
                _ => FirstPerson::False,
            },
            passive_voice: match passive_voice_raw.as_str() {
                "allowed" => PassiveVoice::Allowed,
                "discouraged" => PassiveVoice::Discouraged,
                "prohibited" => PassiveVoice::Prohibited,
                _ => PassiveVoice::Moderate,
            },
            contractions: match contractions_raw.as_str() {
                "true" => Contractions::True,
                "situational" => Contractions::Situational,
                _ => Contractions::False,
            },
            prompt_instructions: as_str(get("prompt_instructions"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
        })
    }

    /// Parse a style profile from a YAML string.
    pub fn from_yaml_str(text: &str) -> CoreResult<StyleProfile> {
        let raw: serde_yaml::Value = serde_yaml::from_str(text)?;
        StyleProfile::from_yaml_doc(&raw)
    }
}

/// Lower-case and validate an enum-valued voice/pipeline field.
fn normalize_enum(
    label: &str,
    value: Option<String>,
    default: &str,
    allowed: &[&str],
) -> CoreResult<String> {
    let raw = value.unwrap_or_else(|| default.to_string());
    let lowered = raw.to_lowercase();
    if allowed.contains(&lowered.as_str()) {
        Ok(lowered)
    } else {
        Err(CoreError::parse(format!(
            "invalid value {raw:?} for field {label}; expected one of {allowed:?}"
        )))
    }
}

// ---------------------------------------------------------------------------
// Built-in profiles (embedded via include_str!)
// ---------------------------------------------------------------------------

/// The five style profiles shipped with the crate, as (name, yaml_text).
pub const BUILTIN_PROFILES: &[(&str, &str)] = &[
    ("academic", include_str!("../assets/styles/academic.yaml")),
    ("magazine", include_str!("../assets/styles/magazine.yaml")),
    (
        "personal-essay",
        include_str!("../assets/styles/personal-essay.yaml"),
    ),
    (
        "policy-brief",
        include_str!("../assets/styles/policy-brief.yaml"),
    ),
    ("technical", include_str!("../assets/styles/technical.yaml")),
];

/// Return the raw YAML text of a built-in profile by name.
pub fn load_profile_bytes(name: &str) -> Option<&'static str> {
    BUILTIN_PROFILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, yaml)| *yaml)
}

/// Parse every built-in profile.
///
/// Returns an error if any bundled YAML fails to parse, which would be a
/// build-time defect in the assets directory.
pub fn builtin_styles() -> CoreResult<Vec<StyleProfile>> {
    BUILTIN_PROFILES
        .iter()
        .map(|(name, yaml)| {
            StyleProfile::from_yaml_str(yaml)
                .map_err(|e| CoreError::parse(format!("builtin profile {name} is invalid: {e}")))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Filesystem loader
// ---------------------------------------------------------------------------

/// Default styles directory used when no override is supplied.
///
/// Mirrors the Python STYLES_DIR constant, but resolves relative to the current
/// working directory rather than the installed package location.
pub fn default_styles_dir() -> PathBuf {
    PathBuf::from("config").join("styles")
}

/// Load a single style profile by name from styles_dir.
///
/// Falls back to the built-in profiles when styles_dir is None and the named
/// profile is not present on disk.
pub fn load_profile(name: &str, styles_dir: Option<&Path>) -> CoreResult<StyleProfile> {
    let dir = styles_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(default_styles_dir);

    let path = dir.join(format!("{name}.yaml"));
    if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        return StyleProfile::from_yaml_str(&text);
    }

    if styles_dir.is_none() {
        if let Some(yaml) = load_profile_bytes(name) {
            return StyleProfile::from_yaml_str(yaml);
        }
    }

    Err(CoreError::profile_not_found(name))
}

/// Return the sorted list of available profile names (without extension).
///
/// Files whose stem starts with "_" are skipped, matching the Python loader.
/// With no styles_dir, falls back to the embedded built-in profile names.
pub fn list_profiles(styles_dir: Option<&Path>) -> Vec<String> {
    let dir = styles_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(default_styles_dir);

    let mut names: Vec<String> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| match entry {
                Ok(e) => {
                    let p = e.path();
                    let is_yaml = p
                        .extension()
                        .map(|ext| ext.eq_ignore_ascii_case("yaml"))
                        .unwrap_or(false);
                    if !is_yaml {
                        return None;
                    }
                    let stem = p.file_stem()?.to_str()?.to_string();
                    if stem.starts_with('_') {
                        return None;
                    }
                    Some(stem)
                }
                Err(_) => None,
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    if names.is_empty() && styles_dir.is_none() {
        names = BUILTIN_PROFILES.iter().map(|(n, _)| n.to_string()).collect();
    }

    names.sort();
    names
}

/// Load every profile in the styles directory, keyed by name.
pub fn load_all_profiles(
    styles_dir: Option<&Path>,
) -> CoreResult<BTreeMap<String, StyleProfile>> {
    let names = list_profiles(styles_dir);
    let mut out = BTreeMap::new();
    for name in names {
        let profile = load_profile(&name, styles_dir)?;
        out.insert(name, profile);
    }
    Ok(out)
}
