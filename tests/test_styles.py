"""Tests for autoessay.style — profile loading and data model."""

from __future__ import annotations

from pathlib import Path

import pytest

from autoessay.style import StyleProfile, list_profiles, load_all_profiles, load_profile

# ═══════════════════════════════════════════════════════════════════════════
# Helpers
# ═══════════════════════════════════════════════════════════════════════════

STYLES_DIR = Path(__file__).resolve().parent.parent / "config" / "styles"


# ═══════════════════════════════════════════════════════════════════════════
# Profile loading
# ═══════════════════════════════════════════════════════════════════════════


class TestLoadProfile:
    def test_load_academic(self):
        p = load_profile("academic", STYLES_DIR)
        assert p.name == "academic"
        assert p.citation_verification == "crossref"
        assert p.hallucination_gate == "strict"
        assert p.require_citation_per_claim is True
        assert p.allow_unsourced_opinion is False
        assert p.accuracy_threshold == 8.5
        assert p.first_person == "false"
        assert p.contractions == "false"
        assert len(p.prompt_instructions) > 100

    def test_load_magazine(self):
        p = load_profile("magazine", STYLES_DIR)
        assert p.name == "magazine"
        assert p.hallucination_gate == "lenient"
        assert p.first_person == "situational"
        assert p.style_threshold == 8.5  # style matters most for magazine
        assert "Show, don't tell" in p.prompt_instructions

    def test_load_technical(self):
        p = load_profile("technical", STYLES_DIR)
        assert p.name == "technical"
        assert p.hallucination_gate == "strict"
        assert p.require_citation_per_claim is True
        assert p.contractions == "false"
        assert p.accuracy_threshold == 8.5

    def test_load_personal_essay(self):
        p = load_profile("personal-essay", STYLES_DIR)
        assert p.name == "personal-essay"
        assert p.hallucination_gate == "off"
        assert p.hallucination_enabled is False
        assert p.accuracy_threshold is None  # personal essays don't score accuracy
        assert p.source_integrity_threshold is None
        assert p.first_person == "true"
        assert p.contractions == "true"
        assert p.allow_unsourced_opinion is True

    def test_load_policy_brief(self):
        p = load_profile("policy-brief", STYLES_DIR)
        assert p.name == "policy-brief"
        assert p.citation_verification == "crossref"
        assert p.coherence_threshold == 8.0  # policy needs strong logic
        assert "executive summary" in p.prompt_instructions.lower()

    def test_load_nonexistent_raises(self):
        with pytest.raises(FileNotFoundError):
            load_profile("nonexistent", STYLES_DIR)


class TestListProfiles:
    def test_lists_five_standard_profiles(self):
        names = list_profiles(STYLES_DIR)
        assert len(names) == 5
        assert "academic" in names
        assert "magazine" in names
        assert "technical" in names
        assert "personal-essay" in names
        assert "policy-brief" in names

    def test_skips_underscore_files(self):
        """Files starting with _ should be excluded (templates, private)."""
        # All our profiles start with letters, so list should be 5
        names = list_profiles(STYLES_DIR)
        assert all(not n.startswith("_") for n in names)


class TestLoadAllProfiles:
    def test_loads_all_five(self):
        all_p = load_all_profiles(STYLES_DIR)
        assert len(all_p) == 5
        assert all(isinstance(p, StyleProfile) for p in all_p.values())

    def test_every_profile_has_instructions(self):
        for p in load_all_profiles(STYLES_DIR).values():
            assert len(p.prompt_instructions) > 100, f"{p.name} has short/no instructions"


# ═══════════════════════════════════════════════════════════════════════════
# Data model behaviour
# ═══════════════════════════════════════════════════════════════════════════


class TestStyleProfileProperties:
    def test_hallucination_enabled_strict(self):
        p = load_profile("academic", STYLES_DIR)
        assert p.hallucination_enabled is True

    def test_hallucination_enabled_lenient(self):
        p = load_profile("magazine", STYLES_DIR)
        assert p.hallucination_enabled is True

    def test_hallucination_disabled_for_personal(self):
        p = load_profile("personal-essay", STYLES_DIR)
        assert p.hallucination_enabled is False

    def test_citations_required_academic(self):
        p = load_profile("academic", STYLES_DIR)
        assert p.citations_required is True

    def test_citations_not_required_personal(self):
        p = load_profile("personal-essay", STYLES_DIR)
        assert p.citations_required is False


# ═══════════════════════════════════════════════════════════════════════════
# Consistency — every profile has valid values
# ═══════════════════════════════════════════════════════════════════════════

class TestProfileConsistency:
    @pytest.mark.parametrize("name", list_profiles(STYLES_DIR))
    def test_valid_citation_verification(self, name):
        p = load_profile(name, STYLES_DIR)
        assert p.citation_verification in {"crossref", "web", "off"}

    @pytest.mark.parametrize("name", list_profiles(STYLES_DIR))
    def test_valid_hallucination_gate(self, name):
        p = load_profile(name, STYLES_DIR)
        assert p.hallucination_gate in {"strict", "lenient", "off"}

    @pytest.mark.parametrize("name", list_profiles(STYLES_DIR))
    def test_valid_voice_fields(self, name):
        p = load_profile(name, STYLES_DIR)
        assert p.first_person in {"true", "false", "situational"}
        assert p.passive_voice in {"allowed", "moderate", "discouraged", "prohibited"}
        assert p.contractions in {"true", "false", "situational"}

    @pytest.mark.parametrize("name", list_profiles(STYLES_DIR))
    def test_thresholds_in_range(self, name):
        p = load_profile(name, STYLES_DIR)
        for attr in ["accuracy_threshold", "coherence_threshold",
                      "style_threshold", "source_integrity_threshold"]:
            val = getattr(p, attr, None)
            if val is not None:
                assert 0 <= val <= 10, f"{name}.{attr} = {val} out of range"
