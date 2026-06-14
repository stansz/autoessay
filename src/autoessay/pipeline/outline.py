"""Phase 1b — Outline: research notes → thesis + argument structure + section map."""

from __future__ import annotations

from autoessay.pipeline._utils import _load_program_section
from autoessay.provider import Provider, ProviderRegistry, get_provider
from autoessay.style.profile import StyleProfile


def generate_outline(
    research_notes: str,
    *,
    style: StyleProfile | None = None,
    registry: ProviderRegistry | None = None,
) -> str:
    """Generate a thesis and section map from research notes.

    Calls the LLM (smart tier) with outline program instructions.
    Returns a structured markdown outline with thesis statement,
    argument structure, counterargument section, and conclusion arc.

    Args:
        research_notes: Markdown research output from ``generate_research()``.
        style: Optional style profile — affects section count and tone
               (e.g., Magazine might want 4-5 narrative sections,
                Policy Brief might want 3 structured sections with recommendations).
        registry: Optional pre-configured ProviderRegistry.

    Returns:
        Markdown outline following the program.md format.
    """
    provider = get_provider() if registry is None else Provider(registry)

    program = _load_program_section("Phase 1b: Outline (gen_outline.py)")
    if not program:
        program = _default_outline_program()

    system_prompt = program
    if style:
        system_prompt += f"\n\nTarget style: {style.use_case}\n{style.prompt_instructions}"

    user_prompt = (
        "Based on the following research notes, produce a thesis statement "
        "and a detailed section map:\n\n"
        f"{research_notes}"
    )

    resp = provider.chat_for_role(
        [{"role": "user", "content": user_prompt}],
        role="outline", prefer="zai",
        system=system_prompt,
        temperature=0.5,
    )

    return resp.content


def _default_outline_program() -> str:
    """Fallback outline instructions if program.md is unavailable."""
    return """You are a structural editor. Given research notes, produce a thesis and section map.

Output format:
## Thesis
One-sentence thesis statement.

## Argument Structure
1. **Section Title** — One-sentence purpose | Sources: [src_01, src_02]
2. ...

## Counterargument Section
...

## Conclusion Arc
...

Rules:
- Thesis must be debatable, not a statement of fact
- Every section must reference sources from the research notes
- Counterarguments get their own section, not tucked into footnotes
- Non-fiction structure: intro/thesis → body → counter → conclusion"""
