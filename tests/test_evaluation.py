"""Tests for autoessay.pipeline.evaluation — section scoring and hallucination detection."""

from __future__ import annotations

import json
from unittest.mock import Mock, patch

from autoessay.pipeline.evaluation import (
    EvaluationResult,
    HallucinationFlag,
    _build_eval_system_prompt,
    _build_eval_user_prompt,
    _extract_json,
    _parse_eval_response,
    evaluate_section,
)
from autoessay.style import load_profile

MAGAZINE = load_profile("magazine")
ACADEMIC = load_profile("academic")
PERSONAL = load_profile("personal-essay")

SECTION = (
    "Remote work didn't kill culture — it exposed how fragile it already was. "
    "According to a 2023 Stanford study, productivity rose 13% among remote workers, "
    "but the same study found that collaboration networks shrank by 25% [src_01]. "
    "The watercooler, it turns out, wasn't just a cliché."
)

RESEARCH = (
    "## Remote Work\n\n"
    "### Key Claims\n"
    "- Claim: Remote work increased productivity by 13% | Source: src_01\n"
    "- Claim: Collaboration networks shrank by 25% | Source: src_01\n\n"
    "### Sources\n"
    "- [src_01] Stanford WFH Study, 2023\n"
)

EVAL_JSON = {
    "accuracy": 8.5,
    "coherence": 8.0,
    "style": 9.0,
    "source_integrity": 8.0,
    "overall": "pass",
    "hallucination_flags": [],
    "notes": "Strong opening. Paragraph 2 needs a smoother transition.",
}


# ═══════════════════════════════════════════════════════════════════════════
# evaluate_section
# ═══════════════════════════════════════════════════════════════════════════


class TestEvaluateSection:
    def test_returns_evaluation_result(self):
        with patch("autoessay.pipeline.evaluation.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = json.dumps(EVAL_JSON)
            mock_get.return_value = mock_provider

            result = evaluate_section(SECTION, RESEARCH, MAGAZINE)

        assert isinstance(result, EvaluationResult)
        assert result.passed is True
        assert result.accuracy == 8.5

    def test_uses_evaluation_role(self):
        with patch("autoessay.pipeline.evaluation.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = json.dumps(EVAL_JSON)
            mock_get.return_value = mock_provider

            evaluate_section(SECTION, RESEARCH, MAGAZINE)

        assert mock_provider.chat_for_role.call_args[1]["role"] == "evaluation"

    def test_passes_exclude_provider(self):
        """Anti-self-review: exclude_provider is forwarded to the LLM call."""
        with patch("autoessay.pipeline.evaluation.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = json.dumps(EVAL_JSON)
            mock_get.return_value = mock_provider

            evaluate_section(SECTION, RESEARCH, MAGAZINE, exclude_provider="anthropic")

        assert mock_provider.chat_for_role.call_args[1]["exclude"] == "anthropic"

    def test_low_temperature(self):
        with patch("autoessay.pipeline.evaluation.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = json.dumps(EVAL_JSON)
            mock_get.return_value = mock_provider

            evaluate_section(SECTION, RESEARCH, MAGAZINE)

        assert mock_provider.chat_for_role.call_args[1]["temperature"] == 0.3

    def test_system_prompt_includes_thresholds(self):
        with patch("autoessay.pipeline.evaluation.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = json.dumps(EVAL_JSON)
            mock_get.return_value = mock_provider

            evaluate_section(SECTION, RESEARCH, MAGAZINE)

        system = mock_provider.chat_for_role.call_args[1]["system"]
        assert "Quality Thresholds" in system
        assert "magazine" in system.lower() or "Magazine" in system

    def test_user_prompt_includes_section_and_research(self):
        with patch("autoessay.pipeline.evaluation.get_provider") as mock_get:
            mock_provider = Mock()
            mock_provider.chat_for_role.return_value.content = json.dumps(EVAL_JSON)
            mock_get.return_value = mock_provider

            evaluate_section(SECTION, RESEARCH, MAGAZINE)

        messages = mock_provider.chat_for_role.call_args[0][0]
        prompt = messages[0]["content"]
        assert "Stanford study" in prompt
        assert "Section to Evaluate" in prompt
        assert "Research Notes" in prompt


# ═══════════════════════════════════════════════════════════════════════════
# Threshold enforcement
# ═══════════════════════════════════════════════════════════════════════════


class TestThresholdEnforcement:
    def test_fails_below_accuracy_threshold(self):
        data = dict(EVAL_JSON, accuracy=6.0)  # below magazine's 7.0
        result = _parse_eval_response(json.dumps(data), MAGAZINE)
        assert result.passed is False
        assert result.accuracy == 6.0

    def test_fails_below_coherence_threshold(self):
        data = dict(EVAL_JSON, coherence=4.0, accuracy=9.0, style=9.0, source_integrity=9.0)
        result = _parse_eval_response(json.dumps(data), MAGAZINE)
        assert result.passed is False

    def test_passes_when_all_above_thresholds(self):
        data = dict(EVAL_JSON, accuracy=9.0, coherence=9.0, style=9.0, source_integrity=9.0)
        result = _parse_eval_response(json.dumps(data), MAGAZINE)
        assert result.passed is True

    def test_personal_essay_skips_accuracy_check(self):
        """Personal essays have accuracy_threshold=None — always pass accuracy."""
        data = dict(EVAL_JSON, accuracy=1.0)  # would fail for magazine
        result = _parse_eval_response(json.dumps(data), PERSONAL)
        # accuracy is not checked for personal-essay
        assert result.accuracy == 1.0
        # But coherence still checked (6.0 threshold, score is 7.0 → pass)
        assert result.passed is True


# ═══════════════════════════════════════════════════════════════════════════
# Hallucination gate
# ═══════════════════════════════════════════════════════════════════════════


class TestHallucinationGate:
    def test_strict_gate_critical_flag_auto_fails(self):
        data = {
            **EVAL_JSON,
            "accuracy": 9.0, "coherence": 9.0, "style": 9.0, "source_integrity": 9.0,
            "hallucination_flags": [
                {"claim": "Made-up stat", "severity": "critical", "note": "Not in sources"}
            ],
        }
        result = _parse_eval_response(json.dumps(data), ACADEMIC)  # strict gate
        assert result.passed is False
        assert len(result.hallucination_flags) == 1

    def test_lenient_gate_single_critical_passes(self):
        data = {
            **EVAL_JSON,
            "hallucination_flags": [
                {"claim": "Minor issue", "severity": "critical", "note": "Borderline"}
            ],
        }
        result = _parse_eval_response(json.dumps(data), MAGAZINE)  # lenient gate
        assert result.passed is True  # single critical → still passes in lenient

    def test_lenient_gate_two_critical_fails(self):
        data = {
            **EVAL_JSON,
            "hallucination_flags": [
                {"claim": "Issue 1", "severity": "critical", "note": "a"},
                {"claim": "Issue 2", "severity": "critical", "note": "b"},
            ],
        }
        result = _parse_eval_response(json.dumps(data), MAGAZINE)  # lenient gate
        assert result.passed is False  # 2+ critical → fail

    def test_off_gate_ignores_flags(self):
        data = {
            **EVAL_JSON,
            "hallucination_flags": [
                {"claim": "Made up", "severity": "critical", "note": "Fake"}
            ],
        }
        result = _parse_eval_response(json.dumps(data), PERSONAL)  # off gate
        assert result.passed is True  # hallucination disabled


# ═══════════════════════════════════════════════════════════════════════════
# JSON parsing
# ═══════════════════════════════════════════════════════════════════════════


class TestJSONExtraction:
    def test_extracts_bare_json(self):
        raw = json.dumps(EVAL_JSON)
        assert _extract_json(raw).startswith("{")

    def test_extracts_from_code_fence(self):
        raw = f"```json\n{json.dumps(EVAL_JSON)}\n```"
        extracted = _extract_json(raw)
        assert extracted.startswith("{")

    def test_extracts_json_from_wrapping_text(self):
        raw = f"Here is the evaluation:\n\n{json.dumps(EVAL_JSON)}\n\nHope that helps."
        extracted = _extract_json(raw)
        assert extracted.startswith("{")
        assert "accuracy" in extracted

    def test_parse_result_creates_flags(self):
        data = {
            **EVAL_JSON,
            "hallucination_flags": [
                {"claim": "Fake claim", "severity": "major", "note": "Not found in sources"}
            ],
        }
        result = _parse_eval_response(json.dumps(data), MAGAZINE)
        assert len(result.hallucination_flags) == 1
        assert result.hallucination_flags[0].claim == "Fake claim"
        assert result.hallucination_flags[0].severity == "major"

    def test_handles_invalid_json_gracefully(self):
        result = _parse_eval_response("not json at all", MAGAZINE)
        assert result.passed is False
        assert "failed to produce valid json" in result.notes.lower()


# ═══════════════════════════════════════════════════════════════════════════
# Prompt assembly
# ═══════════════════════════════════════════════════════════════════════════


class TestEvalPromptAssembly:
    def test_user_prompt_includes_section(self):
        prompt = _build_eval_user_prompt(SECTION, RESEARCH)
        assert "Stanford study" in prompt
        assert "Section to Evaluate" in prompt

    def test_system_prompt_includes_thresholds(self):
        prompt = _build_eval_system_prompt(MAGAZINE)
        assert "magazine" in prompt.lower()
        assert "7.0" in prompt  # accuracy threshold for magazine

    def test_system_prompt_falls_back(self):
        with patch(
            "autoessay.pipeline.evaluation._load_program_section", return_value=""
        ):
            prompt = _build_eval_system_prompt(MAGAZINE)
        assert "Return ONLY a JSON object" in prompt


# ═══════════════════════════════════════════════════════════════════════════
# Data model
# ═══════════════════════════════════════════════════════════════════════════


class TestEvaluationResult:
    def test_passed_property(self):
        r = EvaluationResult(accuracy=9, coherence=9, style=9, source_integrity=9, overall="pass")
        assert r.passed is True

    def test_failed_property(self):
        r = EvaluationResult(accuracy=5, coherence=5, style=5, source_integrity=5, overall="fail")
        assert r.passed is False

    def test_with_flags(self):
        flag = HallucinationFlag(claim="test", severity="minor")
        r = EvaluationResult(
            accuracy=8, coherence=8, style=8, source_integrity=8,
            overall="pass", hallucination_flags=[flag],
        )
        assert len(r.hallucination_flags) == 1
        assert r.passed is True


# ═══════════════════════════════════════════════════════════════════════════
# Integration — draft → evaluate with anti-self-review
# ═══════════════════════════════════════════════════════════════════════════


class TestDraftEvaluateIntegration:
    def test_evaluate_excludes_drafting_provider(self):
        """Simulate the full draft-then-evaluate flow with anti-self-review."""
        with (
            patch("autoessay.pipeline.drafting.get_provider") as mock_draft_prov,
            patch("autoessay.pipeline.evaluation.get_provider") as mock_eval_prov,
        ):
            # Mock drafting
            mock_drafter = Mock()
            mock_drafter.chat_for_role.return_value.content = SECTION
            mock_drafter.last_provider = "deepseek"
            mock_draft_prov.return_value = mock_drafter

            # Mock evaluation
            mock_evaluator = Mock()
            mock_evaluator.chat_for_role.return_value.content = json.dumps(EVAL_JSON)
            mock_eval_prov.return_value = mock_evaluator

            from autoessay.pipeline import draft_section, evaluate_section

            # Draft
            section = draft_section("T", "P", RESEARCH, MAGAZINE)
            draft_provider = mock_drafter.last_provider

            # Evaluate — exclude drafting provider
            result = evaluate_section(
                section, RESEARCH, MAGAZINE, exclude_provider=draft_provider
            )

        # Verify the exclude was passed through
        assert mock_evaluator.chat_for_role.call_args[1]["exclude"] == "deepseek"
        assert result.passed is True
