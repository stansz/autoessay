# autoessay (Rust)

An autonomous pipeline for researching, drafting, evaluating, and revising
non-fiction essays and short papers — from a seed topic to a finished,
style-consistent, source-tracked document. This is the Rust implementation of
the pipeline; the original Python prototype lives at the repo root (`../../src`).

CLI-first, provider-agnostic, all data on your machine. Every essay project is
a folder of markdown files (the "GitHub as review surface" layout), so you can
diff, review, and roll back any draft.

---

## Quick Start

From the `rust/` directory:

```sh
cp .env.example .env          # add OPENROUTER_API_KEY (https://openrouter.ai/keys)
cargo install --path crates/autoessay-cli     # installs the `autoessay` binary

# or run without installing:
cargo run -p autoessay-cli -- run \
  --seed "Whether remote work killed company culture" \
  --profile magazine --audience 3 --words 2200 \
  --project-dir ./projects/remote-work
```

The MVP needs **one provider key**. OpenRouter gives access to both models the
flow uses:

| Tier | Model | Used for |
|---|---|---|
| `fast` | `deepseek/deepseek-v4.1-flash` | drafting, revision (the heavy lifting) |
| `smart` | `anthropic/claude-sonnet-4.6` | research, outline, evaluation, revision brief |

Drafting and evaluation resolve to **different model families**, so the
evaluator never grades its own writing. The pipeline refuses to run if
`roles.evaluation` is configured to the same model as `roles.drafting`.

If you hit sandbox approval prompts on a restricted machine, see [BUILD.md](BUILD.md)
for a build that keeps cargo's caches inside the repo.

---

## CLI

```
autoessay run       --seed "..." --profile <name> [--dir <styles-dir>]
                    [--audience 1-5] [--words N]
                    [--project-dir DIR] [--notes ...] [--max-cycles N] [--dry-run]
autoessay evaluate  --section sec_02 --project-dir DIR
autoessay revise    --section sec_02 --project-dir DIR [--feedback "..."]
autoessay styles    [--dir DIR]
```

`run` executes the full pipeline and writes the project folder, including
`essay.md`; `evaluate` and `revise` operate on one section of an existing
project.

---

## The Pipeline

| Phase | What it does |
|---|---|
| **Research** | Topic + seed → structured research notes with tracked sources |
| **Outline** | Research notes → thesis + argument structure + section map |
| **Drafting** | Each section written with the style profile, program.md Phase 2 instructions, anti-slop rules, sources, and a per-section word budget |
| **Evaluation** | Scores accuracy, coherence, style, source integrity; flags hallucinations |
| **Revision** | Evaluation → revision brief → rewrite, looping until pass, plateau, or max cycles |

The orchestrator persists `state.json` after each section and writes
`sources.json` after research. Drafts are written to disk before evaluation so a
failed model call never loses paid-for output. `essay.md` is the assembled
finished document; per-section markdown stays in `sections/`.

---

## Output layout

```
projects/<name>/
  seed.md            run configuration (seed, style, audience, target words)
  research.md        research notes (source of truth for evaluation)
  outline.md         thesis + section map
  sources.json       extracted source registry
  sections/sec_01.md final text per section
  revisions/         archived prior versions when a section was revised
  state.json         phase, per-section status, scores, cycles, flags
  essay.md           assembled finished document
```

---

## Style System

Five standard profiles ship embedded in the binary; `autoessay styles` lists
them. The style profile **is** the configuration: hallucination gates and
quality thresholds flow from it.

| Profile | Use case |
|---|---|
| `academic` | Journal paper, thesis, conference proceeding |
| `magazine` | Longform journalism, feature article |
| `technical` | Whitepaper, docs, engineering blog |
| `personal-essay` | Substack, blog, opinion piece |
| `policy-brief` | Think tank report, advocacy, government briefing |

Custom profiles: drop a YAML in a styles directory and pass `--profile <name>
--dir <dir>` to `run`. See `crates/autoessay-core/assets/styles/*.yaml` for the
schema.

---

## Workspace Layout

| Crate | Contents | Ported from |
|---|---|---|
| `autoessay-core` | Domain types (style profiles, evaluation, project/source state), program.md section loader | `style/profile.py`, `pipeline/evaluation.py`, `pipeline/_utils.py` |
| `autoessay-providers` | LLM registry + HTTP client (OpenAI-compatible & Anthropic), role→tier routing, retries | `provider.py` |
| `autoessay-cli` | Five pipeline phases, orchestrator, clap CLI | `pipeline/*.py`, `cli.py` |

---

## What's Not Built Yet

This is a working MVP of the core loop. Deliberately out of scope (see
`../../ARCHITECTURE.md` for the fuller vision):

- **RAG exemplar layer** (FAISS / embeddings) — deferred; style profiles carry the voice guidance for now.
- **Interactive style discovery survey** and the **style fingerprint wizard**.
- **LaTeX / ePub / landing-page export** — output is markdown for now.
- **CrossRef / Semantic Scholar citation verification** and the **3-persona reader panel** — the evaluator flags unsupported claims but does not independently verify sources yet.
- **Resume / idempotency** — a re-run starts a project over; use a new `--project-dir` or copy the old one first.
- **Token/cost accounting** — usage is returned by the provider but not yet aggregated or printed.
- **Git commits per phase** — the file layout is ready, but the pipeline does not shell out to git.

---

## Development

```sh
cargo test --workspace          # 114 tests, all offline (no network, no API keys)
cargo clippy --all-targets -- -D warnings
```

Tests never call a real LLM: phases take an `LlmClient` trait object, with
`MockLlm` for tests and `ProviderLlm` (wrapping `autoessay-providers`) in
production. Workspace lints deny `unwrap`/`expect`/`panic` in library code —
errors are values, not panics.

## License

MIT — see [`../LICENSE`](../LICENSE).
