"""Style profile data model."""

from dataclasses import dataclass, field


@dataclass
class StyleProfile:
    name: str
    use_case: str
    key_traits: list[str] = field(default_factory=list)

    # Pipeline behavior (from profile behavior matrix)
    citation_verification: str = "web"  # crossref / web / off
    source_deduplication: bool = True
    hallucination_gate: str = "lenient"  # strict / lenient / off
    require_citation_per_claim: bool = False
    allow_unsourced_opinion: bool = True
    accuracy_threshold: float | None = 7.0

    # Voice constraints
    first_person: bool = False
    passive_voice: str = "moderate"  # allowed / moderate / discouraged / prohibited
    contractions: bool = False

    # Quality thresholds
    coherence_threshold: float = 7.0
    style_threshold: float = 7.0
    source_integrity_threshold: float = 7.0
