"""Pipeline phases — research, outline, drafting, evaluation, revision."""

from autoessay.pipeline.outline import generate_outline
from autoessay.pipeline.research import generate_research

__all__ = ["generate_research", "generate_outline"]
