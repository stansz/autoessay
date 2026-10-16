# autoessay-rs — Architecture

Short companion to [`README.md`](README.md). The original, much larger design
vision lives in [`../autoessay/ARCHITECTURE.md`](../autoessay/ARCHITECTURE.md)
and still describes the parts not yet built (RAG exemplars, style survey,
LaTeX/ePub export). This doc covers what exists and why it is shaped this way.

---

## Three crates

```
autoessay-rs/
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
prompt iteration without recompiling); the 5 standard style profiles are
embedded via `include_str!` so the binary is self-contained.

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
tests' patch-then-Mock. **All 108 tests run with no network and no API keys.**
3. **Role → tier routing** (`providers/config.rs`) reproduces the Python's
   anti-self-review rule: evaluation runs on a different provider/model than
drafting, and `resolve_provider(.., exclude=..)` enforces it.
4. **Retries return the final error**, never swallow it: {429,500,502,503,504}
   or connect/timeout, 4 attempts, `tokio::time::sleep` 2s/4s/8s.
5. **The orchestrator is new.** The Python never wired the phases together;
   `cli/src/orchestrator.rs` does research → outline → per-section draft →
evaluate → revise-until-pass, persisting `state.json` + `sources.json` after
every phase and writing the GitHub-as-review-surface layout. It does not shell
out to git — the layout is ready for per-phase commits as a later step.
6. **The eval parser is the fragile seam.** Model output is imperfect, so
   `phases/evaluation.rs` salvages JSON from code fences, wrapping prose, and
trailing text (brace-balanced salvage beyond the Python's greedy regex), and
returns a clear failed `EvaluationResult` — never a panic — on malformed input.

## Build

See [`BUILD.md`](BUILD.md) for building without sandbox approval prompts.
