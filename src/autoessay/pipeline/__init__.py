"""Pipeline phases — research, outline, drafting, evaluation, revision."""

from autoessay.pipeline.drafting import draft_section
from autoessay.pipeline.outline import generate_outline
from autoessay.pipeline.research import generate_research

__all__ = ["draft_section", "generate_outline", "generate_research"]
