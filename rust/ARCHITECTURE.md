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
    autoessay-cli/              pipeline phases, orchestrator, verification, clap CLI
```

Dependency direction is one-way: `cli` → `providers` and `cli` → `core`.
`providers` deliberately shares no domain types with `core` (it returns
`ProviderResult<T, ProviderError>` everywhere) so the HTTP layer can be tested
and reasoned about in isolation.

## The MVP provider setup

The MVP runs on a **single provider key** (OpenRouter) and **different model
families per role**:

```yaml
providers:
  openrouter:
    base_url: https://openrouter.ai/api/v1
    env_key: OPENROUTER_API_KEY
    models:
      fast:  deepseek/deepseek-v4.1-flash    # writer
      smart: xiaomi/mimo-v2.6-pro            # editor
      judge: z-ai/glm-5.3                    # optional second judge
roles:
  drafting: fast
  evaluation: smart
  judge: judge
```

Role → tier routing lives in `providers.yaml`; the phases no longer hard-code
provider names. `run_pipeline` calls `LlmClient::route_for_role` for `drafting`
and `evaluation` and refuses to start if they resolve to the same model
(`orchestrator.rs::ensure_distinct_models`). With `--double-judge` the `judge`
route is checked against both as well. That is the production form of the
anti-self-review rule: a reviewer can never be the writer, even though a single
API key serves everything.

Direct DeepSeek/Z.ai providers remain supported (uncomment them in
`providers.yaml`); with more than one provider available, the first available
provider name in sorted order is used unless a role resolves elsewhere.

## Where the Python landed

| Python | Rust |
|---|---|
| `style/profile.py` | `core/src/style.rs` (`StyleProfile` + voice/gate enums + YAML loader) |
| `pipeline/evaluation.py` (data model) | `core/src/eval.rs` (`EvaluationResult`, claims, outline review) |
| — | `core/src/metrics.rs` (mechanical style metrics) |
| `pipeline/_utils.py` | `core/src/program.rs` (`load_program_section`) |
| — (data model in ARCHITECTURE.md) | `core/src/state.rs` (`ProjectState`, `SectionState`, `Source`, `SourcesFile`) |
| `provider.py` | `providers/src/{config,client,types,error}.rs` |
| `pipeline/{research,outline,drafting,evaluation,revision}.py` | `cli/src/phases/*.rs` |
| `cli.py` (12-line stub) | `cli/src/{cli,main}.rs` + `orchestrator.rs` + `verify.rs` |

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
   expect_used, panic)` in library code. Tests opt out explicitly.
2. **`LlmClient` trait for offline testing.** Every phase is
   `async fn ...(llm: &impl LlmClient, ...)`. `ProviderLlm` wraps the real
   provider; `MockLlm` serves canned responses and records requests. **All 152
   tests run with no network and no API keys.**
3. **Role → tier routing plus model-identity checks** replace hard-coded
   provider exclusion; see "The MVP provider setup".
4. **Retries return the final error**, never swallow it: {429,500,502,503,504}
   or connect/timeout, 4 attempts, `tokio::time::sleep` 2s/4s/8s.
5. **The orchestrator is new.** The Python never wired the phases together. It
   now runs research → Crossref verification → outline → outline-review gate →
   per-section draft/evaluate/revise → metrics → assembly, persisting state
   after each section and each draft before evaluation. It does not shell out
   to git — the layout is ready for per-phase commits as a later step.
6. **The eval parser is the fragile seam, so we asked for structure.** Requests
   carry an OpenAI-compatible JSON Schema `response_format`; our OpenRouter
   models support structured outputs. The brace-balanced salvage parser remains
   as the fallback for endpoints that do not.
7. **Claim-level support matrix.** The evaluator decomposes each section into
   claims and marks support against the research notes. This is the inspectable
   version of "hallucination detection": per-claim verdicts feed the revision
   brief and the CLI summary instead of a single accuracy number.
8. **Independent source verification.** `verify.rs` resolves DOIs/titles via
   Crossref behind a `MetadataResolver` trait. The important verdict is
   `mismatch` (a DOI that resolves to a *different* work), not just "not found";
   verification status is injected into every evaluator prompt.
9. **Mechanical metrics beside the LLM judge.** Readability, sentence-length
   variance, passive ratio, hedges, nominalizations and first-person/contraction
   counts are computed in Rust and included in the evaluator prompt, so the
   judge has objective signals and the report has regression checks.
10. **Cache-friendly prompt ordering.** Research notes (stable across all
    sections) come before section-specific material; evaluation prompts put
    research before the section. This makes provider prompt caching effective,
    and the CLI reports cached-token counts.
11. **Out-of-loop review commands.** `improve` turns recurring evaluation
    findings into reviewable patch proposals (GEIS-style improvement loop, but
    human-applied); `panel` runs the three-persona reader panel over the
    finished essay.
12. **Length guard.** Writer calls continue when `finish_reason` reports a
    token-limit stop, so a budget cut cannot silently truncate a section.

## Build

See [`BUILD.md`](BUILD.md) for building without sandbox approval prompts.
