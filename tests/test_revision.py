"""Tests for autoessay.pipeline.revision — brief, rewrite, revise-until-pass loop."""

from __future__ import annotations

import json
from unittest.mock import Mock, patch

from autoessay.pipeline.evaluation import EvaluationResult
from autoessay.pipeline.revision import (
    generate_revision_brief,
    revise_section,
    revise_until_pass,
)
from autoessay.style import load_profile

MAGAZINE = load_profile("magazine")
RESEARCH = (
    "## Remote Work\n\n"
    "### Key Claims\n"
    "- Claim: Productivity rose 13% | Source: src_01\n\n"
    "### Sources\n"
    "- [src_01] Stanford WFH Study, 2023\n"
)

SECTION = (
    "Remote work didn't kill culture — it exposed how fragile it already was. "
    "According to a 2023 Stanford study, productivity rose 13% [src_01]."
)

FAILED_EVAL = EvaluationResult(
    accuracy=5.0,
    coherence=6.0,
    style=7.0,
    source_integrity=8.0,
    overall="fail",
    notes="Paragraph 1 is strong but needs more evidence. Lede is weak.",
)

PASSED_EVAL = EvaluationResult(
    accuracy=9.0,
    coherence=9.0,
    style=9.0,
    source_integrity=9.0,
    overall="pass",
    notes="Excellent section. Ready for publication.",
)

REVISION_BRIEF = (
    "1. [Paragraph 1] Strengthen the lede with a specific anecdote.\n"
    "2. [Evidence] Add one more data point to support the 13% claim.\n"
    "3. [Style] Reduce passive voice in the opening sentence."
)


# ═══════════════════════════════════════════════════════════════════════════
# generate_revision_brief
# ═══════════════════════════════════════════════════════════════════════════


class TestGenerateRevisionBrief:
    def test_returns_brief(self):
        with patch("autoessay.pipeline.revision.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = REVISION_BRIEF
            mock_get.return_value = mock_provider

            result = generate_revision_brief(SECTION, FAILED_EVAL, MAGAZINE)

        assert "Paragraph 1" in result

    def test_uses_revision_brief_role(self):
        with patch("autoessay.pipeline.revision.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "1. Fix X"
            mock_get.return_value = mock_provider

            generate_revision_brief(SECTION, FAILED_EVAL, MAGAZINE)

        assert mock_provider.chat_for_role.call_args[1]["role"] == "revision_brief"

    def test_user_prompt_includes_scores(self):
        with patch("autoessay.pipeline.revision.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "1. Fix X"
            mock_get.return_value = mock_provider

            generate_revision_brief(SECTION, FAILED_EVAL, MAGAZINE)

        messages = mock_provider.chat_for_role.call_args[0][0]
        prompt = messages[0]["content"]
        assert "5.0" in prompt  # accuracy score
        assert "6.0" in prompt  # coherence score

    def test_user_prompt_includes_notes(self):
        with patch("autoessay.pipeline.revision.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "1. Fix X"
            mock_get.return_value = mock_provider

            generate_revision_brief(SECTION, FAILED_EVAL, MAGAZINE)

        messages = mock_provider.chat_for_role.call_args[0][0]
        prompt = messages[0]["content"]
        assert "Lede is weak" in prompt


# ═══════════════════════════════════════════════════════════════════════════
# revise_section
# ═══════════════════════════════════════════════════════════════════════════


class TestReviseSection:
    def test_returns_revised_content(self):
        with patch("autoessay.pipeline.revision.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "Revised section text here."
            mock_get.return_value = mock_provider

            result = revise_section(SECTION, REVISION_BRIEF, RESEARCH, MAGAZINE)

        assert "Revised section" in result

    def test_uses_revision_role(self):
        with patch("autoessay.pipeline.revision.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            revise_section(SECTION, REVISION_BRIEF, RESEARCH, MAGAZINE)

        assert mock_provider.chat_for_role.call_args[1]["role"] == "revision"

    def test_user_prompt_includes_original_and_brief(self):
        with patch("autoessay.pipeline.revision.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            revise_section(SECTION, REVISION_BRIEF, RESEARCH, MAGAZINE)

        messages = mock_provider.chat_for_role.call_args[0][0]
        prompt = messages[0]["content"]
        assert "fragile" in prompt  # from original section
        assert "Paragraph 1" in prompt  # from revision brief

    def test_system_prompt_includes_style(self):
        with patch("autoessay.pipeline.revision.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = "ok"
            mock_get.return_value = mock_provider

            revise_section(SECTION, REVISION_BRIEF, RESEARCH, MAGAZINE)

        system = mock_provider.chat_for_role.call_args[1]["system"]
        assert "Show, don't tell" in system  # magazine style


# ═══════════════════════════════════════════════════════════════════════════
# revise_until_pass — the core loop
# ═══════════════════════════════════════════════════════════════════════════


class TestReviseUntilPass:
    def test_stops_immediately_if_already_passing(self):
        """If the section already passes, return immediately with one eval."""
        with patch("autoessay.pipeline.evaluation.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = json.dumps({
                "accuracy": 9.0, "coherence": 9.0, "style": 9.0, "source_integrity": 9.0,
                "overall": "pass", "hallucination_flags": [], "notes": "Good.",
            })
            mock_get.return_value = mock_provider

            final, history = revise_until_pass(SECTION, RESEARCH, MAGAZINE, max_cycles=3)

        assert len(history) == 1
        assert history[0].passed is True
        assert final == SECTION  # unchanged, already good

    def test_revises_until_pass(self):
        """Simulate: fail → brief → revise → pass in 2 cycles."""
        eval_responses = [
            # Cycle 1: fail
            json.dumps({
                "accuracy": 5.0, "coherence": 6.0, "style": 7.0, "source_integrity": 8.0,
                "overall": "fail", "hallucination_flags": [], "notes": "Weak lede.",
            }),
            # Cycle 2: pass
            json.dumps({
                "accuracy": 9.0, "coherence": 9.0, "style": 9.0, "source_integrity": 9.0,
                "overall": "pass", "hallucination_flags": [], "notes": "Much better.",
            }),
        ]

        with (
            patch("autoessay.pipeline.evaluation.get_provider") as mock_eval,
            patch("autoessay.pipeline.revision.get_provider") as mock_rev,
        ):
            mock_evaluator = Mock()
            mock_evaluator.chat_for_role.side_effect = [
                Mock(content=eval_responses[0]),
                Mock(content=eval_responses[1]),
            ]
            mock_eval.return_value = mock_evaluator

            mock_reviser = Mock()
            mock_reviser.chat_for_role.return_value.content = "Revised section text."
            mock_rev.return_value = mock_reviser

            final, history = revise_until_pass(SECTION, RESEARCH, MAGAZINE, max_cycles=3)

        assert len(history) == 2
        assert history[0].passed is False
        assert history[1].passed is True
        assert final == "Revised section text."

    def test_gives_up_after_max_cycles(self):
        """If the section never passes, stop at max_cycles and return best effort."""
        eval_response = json.dumps({
            "accuracy": 5.0, "coherence": 5.0, "style": 5.0, "source_integrity": 5.0,
            "overall": "fail", "hallucination_flags": [], "notes": "Still weak.",
        })

        with (
            patch("autoessay.pipeline.evaluation.get_provider") as mock_eval,
            patch("autoessay.pipeline.revision.get_provider") as mock_rev,
        ):
            mock_evaluator = Mock()
            mock_evaluator.chat_for_role.return_value.content = eval_response
            mock_eval.return_value = mock_evaluator

            mock_reviser = Mock()
            mock_reviser.chat_for_role.return_value.content = "Revised v2."
            mock_rev.return_value = mock_reviser

            final, history = revise_until_pass(SECTION, RESEARCH, MAGAZINE, max_cycles=2)

        assert len(history) == 2
        assert all(not h.passed for h in history)
        # Should have the last revised version
        assert final == "Revised v2."

    def test_exclude_provider_forwarded(self):
        """Anti-self-review: exclude_provider is passed through to evaluation."""
        with patch("autoessay.pipeline.evaluation.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = json.dumps({
                "accuracy": 9.0, "coherence": 9.0, "style": 9.0, "source_integrity": 9.0,
                "overall": "pass", "hallucination_flags": [], "notes": "Good.",
            })
            mock_get.return_value = mock_provider

            revise_until_pass(
                SECTION, RESEARCH, MAGAZINE,
                max_cycles=3, exclude_provider="deepseek",
            )

        assert mock_provider.chat_for_role.call_args[1]["exclude"] == "deepseek"
