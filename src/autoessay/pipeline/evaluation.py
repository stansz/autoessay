"""Phase 3 — Evaluation: score a drafted section on accuracy, coherence, style, sources."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field

from autoessay.pipeline._utils import _load_program_section
from autoessay.provider import get_provider
from autoessay.style.profile import StyleProfile

# ═══════════════════════════════════════════════════════════════════════════
# Data model
# ═══════════════════════════════════════════════════════════════════════════


@dataclass
class HallucinationFlag:
    """A flagged claim that may be unsupported or fabricated."""

    claim: str
    severity: str  # critical / major / minor
    note: str = ""


@dataclass
class EvaluationResult:
    """Structured evaluation of a single drafted section."""

    accuracy: float  # 1-10 — factual fidelity to sources
    coherence: float  # 1-10 — logical flow, counterargument engagement
    style: float  # 1-10 — adherence to style profile
    source_integrity: float  # 1-10 — citation verifiability, source diversity

    overall: str  # "pass" | "fail"
    hallucination_flags: list[HallucinationFlag] = field(default_factory=list)
    notes: str = ""

    @property
    def passed(self) -> bool:
        """Whether this section meets all quality thresholds."""
        return self.overall == "pass"


# ═══════════════════════════════════════════════════════════════════════════
# Public API
# ═══════════════════════════════════════════════════════════════════════════


def evaluate_section(
    section_text: str,
    research_notes: str,
    style: StyleProfile,
    *,
    exclude_provider: str | None = None,
) -> EvaluationResult:
    """Score a drafted section on four quality dimensions.

    Uses a different LLM provider/model than drafting (anti-self-review bias)
    when *exclude_provider* is set.

    Args:
        section_text: The drafted section content (markdown).
        research_notes: Original research notes for fact-checking claims.
        style: Active style profile — provides quality thresholds.
        exclude_provider: Provider name to exclude from evaluation
                          (pass the drafting provider to prevent self-review).

    Returns:
        ``EvaluationResult`` with scores, flags, and editorial notes.
    """
    provider = get_provider()

    system_prompt = _build_eval_system_prompt(style)
    user_prompt = _build_eval_user_prompt(section_text, research_notes)

    resp = provider.chat_for_role(
        [{"role": "user", "content": user_prompt}],
        role="evaluation",
        system=system_prompt,
        temperature=0.3,  # low temp for consistent scoring
        exclude=exclude_provider,
    )

    return _parse_eval_response(resp.content, style)


# ═══════════════════════════════════════════════════════════════════════════
# Prompt assembly
# ═══════════════════════════════════════════════════════════════════════════


def _build_eval_system_prompt(style: StyleProfile) -> str:
    """Assemble the evaluator's system prompt with thresholds from the style profile."""
    program = _load_program_section("Phase 3: Evaluation (evaluate.py)")
    if not program:
        program = _default_eval_program()

    thresholds = (
        f"\n\n## Quality Thresholds for {style.name}\n"
        f"- Accuracy: {style.accuracy_threshold or 'N/A'} (minimum score to pass)\n"
        f"- Coherence: {style.coherence_threshold}\n"
        f"- Style adherence: {style.style_threshold}\n"
        f"- Source integrity: {style.source_integrity_threshold or 'N/A'}\n"
        f"\nScoring rules:\n"
        f"- If any active threshold is not met → overall = fail\n"
        f"- Hallucination gate: {style.hallucination_gate}\n"
        f"  * strict: flag every unsupported claim as critical → auto-fail\n"
        f"  * lenient: flag major issues only → pass possible with minor flags\n"
        f"  * off: skip hallucination detection entirely\n"
    )

    return program + thresholds


def _build_eval_user_prompt(section_text: str, research_notes: str) -> str:
    """Assemble the user prompt with the section to evaluate and source material."""
    return (
        "## Section to Evaluate\n\n"
        f"{section_text}\n\n"
        "## Research Notes (source of truth)\n\n"
        f"{research_notes}\n\n"
        "Evaluate the section and return ONLY the JSON object (no other text)."
    )


# ═══════════════════════════════════════════════════════════════════════════
# Response parsing
# ═══════════════════════════════════════════════════════════════════════════


def _parse_eval_response(raw: str, style: StyleProfile) -> EvaluationResult:
    """Extract structured evaluation from LLM output.

    Handles JSON in code fences, bare JSON, and partial JSON.
    """
    # Try to extract JSON from the response
    json_str = _extract_json(raw)

    try:
        data = json.loads(json_str)
    except json.JSONDecodeError:
        # Fallback: return a failed evaluation
        return EvaluationResult(
            accuracy=0,
            coherence=0,
            style=0,
            source_integrity=0,
            overall="fail",
            notes=f"Evaluation failed to produce valid JSON. Raw output: {raw[:200]}...",
        )

    flags = [
        HallucinationFlag(
            claim=f.get("claim", ""),
            severity=f.get("severity", "minor"),
            note=f.get("note", ""),
        )
        for f in data.get("hallucination_flags", [])
    ]

    # Determine pass/fail based on style profile thresholds
    overall = "pass"
    if style.hallucination_gate == "strict" and any(
        f.severity == "critical" for f in flags
    ):
        overall = "fail"
    if style.hallucination_gate == "lenient" and len([
        f for f in flags if f.severity == "critical"
    ]) >= 2:
        overall = "fail"
    if style.accuracy_threshold is not None and data.get("accuracy", 0) < style.accuracy_threshold:
        overall = "fail"
    if data.get("coherence", 0) < style.coherence_threshold:
        overall = "fail"
    if data.get("style", 0) < style.style_threshold:
        overall = "fail"
    if (
        style.source_integrity_threshold is not None
        and data.get("source_integrity", 0) < style.source_integrity_threshold
    ):
        overall = "fail"

    return EvaluationResult(
        accuracy=float(data.get("accuracy", 0)),
        coherence=float(data.get("coherence", 0)),
        style=float(data.get("style", 0)),
        source_integrity=float(data.get("source_integrity", 0)),
        overall=overall,
        hallucination_flags=flags,
        notes=data.get("notes", ""),
    )


def _extract_json(raw: str) -> str:
    """Extract JSON from LLM output — handles code fences and partial fragments."""
    # Strip markdown code fences
    cleaned = re.sub(r"^```(?:json)?\s*", "", raw.strip())
    cleaned = re.sub(r"\s*```$", "", cleaned)

    # If it starts with {, use as-is
    if cleaned.strip().startswith("{"):
        return cleaned.strip()

    # Try to find a JSON object in the text
    match = re.search(r"\{.*\}", cleaned, re.DOTALL)
    if match:
        return match.group(0)

    return cleaned


# ═══════════════════════════════════════════════════════════════════════════
# Fallback
# ═══════════════════════════════════════════════════════════════════════════


def _default_eval_program() -> str:
    """Fallback evaluation instructions if program.md is unavailable."""
    return """You are an editor. Score a section on four dimensions.

Return ONLY a JSON object — no other text:
{
  "accuracy": <1-10>,
  "coherence": <1-10>,
  "style": <1-10>,
  "source_integrity": <1-10>,
  "overall": "pass" | "fail",
  "hallucination_flags": [
    {"claim": "the unsupported claim", "severity": "critical|major|minor", "note": "why"}
  ],
  "notes": "editorial feedback — what's strong, what needs work, specific paragraph references"
}

Rules:
1. Factual accuracy — do claims match the research notes? Flag anything hallucinated.
2. Argument coherence — does the logic flow? Are counterarguments engaged?
3. Style adherence — does the writing match the target style?
4. Source integrity — are citations used appropriately?
5. Be specific in notes — cite paragraph numbers."""
