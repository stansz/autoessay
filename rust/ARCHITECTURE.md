# autoessay-rs — Architecture

Short companion to [`README.md`](README.md). The original, much larger design
vision lives in [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) and still
describes the parts not yet built (RAG exemplars, style survey, LaTeX/ePub
export). This doc covers what exists and why it is shaped this way.

---

## Three crates

```
rust/
  Cargo.toml                    workspace + shared deps + [workspace.lints]
  crates/
    autoessay-core/             domain types, no I/O beyond YAML/markdown files
    autoessay-providers/        LLM HTTP layer (registry, routing, retries)
    autoessay-cli/              pipeline phases, orchestrator, clap CLI
```

Dependency direction is one-way: `cli` → `providers` and `cli` → `core`.
`providers` deliberately shares no domain types with `core` (it returns
`ProviderResult<T, ProviderError>` everywhere) so the HTTP layer can be tested
and reasoned about in isolation.

## The MVP provider setup

The MVP runs on a **single provider key** (OpenRouter) and **two models from
different families**:

```yaml
providers:
  openrouter:
    base_url: https://openrouter.ai/api/v1
    env_key: OPENROUTER_API_KEY
    models:
      fast:  deepseek/deepseek-v4.1-flash    # writer
      smart: xiaomi/mimo-v2.6-pro            # editor
roles:
  drafting: fast
  evaluation: smart
```

Role → tier routing lives in `providers/config.yaml`; the phases no longer
hard-code provider names. `run_pipeline` calls `LlmClient::route_for_role` for
`drafting` and `evaluation` and refuses to start if they resolve to the same
model (`orchestrator.rs::ensure_distinct_models`). That is the production form
of the anti-self-review rule: a reviewer can never be the writer, even though a
single API key serves both.

Direct DeepSeek/Z.ai providers remain supported (uncomment them in
`providers.yaml`); with more than one provider available, the first available
provider name in sorted order is used unless a role resolves elsewhere.

## Where the Python landed

| Python | Rust |
|---|---|
| `style/profile.py` | `core/src/style.rs` (`StyleProfile` + voice/gate enums + YAML loader) |
| `pipeline/evaluation.py` (data model) | `core/src/eval.rs` (`EvaluationResult`, `HallucinationFlag`, `Severity`) |
| `pipeline/_utils.py` | `core/src/program.rs` (`load_program_section`) |
| — (data model in ARCHITECTURE.md) | `core/src/state.rs` (`ProjectState`, `SectionState`, `Source`, `SourcesFile`) |
| `provider.py` | `providers/src/{config,client,types,error}.rs` |
| `pipeline/{research,outline,drafting,evaluation,revision}.py` | `cli/src/phases/*.rs` |
| `cli.py` (12-line stub) | `cli/src/{cli,main}.rs` + `cli/src/orchestrator.rs` |

The **prompt text** in `program.md`, `ANTI-SLOP.md`, and the style YAMLs is the
valuable output of the original project and is ported **verbatim** — do not
paraphrase it. `program.md` and `ANTI-SLOP.md` are loaded at runtime (fast
prompt iteration without recompiling); heading matching is formatting-tolerant,
so markup changes in the document do not silently disable a section. The 5
standard style profiles are embedded via `include_str!` so the binary is
self-contained. `.env` loading was restored from the Python implementation.

## Key design decisions

1. **Errors are values.** `CoreError` / `ProviderError` via `thiserror`; every
   fallible path is a `Result`. Workspace lints `deny(clippy::unwrap_used,
   expect_used, panic)` in library code — this is the whole reason for choosing
   Rust over Python: the compiler enforces that AI-written code cannot silently
   drop a failure path. Tests opt out of those lints explicitly.
2. **`LlmClient` trait for offline testing.** Every phase is
   `async fn ...(llm: &impl LlmClient, ...)`. `ProviderLlm` wraps the real
   `autoessay_providers::Provider`; `MockLlm` serves canned responses and
   records requests. This is the compiler-enforced analogue of the Python
   tests' patch-then-Mock. **All 116 tests run with no network and no API keys.**
3. **Role → tier routing plus a model-identity check** replaces the old
   hard-coded `exclude="deepseek"`; see "The MVP provider setup" above.
4. **Retries return the final error**, never swallow it: {429,500,502,503,504}
   or connect/timeout, 4 attempts, `tokio::time::sleep` 2s/4s/8s.
5. **The orchestrator is new.** The Python never wired the phases together;
   `cli/src/orchestrator.rs` does research → outline → per-section draft →
   evaluate → revise-until-pass. It writes `state.json` after each section,
   `sources.json` after research, persists each draft before evaluation, and
   assembles `essay.md`. It does not shell out to git — the layout is ready for
   per-phase commits as a later step.
6. **The eval parser is the fragile seam.** Model output is imperfect, so
   `phases/evaluation.rs` salvages JSON from code fences, wrapping prose, and
   trailing text (brace-balanced salvage beyond the Python's greedy regex), and
   returns a clear failed `EvaluationResult` — never a panic — on malformed input.
7. **Length and audience reach the prompts.** `--words` is divided across
   sections and injected into drafting/revision; `--audience` is injected as
   level 1–5, instead of being metadata only.

## Build

See [`BUILD.md`](BUILD.md) for building without sandbox approval prompts.
