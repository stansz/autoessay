"""Style profile data model and YAML loader."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml

# ═══════════════════════════════════════════════════════════════════════════
# Data model
# ═══════════════════════════════════════════════════════════════════════════


@dataclass
class StyleProfile:
    """A writing style profile — loaded from YAML, injected into LLM prompts.

    Mirrors the structure of ``config/styles/<name>.yaml``.
    """

    name: str
    use_case: str
    key_traits: list[str] = field(default_factory=list)

    # Pipeline behaviour
    citation_verification: str = "web"  # crossref / web / off
    source_deduplication: bool = True
    hallucination_gate: str = "lenient"  # strict / lenient / off
    require_citation_per_claim: bool = False
    allow_unsourced_opinion: bool = True

    # Quality thresholds (1-10, nullable — personal essays skip some)
    accuracy_threshold: float | None = 7.0
    coherence_threshold: float = 7.0
    style_threshold: float = 7.0
    source_integrity_threshold: float | None = 7.0

    # Voice constraints
    first_person: str = "false"  # true / false / situational
    passive_voice: str = "moderate"  # allowed / moderate / discouraged / prohibited
    contractions: str = "false"  # true / false / situational

    # The prompt instructions injected into drafting calls
    prompt_instructions: str = ""

    @property
    def hallucination_enabled(self) -> bool:
        """Whether hallucination detection should run for this profile."""
        return self.hallucination_gate != "off"

    @property
    def citations_required(self) -> bool:
        """Whether every claim must have a source citation."""
        return self.require_citation_per_claim


# ═══════════════════════════════════════════════════════════════════════════
# YAML loader
# ═══════════════════════════════════════════════════════════════════════════

STYLES_DIR = (
    Path(__file__).resolve().parent.parent.parent.parent / "config" / "styles"
)


def _load_yaml(path: Path) -> dict[str, Any]:
    """Load a YAML file, returning an empty dict on missing file."""
    if not path.exists():
        raise FileNotFoundError(f"Style profile not found: {path}")
    return yaml.safe_load(path.read_text()) or {}


def load_profile(name: str, styles_dir: Path | None = None) -> StyleProfile:
    """Load a single style profile by name.

    Args:
        name: Profile name without extension (e.g. ``"magazine"``).
        styles_dir: Override the default ``config/styles/`` directory.

    Returns:
        A populated ``StyleProfile``.

    Raises:
        FileNotFoundError: The profile YAML file does not exist.
    """
    path = (styles_dir or STYLES_DIR) / f"{name}.yaml"
    raw = _load_yaml(path)
    return _from_dict(raw)


def list_profiles(styles_dir: Path | None = None) -> list[str]:
    """Return sorted list of available profile names (without extension)."""
    d = styles_dir or STYLES_DIR
    if not d.exists():
        return []
    return sorted(
        p.stem for p in d.glob("*.yaml")
        if not p.name.startswith("_")  # skip private / template files
    )


def load_all_profiles(styles_dir: Path | None = None) -> dict[str, StyleProfile]:
    """Load every profile in the styles directory.

    Returns:
        ``{name: StyleProfile, ...}``
    """
    return {name: load_profile(name, styles_dir) for name in list_profiles(styles_dir)}


# ═══════════════════════════════════════════════════════════════════════════
# Internal
# ═══════════════════════════════════════════════════════════════════════════


def _from_dict(raw: dict[str, Any]) -> StyleProfile:
    """Build a StyleProfile from the YAML dict."""
    pipeline = raw.get("pipeline", {})
    thresholds = raw.get("thresholds", {})
    voice = raw.get("voice", {})

    return StyleProfile(
        name=raw["name"],
        use_case=raw.get("use_case", ""),
        key_traits=raw.get("key_traits", []),
        # Pipeline
        citation_verification=pipeline.get("citation_verification", "web"),
        source_deduplication=pipeline.get("source_deduplication", True),
        hallucination_gate=pipeline.get("hallucination_gate", "lenient"),
        require_citation_per_claim=pipeline.get("require_citation_per_claim", False),
        allow_unsourced_opinion=pipeline.get("allow_unsourced_opinion", True),
        # Thresholds
        accuracy_threshold=thresholds.get("accuracy"),
        coherence_threshold=thresholds.get("coherence", 7.0),
        style_threshold=thresholds.get("style", 7.0),
        source_integrity_threshold=thresholds.get("source_integrity"),
        # Voice
        first_person=voice.get("first_person", "false"),
        passive_voice=voice.get("passive_voice", "moderate"),
        contractions=voice.get("contractions", "false"),
        # Prompt
        prompt_instructions=raw.get("prompt_instructions", "").strip(),
    )
