//! Provider configuration and registry — port of ProviderRegistry in
//! autoessay/src/autoessay/provider.py.
//!
//! Responsibilities:
//! - load providers.yaml (from an explicit path, $AUTOESSAY_CONFIG, the CWD,
//!   or the embedded default),
//! - infer api_type (Anthropic iff the provider is named "anthropic"),
//! - detect which providers have API keys available,
//! - map pipeline roles to tiers,
//! - resolve a (provider, model) pair for a tier, honouring prefer/exclude.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{ProviderError, ProviderResult};
use crate::types::Tier;

/// Default provider configuration, embedded in the binary via include_str!.
///
/// This is a verbatim copy of autoessay/config/providers.yaml.
pub const DEFAULT_PROVIDERS_YAML: &str = include_str!("../config/providers.yaml");

/// Which wire protocol a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiType {
    /// OpenAI-compatible /chat/completions (DeepSeek, OpenRouter, Z.ai, ...).
    OpenAi,
    /// Native Anthropic Messages API (/messages).
    Anthropic,
}

/// Definition of a single LLM provider.
///
/// Port of the Python ProviderConfig dataclass. Fields are public so callers
/// (and tests) can inspect them exactly as they did in Python.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    /// Provider name, e.g. "deepseek".
    pub name: String,
    /// API root, e.g. "https://api.deepseek.com/v1".
    pub base_url: String,
    /// Environment variable holding the API key.
    pub env_key: String,
    /// Tier -> model id, e.g. {"fast": "deepseek-v4-flash", ...}.
    pub models: HashMap<String, String>,
    /// Inferred from the name: Anthropic iff name == "anthropic".
    pub api_type: ApiType,
}

/// A YAML document describing providers and role->tier mappings.
#[derive(Debug, Clone, serde::Deserialize)]
struct RawConfig {
    /// A document without a providers key yields the Python error
    /// "Invalid providers.yaml: missing 'providers' key".
    #[serde(default)]
    providers: HashMap<String, RawProvider>,
    #[serde(default)]
    roles: HashMap<String, String>,
}

/// One entry of the providers map in providers.yaml.
#[derive(Debug, Clone, serde::Deserialize)]
struct RawProvider {
    base_url: String,
    env_key: String,
    models: HashMap<String, String>,
}

impl ProviderConfig {
    /// Pick the model id for a tier, falling back to the provider's first
    /// configured model when the tier is not listed.
    ///
    /// Port of cfg.models.get(tier, next(iter(cfg.models.values()))).
    pub fn model_for_tier(&self, tier: Tier) -> ProviderResult<String> {
        if let Some(model) = self.models.get(tier.as_str()) {
            return Ok(model.clone());
        }
        // Python falls back to the first value in dict order; serde_yaml feeds
        // a plain HashMap here, so fall back to any entry in a deterministic
        // (sorted) order instead.
        let mut keys: Vec<&String> = self.models.keys().collect();
        keys.sort();
        match keys.first() {
            Some(key) => match self.models.get(*key) {
                Some(model) => Ok(model.clone()),
                None => Err(ProviderError::ConfigInvalid(format!(
                    "provider '{}' has no models configured",
                    self.name
                ))),
            },
            None => Err(ProviderError::ConfigInvalid(format!(
                "provider '{}' has an empty 'models' map",
                self.name
            ))),
        }
    }
}

/// Loads provider definitions from YAML and detects available providers.
///
/// Port of the Python ProviderRegistry. The Python constructor takes an
/// optional _env dict for testability; that is the primary constructor here
/// (ProviderRegistry::from_env), and the process-environment variants wrap it.
#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    /// Path the configuration was loaded from (or the pseudo-path
    /// "<embedded:config/providers.yaml>" for the built-in default).
    pub config_path: PathBuf,
    /// Provider name -> definition.
    pub providers: HashMap<String, ProviderConfig>,
    /// role name -> tier name ("fast" / "smart").
    roles: HashMap<String, String>,
    /// Environment used for API-key detection (testable: never the real env
    /// unless asked for).
    env: HashMap<String, String>,
}

impl ProviderRegistry {
    /// Load the registry from an explicit YAML file, injecting the environment
    /// used for API-key detection.
    ///
    /// Equivalent to ProviderRegistry(config_path, _env=env) in Python.
    pub fn from_env_with_path(
        config_path: impl AsRef<Path>,
        env: HashMap<String, String>,
    ) -> ProviderResult<Self> {
        let config_path = config_path.as_ref();
        let text = std::fs::read_to_string(config_path).map_err(|source| {
            ProviderError::ConfigIo {
                path: config_path.display().to_string(),
                source,
            }
        })?;
        Self::from_yaml_str(&text, env, config_path.to_path_buf())
    }

    /// Load the embedded default providers.yaml with a test-local environment.
    ///
    /// Mirrors the Python testability hook (_env=...) without touching process
    /// env or the filesystem.
    pub fn from_env(env: HashMap<String, String>) -> ProviderResult<Self> {
        Self::from_yaml_str(DEFAULT_PROVIDERS_YAML, env, embedded_path())
    }

    /// Parse a providers.yaml document from memory.
    pub fn from_yaml_str(
        yaml_text: &str,
        env: HashMap<String, String>,
        config_path: PathBuf,
    ) -> ProviderResult<Self> {
        let raw: RawConfig = serde_yaml::from_str(yaml_text).map_err(|source| {
            ProviderError::ConfigYaml {
                path: config_path.display().to_string(),
                source,
            }
        })?;
        Self::from_raw(raw, env, config_path)
    }

    /// Load using the real process environment and config discovery
    /// ($AUTOESSAY_CONFIG, ./config/providers.yaml, then the embedded default).
    pub fn load() -> ProviderResult<Self> {
        Self::load_with_env(process_env())
    }

    /// Load using config discovery but an injected environment.
    pub fn load_with_env(env: HashMap<String, String>) -> ProviderResult<Self> {
        match Self::find_config()? {
            ConfigSource::Path(path) => Self::from_env_with_path(path, env),
            ConfigSource::Embedded => {
                Self::from_yaml_str(DEFAULT_PROVIDERS_YAML, env, embedded_path())
            }
        }
    }

    /// Locate providers.yaml via env var, CWD, or fall back to the embedded copy.
    ///
    /// Port of _find_config(), except that a missing file on disk is no longer
    /// fatal: the shipped default is embedded in the binary.
    fn find_config() -> ProviderResult<ConfigSource> {
        if let Ok(path) = std::env::var("AUTOESSAY_CONFIG") {
            return Ok(ConfigSource::Path(PathBuf::from(path)));
        }
        for candidate in [
            PathBuf::from("config/providers.yaml"),
            package_relative_config(),
        ] {
            if candidate.exists() {
                return Ok(ConfigSource::Path(candidate));
            }
        }
        Ok(ConfigSource::Embedded)
    }

    /// Build the registry from a parsed YAML document.
    fn from_raw(
        raw: RawConfig,
        env: HashMap<String, String>,
        config_path: PathBuf,
    ) -> ProviderResult<Self> {
        if raw.providers.is_empty() {
            return Err(ProviderError::ConfigInvalid("missing 'providers' key".into()));
        }

        let mut providers = HashMap::new();
        for (name, cfg) in raw.providers {
            let api_type = if name == "anthropic" {
                ApiType::Anthropic
            } else {
                ApiType::OpenAi
            };
            providers.insert(
                name.clone(),
                ProviderConfig {
                    name,
                    base_url: cfg.base_url,
                    env_key: cfg.env_key,
                    models: cfg.models,
                    api_type,
                },
            );
        }

        Ok(ProviderRegistry {
            config_path,
            providers,
            roles: raw.roles,
            env,
        })
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Provider names with API keys set in the environment.
    ///
    /// Port of the Python available property; the result is sorted so "the
    /// first available provider" is deterministic across runs.
    pub fn available(&self) -> Vec<String> {
        let mut names: Vec<&String> = self.providers.keys().collect();
        names.sort();
        names
            .into_iter()
            .filter(|name| {
                self.providers
                    .get(*name)
                    .and_then(|cfg| self.env.get(&cfg.env_key))
                    .is_some_and(|value| !value.is_empty())
            })
            .cloned()
            .collect()
    }

    /// Look up a provider by name.
    ///
    /// Port of the Python get(); the error text matches the KeyError message.
    pub fn get(&self, name: &str) -> ProviderResult<ProviderConfig> {
        match self.providers.get(name) {
            Some(cfg) => Ok(cfg.clone()),
            None => {
                let mut available: Vec<&String> = self.providers.keys().collect();
                available.sort();
                let available: Vec<&str> = available.iter().map(|s| s.as_str()).collect();
                Err(ProviderError::UnknownProvider {
                    name: name.to_string(),
                    available: format!("{available:?}"),
                })
            }
        }
    }

    /// Return the tier ("fast" / "smart") configured for a pipeline role
    /// ("drafting", "research", ...); unknown roles default to fast.
    pub fn tier_for_role(&self, role: &str) -> Tier {
        self.roles
            .get(role)
            .and_then(|tier| tier.parse::<Tier>().ok())
            .unwrap_or(Tier::Fast)
    }

    /// The raw role -> tier map from providers.yaml.
    pub fn roles(&self) -> &HashMap<String, String> {
        &self.roles
    }

    /// The value of an environment variable in the registry's environment.
    pub fn env_var(&self, key: &str) -> Option<&str> {
        self.env.get(key).map(String::as_str)
    }

    /// Select an available provider + model for the given tier.
    ///
    /// Port of resolve_provider() with identical semantics:
    /// - the candidate list is every available provider minus exclude,
    /// - prefer wins when it is available, otherwise the first available,
    /// - model is models[tier], falling back to the provider's first model,
    /// - two distinct error message shapes when nothing is left (see
    ///   ProviderError::NoProvider).
    ///
    /// Returns (ProviderConfig, model_id).
    pub fn resolve_provider(
        &self,
        tier: Tier,
        prefer: Option<&str>,
        exclude: Option<&str>,
    ) -> ProviderResult<(ProviderConfig, String)> {
        let available: Vec<String> = self
            .available()
            .into_iter()
            .filter(|name| Some(name.as_str()) != exclude)
            .collect();

        if available.is_empty() {
            return Err(ProviderError::NoProvider {
                tier,
                excluded: exclude.map(str::to_string),
            });
        }

        // User-specified preference wins when it survived the filter.
        let name = match prefer {
            Some(preferred) if available.iter().any(|n| n == preferred) => preferred.to_string(),
            _ => available[0].clone(),
        };

        let cfg = self.get(&name)?;
        let model = cfg.model_for_tier(tier)?;
        Ok((cfg, model))
    }
}

/// Where the configuration came from.
enum ConfigSource {
    Path(PathBuf),
    Embedded,
}

/// Pseudo-path recorded for the embedded default config.
fn embedded_path() -> PathBuf {
    PathBuf::from("<embedded:config/providers.yaml>")
}

/// The config/providers.yaml next to the crate, when running from the repo.
fn package_relative_config() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("config")
        .join("providers.yaml")
}

/// Snapshot of the process environment.
fn process_env() -> HashMap<String, String> {
    std::env::vars().collect()
}
