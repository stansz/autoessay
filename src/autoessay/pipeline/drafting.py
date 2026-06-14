"""Phase 2 — Drafting: section brief → written section with style and source context."""

from __future__ import annotations

from pathlib import Path

from autoessay.provider import get_provider
from autoessay.style.profile import StyleProfile

_ANTI_SLOP_PATH = (
    Path(__file__).resolve().parent.parent.parent.parent / "ANTI-SLOP.md"
)


def draft_section(
    section_title: str,
    section_purpose: str,
    research_notes: str,
    style: StyleProfile,
    *,
    sources: list[str] | None = None,
    previous_sections: list[str] | None = None,
) -> str:
    """Draft a single section of the essay.

    Injects the style profile's prompt instructions, anti-slop rules,
    research context, and source materials into the LLM call. Returns
    the drafted section as markdown.

    Args:
        section_title: Section heading (e.g., "The Promise of Remote Work").
        section_purpose: What this section should accomplish — from the outline.
        research_notes: Full research markdown from ``generate_research()``.
        style: The active style profile (voice, structure, prohibitions).
        sources: Source IDs relevant to this section (e.g., ``["src_01", "src_03"]``).
        previous_sections: Content of already-drafted sections for continuity.

    Returns:
        Drafted section as markdown.
    """
    provider = get_provider()

    system_prompt = _build_system_prompt(style)

    user_prompt = _build_user_prompt(
        section_title, section_purpose, research_notes,
        sources=sources, previous_sections=previous_sections,
    )

    resp = provider.chat_for_role(
        [{"role": "user", "content": user_prompt}],
        role="drafting",
        system=system_prompt,
        temperature=0.7,
        max_tokens=4096,
    )

    return resp.content


# ═══════════════════════════════════════════════════════════════════════════
# Prompt assembly
# ═══════════════════════════════════════════════════════════════════════════


def _build_system_prompt(style: StyleProfile) -> str:
    """Assemble the full system prompt: program + style + anti-slop."""
    parts: list[str] = []

    # Style instructions (the primary voice guide)
    parts.append(style.prompt_instructions)

    # Anti-slop rules — loaded from ANTI-SLOP.md
    anti_slop = _load_anti_slop()
    if anti_slop:
        parts.append("\n\n---\n\n## Writing Rules\n\n")
        parts.append(anti_slop)

    return "".join(parts)


def _build_user_prompt(
    title: str,
    purpose: str,
    research: str,
    *,
    sources: list[str] | None = None,
    previous_sections: list[str] | None = None,
) -> str:
    """Assemble the user prompt with section brief, research, and sources."""
    parts: list[str] = []

    parts.append(f"Write the following section:\n\n## {title}\n\n{purpose}")

    # Source references for this section
    if sources:
        src_list = ", ".join(sources)
        parts.append(f"\n\nUse these sources: {src_list}")

    # Research context (trim to relevant parts if too long)
    parts.append(f"\n\n## Research Notes\n\n{research}")

    # Previous sections for continuity
    if previous_sections:
        prev = "\n\n---\n\n".join(previous_sections)
        parts.append(
            f"\n\n## Previous Sections (for continuity)\n\n{prev}"
        )

    parts.append(
        "\n\nWrite only the section content. "
        "Do not repeat the section title as a heading — it will be added separately."
    )

    return "\n".join(parts)


def _load_anti_slop() -> str:
    """Load ANTI-SLOP.md content if available."""
    if _ANTI_SLOP_PATH.exists():
        return _ANTI_SLOP_PATH.read_text().strip()
    return ""
