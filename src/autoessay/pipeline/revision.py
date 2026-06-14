"""Phase 4 — Revision: evaluate → revise → tighten loop until quality gates pass."""

from __future__ import annotations

from autoessay.pipeline._utils import _load_program_section
from autoessay.pipeline.evaluation import EvaluationResult, evaluate_section
from autoessay.provider import get_provider
from autoessay.style.profile import StyleProfile


def generate_revision_brief(
    section_text: str,
    evaluation: EvaluationResult,
    style: StyleProfile,
) -> str:
    """Generate an actionable revision plan from evaluation feedback.

    Uses the evaluation scores, hallucination flags, and editorial notes
    to produce a concrete list of changes to make.

    Args:
        section_text: The drafted section content.
        evaluation: ``EvaluationResult`` from ``evaluate_section()``.
        style: Active style profile.

    Returns:
        Markdown revision brief with specific, numbered action items.
    """
    provider = get_provider()

    program = _load_program_section("Phase 4: Revision (gen_revision.py)")
    if not program:
        program = _default_revision_brief_program()

    system_prompt = program

    # Build a detailed prompt from evaluation data
    flags_text = ""
    if evaluation.hallucination_flags:
        flags_text = "\n".join(
            f"- [{f.severity}] {f.claim}: {f.note}"
            for f in evaluation.hallucination_flags
        )

    user_prompt = (
        "## Original Section\n\n"
        f"{section_text}\n\n"
        "## Evaluation Scores\n\n"
        f"- Accuracy: {evaluation.accuracy}/10\n"
        f"- Coherence: {evaluation.coherence}/10\n"
        f"- Style: {evaluation.style}/10\n"
        f"- Source integrity: {evaluation.source_integrity}/10\n"
        f"- Overall: {evaluation.overall}\n\n"
        f"## Hallucination Flags\n\n{flags_text or 'None'}\n\n"
        f"## Editor's Notes\n\n{evaluation.notes}\n\n"
        "Produce a concrete, numbered revision brief. "
        "Each item must reference a specific paragraph or claim. "
        "Return only the revision brief."
    )

    resp = provider.chat_for_role(
        [{"role": "user", "content": user_prompt}],
        role="revision_brief", prefer="zai",
        system=system_prompt,
        temperature=0.5,
    )

    return resp.content


def revise_section(
    section_text: str,
    revision_brief: str,
    research_notes: str,
    style: StyleProfile,
) -> str:
    """Rewrite a section following a revision brief.

    Preserves what scored well and addresses every item in the brief.
    Returns the complete revised section.

    Args:
        section_text: The original section content.
        revision_brief: Output from ``generate_revision_brief()``.
        research_notes: Original research notes for fact-checking.
        style: Active style profile.

    Returns:
        Revised section as markdown.
    """
    provider = get_provider()

    system_prompt = _build_revise_system_prompt(style)

    user_prompt = (
        "## Original Section\n\n"
        f"{section_text}\n\n"
        "## Revision Brief\n\n"
        f"{revision_brief}\n\n"
        "## Research Notes\n\n"
        f"{research_notes}\n\n"
        "Rewrite the section following the revision brief. "
        "Preserve what works. Fix what doesn't. "
        "Return only the revised section text — no preamble."
    )

    resp = provider.chat_for_role(
        [{"role": "user", "content": user_prompt}],
        role="revision", prefer="deepseek",
        system=system_prompt,
        temperature=0.6,
    )

    return resp.content


def revise_until_pass(
    section_text: str,
    research_notes: str,
    style: StyleProfile,
    *,
    max_cycles: int = 3,
    exclude_provider: str | None = None,
) -> tuple[str, list[EvaluationResult]]:
    """Run the evaluate → brief → revise loop until the section passes.

    Stops when ``evaluation.passed`` is True or *max_cycles* is reached.
    Returns the final section text and the full evaluation history.

    Args:
        section_text: The drafted section to improve.
        research_notes: Original research notes.
        style: Active style profile with quality thresholds.
        max_cycles: Maximum revision cycles before giving up.
        exclude_provider: Provider to exclude from evaluation
                          (anti-self-review — pass the drafting provider).

    Returns:
        ``(final_section, evaluation_history)`` where the last evaluation
        is the most recent (pass or gave up).
    """
    current = section_text
    history: list[EvaluationResult] = []

    for cycle in range(1, max_cycles + 1):
        evaluation = evaluate_section(
            current, research_notes, style,
            exclude_provider=exclude_provider,
        )
        history.append(evaluation)

        if evaluation.passed:
            break

        brief = generate_revision_brief(current, evaluation, style)
        current = revise_section(current, brief, research_notes, style)

    return current, history


# ═══════════════════════════════════════════════════════════════════════════
# Prompt assembly
# ═══════════════════════════════════════════════════════════════════════════


def _build_revise_system_prompt(style: StyleProfile) -> str:
    """System prompt for revision — style instructions + revision rules."""
    program = _load_program_section("Phase 4: Revision (gen_revision.py)")
    if not program:
        program = _default_revise_program()

    return f"{style.prompt_instructions}\n\n---\n\n{program}"


# ═══════════════════════════════════════════════════════════════════════════
# Fallbacks
# ═══════════════════════════════════════════════════════════════════════════


def _default_revision_brief_program() -> str:
    return """You are a revising editor. Given evaluation feedback, produce a revision brief.

Output a numbered list of specific, actionable changes:
1. [Paragraph 2] Fix the unsupported claim about X — either source it or remove it.
2. [Structure] The transition between paragraphs 3 and 4 is abrupt. Add a bridging sentence.
3. [Style] Paragraph 1 uses passive voice 4 times. Rewrite in active voice.

Rules:
- Every item must reference a specific location (paragraph, sentence, claim).
- Prioritize critical issues (hallucinations, factual errors) first.
- Don't suggest vague improvements ("make it better"). Be specific.
- Preserve what already works — only list what needs changing."""


def _default_revise_program() -> str:
    return """You are a revising editor. Rewrite the section following the revision brief.

Rules:
- Address every item in the revision brief.
- Preserve what already worked — don't rewrite the whole section if only paragraph 2 needs fixing.
- Respect the target style profile.
- If the revision brief asks you to remove a claim, remove it entirely — don't hedge.
- Return the complete revised section, not a diff."""
