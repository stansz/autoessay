"""Tests for autoessay.pipeline — research and outline phases."""

from __future__ import annotations

from unittest.mock import Mock, patch

from autoessay.pipeline import generate_outline, generate_research
from autoessay.style import load_profile

# ═══════════════════════════════════════════════════════════════════════════
# Research
# ═══════════════════════════════════════════════════════════════════════════


class TestGenerateResearch:
    def test_returns_markdown(self):
        with patch("autoessay.pipeline.research.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = (
                "## Research\n\n### Key Claims\n- Claim: Test"
            )
            mock_get.return_value = mock_provider

            result = generate_research("climate policy")

        assert "## Research" in result
        assert "Key Claims" in result

    def test_passes_topic_to_llm(self):
        with patch("autoessay.pipeline.research.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_research("quantum computing advances")

        # messages is the first positional arg
        call = mock_provider.chat_for_role.call_args
        messages = call[0][0]  # first positional argument
        assert any("quantum computing" in m["content"] for m in messages)

    def test_uses_smart_tier(self):
        with patch("autoessay.pipeline.research.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_research("test topic")

        assert mock_provider.chat_for_role.call_args[1]["role"] == "research"

    def test_includes_seed_in_prompt(self):
        with patch("autoessay.pipeline.research.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_research("AI safety", seed="Focus on alignment research from 2024-2025")

        messages = mock_provider.chat_for_role.call_args[0][0]
        assert any("alignment research" in m["content"] for m in messages)
        assert any("2024-2025" in m["content"] for m in messages)

    def test_injects_style_profile(self):
        magazine = load_profile("magazine")
        with patch("autoessay.pipeline.research.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_research("urban design", style=magazine)

        system = mock_provider.chat_for_role.call_args[1]["system"]
        assert "Show, don't tell" in system or "magazine" in system.lower()

    def test_low_temperature_for_research(self):
        with patch("autoessay.pipeline.research.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_research("test")

        assert mock_provider.chat_for_role.call_args[1]["temperature"] == 0.4

    def test_falls_back_to_default_program_when_file_missing(self):
        with patch(
            "autoessay.pipeline.research._load_program_section", return_value=""
        ), patch("autoessay.pipeline.research.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_research("test")

        system = mock_provider.chat_for_role.call_args[1]["system"]
        assert "Key Claims" in system


# ═══════════════════════════════════════════════════════════════════════════
# Outline
# ═══════════════════════════════════════════════════════════════════════════


class TestGenerateOutline:
    def test_returns_markdown(self):
        with patch("autoessay.pipeline.outline.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "## Thesis\n\nTest thesis."
            mock_get.return_value = mock_provider

            result = generate_outline("## Research\n\nClaims here.")

        assert "## Thesis" in result

    def test_passes_research_notes_to_llm(self):
        research = "## Research\n\n### Key Claims\n- Claim: Water is wet"
        with patch("autoessay.pipeline.outline.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_outline(research)

        messages = mock_provider.chat_for_role.call_args[0][0]
        assert any("Water is wet" in m["content"] for m in messages)

    def test_uses_outline_role(self):
        with patch("autoessay.pipeline.outline.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_outline("research notes")

        assert mock_provider.chat_for_role.call_args[1]["role"] == "outline"

    def test_injects_style_context(self):
        academic = load_profile("academic")
        with patch("autoessay.pipeline.outline.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_outline("research", style=academic)

        system = mock_provider.chat_for_role.call_args[1]["system"]
        assert "academic" in system.lower() or "Academic" in system

    def test_falls_back_to_default_program(self):
        with patch(
            "autoessay.pipeline.outline._load_program_section", return_value=""
        ), patch("autoessay.pipeline.outline.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            generate_outline("research")

        system = mock_provider.chat_for_role.call_args[1]["system"]
        assert "Thesis" in system


# ═══════════════════════════════════════════════════════════════════════════
# Integration — research → outline chain
# ═══════════════════════════════════════════════════════════════════════════


class TestResearchOutlineChain:
    def test_chain_research_to_outline(self):
        mock_research_output = (
            "## Climate Policy\n\n"
            "### Key Claims\n"
            "- Claim: Carbon pricing reduces emissions | Source: src_01 | Confidence: high\n\n"
            "### Sources\n"
            "- [src_01] IMF Report, 2024, https://..."
        )

        with (
            patch("autoessay.pipeline.research.get_provider") as mock_research_prov,
            patch("autoessay.pipeline.outline.get_provider") as mock_outline_prov,
        ):
            mock_rp = Mock()
            mock_rp.chat_for_role.return_value.content = mock_research_output
            mock_research_prov.return_value = mock_rp

            mock_op = Mock()
            mock_op.chat_for_role.return_value.content = "## Thesis\n\nCarbon pricing works."
            mock_outline_prov.return_value = mock_op

            research = generate_research("carbon pricing effectiveness")
            outline = generate_outline(research)

        assert "Carbon pricing" in outline
        # Outline should have received the research output as first positional arg
        outline_messages = mock_op.chat_for_role.call_args[0][0]
        assert any("IMF Report" in m["content"] for m in outline_messages)
