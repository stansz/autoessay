# PIPELINE — Full automation spec

Technical specification for the orchestration layer (`run_pipeline.py`).
To be developed in detail during v0.1 implementation.

## Orchestration
- `run_pipeline.py` drives the full pipeline end-to-end
- Each phase is a separate subprocess call (can run independently)
- Phases communicate via files — no in-memory state
- `state.json` is the single source of truth for pipeline progress

## Resume
- If a phase fails, the pipeline can resume from the last completed phase
- State is checkpointed after each phase completes
- Output files are versioned with iteration numbers to prevent overwrite

## Plateau Detection
- If scores stop improving after N revision cycles, flag and stop
- User can override plateau and force another cycle
