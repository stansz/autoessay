"""Phase 1 — Research: topic → structured research notes with source tracking."""

from __future__ import annotations

from autoessay.pipeline._utils import _load_program_section
from autoessay.provider import Provider, ProviderRegistry, get_provider
from autoessay.style.profile import StyleProfile


def generate_research(
    topic: str,
    *,
    seed: str | None = None,
    style: StyleProfile | None = None,
    registry: ProviderRegistry | None = None,
) -> str:
    """Generate structured research notes for a topic.

    Calls the LLM (smart tier) with research program instructions and
    optional style context. Returns markdown with key claims, supporting
    evidence, counterarguments, and a tracked source list.

    Args:
        topic: The topic or question to research (1 sentence to 1 paragraph).
        seed: Optional user-provided notes, angles, or materials.
        style: Optional style profile — injected for context but research
               output is style-agnostic (raw notes, not prose).
        registry: Optional pre-configured ProviderRegistry.

    Returns:
        Markdown research notes following the program.md format.
    """
    provider = get_provider() if registry is None else Provider(registry)

    program = _load_program_section("Phase 1: Research (gen_research.py)")
    if not program:
        program = _default_research_program()

    system_prompt = program
    if style:
        system_prompt += (
            "\n\nStyle context (for reference — research output is style-agnostic):"
            f"\n{style.prompt_instructions}"
        )

    user_prompt = f"Research the following topic:\n\n{topic}"
    if seed:
        user_prompt += f"\n\nAdditional context from the user:\n{seed}"

    resp = provider.chat_for_role(
        [{"role": "user", "content": user_prompt}],
        role="research",
        system=system_prompt,
        temperature=0.4,  # lower temp for factual research
    )

    return resp.content


def _default_research_program() -> str:
    """Fallback research instructions if program.md is unavailable."""
    return """You are a research assistant. Given a topic, produce structured research notes.

Output format:
## <Topic>

### Key Claims
- Claim: ... | Source: <identifier> | Confidence: high/medium/low

### Supporting Evidence
- Evidence: ... | Source: <identifier>

### Counterarguments
- Counter: ... | Source: <identifier>

### Sources
- [src_01] Title, Author, Year, URL, Key Quote

Rules:
- Every claim must link to a source identifier
- Do not fabricate sources — if you don't have one, say "unsourced claim"
- Prefer primary sources over secondary
- Flag speculative claims explicitly"""
