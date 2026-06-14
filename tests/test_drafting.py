"""Tests for autoessay.pipeline.drafting — section drafting with style injection."""

from __future__ import annotations

from unittest.mock import Mock, patch

from autoessay.pipeline.drafting import _build_system_prompt, _build_user_prompt, draft_section
from autoessay.style import load_profile

MAGAZINE = load_profile("magazine")
ACADEMIC = load_profile("academic")
RESEARCH = (
    "## Remote Work\n\n"
    "### Key Claims\n"
    "- Claim: Remote work increased productivity by 13% | Source: src_01\n"
    "- Claim: Company culture degraded in async-first teams | Source: src_02\n\n"
    "### Sources\n"
    "- [src_01] Stanford WFH Study, 2023\n"
    "- [src_02] Harvard Business Review, 2024\n"
)


# ═══════════════════════════════════════════════════════════════════════════
# draft_section
# ═══════════════════════════════════════════════════════════════════════════


class TestDraftSection:
    def test_returns_content(self):
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = (
                "The pandemic didn't kill the office — it just showed us "
                "how little of it we actually needed."
            )
            mock_get.return_value = mock_provider

            result = draft_section(
                "The Rise of Remote Work",
                "Set the scene with data on remote work adoption.",
                RESEARCH,
                MAGAZINE,
            )

        assert "pandemic" in result.lower() or "office" in result.lower()

    def test_uses_drafting_role(self):
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            draft_section("Title", "Purpose", RESEARCH, MAGAZINE)

        assert mock_provider.chat_for_role.call_args[1]["role"] == "drafting"

    def test_uses_fast_tier_via_role(self):
        """Drafting role maps to fast tier."""
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            draft_section("Title", "Purpose", RESEARCH, MAGAZINE)

        # chat_for_role resolves "drafting" → fast tier
        # We just verify it doesn't crash; tier resolution tested in provider tests

    def test_injects_style_instructions(self):
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            draft_section("T", "P", RESEARCH, MAGAZINE)

        system = mock_provider.chat_for_role.call_args[1]["system"]
        assert "Show, don't tell" in system  # magazine's instruction

    def test_injects_academic_style(self):
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            draft_section("T", "P", RESEARCH, ACADEMIC)

        system = mock_provider.chat_for_role.call_args[1]["system"]
        assert "formal, impersonal register" in system.lower()

    def test_user_prompt_includes_section_title_and_purpose(self):
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            draft_section(
                "The Hybrid Compromise",
                "Argue that hybrid models are the worst of both worlds.",
                RESEARCH,
                MAGAZINE,
            )

        messages = mock_provider.chat_for_role.call_args[0][0]
        prompt = messages[0]["content"]
        assert "The Hybrid Compromise" in prompt
        assert "worst of both worlds" in prompt

    def test_user_prompt_includes_research(self):
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            draft_section("T", "P", RESEARCH, MAGAZINE)

        messages = mock_provider.chat_for_role.call_args[0][0]
        prompt = messages[0]["content"]
        assert "Stanford WFH Study" in prompt

    def test_user_prompt_includes_sources(self):
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            draft_section(
                "T", "P", RESEARCH, MAGAZINE,
                sources=["src_01", "src_02"],
            )

        messages = mock_provider.chat_for_role.call_args[0][0]
        prompt = messages[0]["content"]
        assert "src_01" in prompt
        assert "src_02" in prompt

    def test_user_prompt_includes_previous_sections(self):
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            prev = [
                "Section 1 was about the rise of remote work.",
                "Section 2 covered the productivity data.",
            ]
            draft_section("T", "P", RESEARCH, MAGAZINE, previous_sections=prev)

        messages = mock_provider.chat_for_role.call_args[0][0]
        prompt = messages[0]["content"]
        assert "Section 1 was about" in prompt
        assert "Section 2 covered" in prompt

    def test_drafting_temperature_is_creative(self):
        """Drafting should use higher temperature than research for creative variety."""
        with patch("autoessay.pipeline.drafting.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            draft_section("T", "P", RESEARCH, MAGAZINE)

        assert mock_provider.chat_for_role.call_args[1]["temperature"] == 0.7


# ═══════════════════════════════════════════════════════════════════════════
# Prompt assembly
# ═══════════════════════════════════════════════════════════════════════════


class TestBuildSystemPrompt:
    def test_includes_style_instructions(self):
        prompt = _build_system_prompt(MAGAZINE)
        assert "Show, don't tell" in prompt

    def test_includes_anti_slop_when_available(self):
        prompt = _build_system_prompt(MAGAZINE)
        # ANTI-SLOP.md contains these prohibited phrases
        assert "delve into" in prompt.lower()
        assert "Writing Rules" in prompt


class TestBuildUserPrompt:
    def test_includes_title_as_heading(self):
        prompt = _build_user_prompt("The Title", "Purpose here", RESEARCH)
        assert "## The Title" in prompt

    def test_no_sources_section_when_empty(self):
        prompt = _build_user_prompt("T", "P", RESEARCH)
        assert "Use these sources" not in prompt

    def test_no_previous_sections_when_empty(self):
        prompt = _build_user_prompt("T", "P", RESEARCH)
        assert "Previous Sections" not in prompt

    def test_write_only_instruction_appended(self):
        prompt = _build_user_prompt("T", "P", RESEARCH)
        assert "Write only the section content" in prompt
