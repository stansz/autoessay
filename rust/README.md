# autoessay (Rust)

An autonomous pipeline for researching, drafting, evaluating, and revising
non-fiction essays and short papers — from a seed topic to a finished,
style-consistent, source-tracked document. This is a Rust rewrite of the
original Python prototype in [`../autoessay`](../autoessay).

CLI-first, provider-agnostic, all data on your machine. Every essay project is
a folder of markdown files (the "GitHub as review surface" layout), so you can
diff, review, and roll back any draft.

---

## Quick Start

```sh
cd autoessay-rs
cargo install --path crates/autoessay-cli     # installs the `autoessay` binary

# or run without installing:
cargo run -p autoessay-cli -- run \
  --seed "Whether remote work killed company culture" \
  --profile magazine --audience 3 --words 2500 \
  --project-dir ./projects/remote-work
```

Set at least one provider API key first (see `crates/autoessay-providers/config/providers.yaml`):

```sh
cp .env.example .env   # add DEEPSEEK_API_KEY and/or ZAI_API_KEY
```

If you hit sandbox approval prompts on a restricted machine, see [BUILD.md](BUILD.md)
for a build that keeps cargo's caches inside the repo.

---

## CLI

```
autoessay run       --seed "..." --profile <name> [--audience 1-5] [--words N]
                    [--project-dir DIR] [--notes ...] [--max-cycles N] [--dry-run]
autoessay evaluate  --section sec_02 --project-dir DIR
autoessay revise    --section sec_02 --project-dir DIR [--feedback "..."]
autoessay styles    [--dir DIR]
```

`run` executes the full pipeline and writes the project folder; `evaluate` and
`revise` operate on one section of an existing project.

---

## The Pipeline

| Phase | What it does |
|---|---|
| **Research** | Topic + seed → structured research notes with tracked sources |
| **Outline** | Research notes → thesis + argument structure + section map |
| **Drafting** | Each section written with the style profile + anti-slop rules + sources |
| **Evaluation** | Scores accuracy, coherence, style, source integrity; flags hallucinations |
| **Revision** | Evaluation → revision brief → rewrite → tighten, loop until pass or plateau |

Each phase can run independently (`evaluate`, `revise`), and the orchestration
persists `state.json` + `sources.json` after every phase.

---

## Style System

Five standard profiles ship embedded in the binary; `autoessay styles` lists
them. The style profile **is** the configuration: citation strictness,
hallucination gates, voice constraints, and quality thresholds all flow from it.

| Profile | Use case |
|---|---|
| `academic` | Journal paper, thesis, conference proceeding |
| `magazine` | Longform journalism, feature article |
| `technical` | Whitepaper, docs, engineering blog |
| `personal-essay` | Substack, blog, opinion piece |
| `policy-brief` | Think tank report, advocacy, government briefing |

Custom profiles: drop a YAML in a styles directory and pass `--profile <name>`
with `--dir`. See `crates/autoessay-core/assets/styles/*.yaml` for the schema.

---

## Workspace Layout

| Crate | Contents | Ported from |
|---|---|---|
| `autoessay-core` | Domain types (style profiles, evaluation, project/source state), program.md section loader | `style/profile.py`, `pipeline/evaluation.py`, `pipeline/_utils.py` |
| `autoessay-providers` | LLM registry + HTTP client (OpenAI-compatible & Anthropic), role→tier routing, retries | `provider.py` |
| `autoessay-cli` | Five pipeline phases, orchestrator, clap CLI | `pipeline/*.py`, `cli.py` |

---

## What's Not Built Yet

This is a working v1 of the core loop. Deliberately out of scope (see
`../autoessay/ARCHITECTURE.md` for the fuller vision):

- **RAG exemplar layer** (FAISS / embeddings / source dedup) — dropped for v1; a good `prompt_instructions` block plus a few static exemplars covers most of the gain.
- **Interactive style discovery survey** and the **custom-profile wizard TUI**.
- **LaTeX / ePub / landing-page export** — output is markdown for now.
- **CrossRef / Semantic Scholar citation verification** and the **3-persona reader panel**.
- **Git commits per phase** — the file layout is ready for it, but the pipeline does not shell out to git yet.

---

## Development

```sh
cargo test --workspace          # 108 tests, all offline (no network)
cargo clippy --all-targets -- -D warnings
```

Tests never call a real LLM: phases take an `LlmClient` trait object, with
`MockLlm` for tests and `ProviderLlm` (wrapping `autoessay-providers`) in
production. Workspace lints deny `unwrap`/`expect`/`panic` in library code —
errors are values, not panics.

## License

MIT
