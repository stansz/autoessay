"""Shared utilities for pipeline phases."""

from __future__ import annotations

from pathlib import Path

_PROGRAM_PATH = Path(__file__).resolve().parent.parent.parent / "program.md"


def _load_program_section(heading: str) -> str:
    """Extract a section from program.md by heading name.

    Returns the content under the given heading until the next same-level heading.
    """
    if not _PROGRAM_PATH.exists():
        return ""

    text = _PROGRAM_PATH.read_text()
    marker = f"## {heading}"
    idx = text.find(marker)
    if idx == -1:
        return ""

    # Start after the heading line
    start = idx + len(marker)
    body = text[start:]

    # Stop at the next ## heading
    next_h2 = body.find("\n## ")
    if next_h2 != -1:
        body = body[:next_h2]

    return body.strip()
