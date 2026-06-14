"""Pipeline phases — research, outline, drafting, evaluation, revision."""

from autoessay.pipeline.drafting import draft_section
from autoessay.pipeline.evaluation import EvaluationResult, evaluate_section
from autoessay.pipeline.outline import generate_outline
from autoessay.pipeline.research import generate_research
from autoessay.pipeline.revision import (
    generate_revision_brief,
    revise_section,
    revise_until_pass,
)

__all__ = [
    "draft_section",
    "evaluate_section",
    "EvaluationResult",
    "generate_outline",
    "generate_research",
    "generate_revision_brief",
    "revise_section",
    "revise_until_pass",
]
