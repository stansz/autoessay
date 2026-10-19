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
chmod 600 .env                # optional: key readable only by you
cargo install --path crates/autoessay-cli     # installs the `autoessay` binary

# or run without installing:
cargo run -p autoessay-cli -- run \
  --seed "Whether remote work killed company culture" \
  --profile magazine --audience 3 --words 2200 \
  --project-dir ./projects/remote-work
```

The MVP needs **one provider key**. OpenRouter gives access to every model the
flow uses:

| Tier | Model | Used for |
|---|---|---|
| `fast` | `deepseek/deepseek-v4.1-flash` | drafting, revision (the heavy lifting) |
| `smart` | `xiaomi/mimo-v2.6-pro` | research, outline, outline review, evaluation, revision brief |
| `judge` | `z-ai/glm-5.3` | optional second judge for `--double-judge` |

Drafting and evaluation resolve to **different model families**, so the
evaluator never grades its own writing. The pipeline refuses to run if
`roles.evaluation` is configured to the same model as `roles.drafting`, and the
second judge (when enabled) must differ from both.

### Changing models/config without rebuilding

Provider config is resolved at **runtime**, in this order:

1. `--config <path>` (or `AUTOESSAY_CONFIG=<path>`)
2. `./config/providers.yaml` in the directory you launch from
3. `crates/autoessay-providers/config/providers.yaml` (local checkout)
4. the copy embedded in the binary at build time (fallback only)

Editing a file found by rules 1–3 takes effect on the next run — **no manual
rebuild**. Only rule 4 needs `cargo build`/`cargo install` to refresh, which
matters when you copy the binary to a machine without the source tree. `.env`
keys, `program.md`, `ANTI-SLOP.md`, and style profiles in `config/styles/` are
also read from disk at runtime; their embedded copies are fallbacks.

```sh
# switch model config per run, no rebuild:
autoessay --config ./my-providers.yaml run --seed "..." --profile magazine
# or:
AUTOESSAY_CONFIG=./my-providers.yaml autoessay run --seed "..." --profile magazine
```

If you hit sandbox approval prompts on a restricted machine, see [BUILD.md](BUILD.md)
for a build that keeps cargo's caches inside the repo.

---

## CLI

```
autoessay [--config <providers.yaml>] <command>

autoessay run       --seed "..." --profile <name> [--dir <styles-dir>]
                    [--audience 1-5] [--words N]
                    [--project-dir DIR] [--notes ...] [--max-cycles N] [--dry-run]
                    [--no-verify-sources] [--no-outline-review] [--double-judge]
autoessay evaluate  --section sec_02 --project-dir DIR
autoessay revise    --section sec_02 --project-dir DIR [--feedback "..."]
autoessay improve   --project-dir DIR        # propose prompt-asset patches
autoessay panel     --project-dir DIR        # three-persona reader panel
autoessay styles    [--dir DIR]
```

`run` executes the full pipeline and writes the project folder, including
`essay.md`. `improve` reads the project's evaluation findings and writes
`improvements.md` (reviewable patch proposals for `program.md`, `ANTI-SLOP.md`,
and the style profile — nothing is applied automatically). `panel` writes
`panel.md` with domain-expert, general-reader, and editor reviews of the essay.

---

## The Pipeline

| Stage | What it does |
|---|---|
| **Research** | Topic + seed → structured research notes with tracked sources |
| **Verify sources** | Resolve DOIs/titles against Crossref; mark verified / mismatch / not found. A DOI that resolves to a *different* paper is flagged as a mismatch, not trusted |
| **Outline** | Research notes → thesis + argument structure + section map |
| **Outline review** | Editor scores the outline (thesis, structure, source coverage, counterargument); regenerates it once when weak, before any drafting spend |
| **Drafting** | Each section written with the style profile, program.md Phase 2 instructions, anti-slop rules, sources, and a per-section word budget; truncated outputs are continued, not silently cut |
| **Evaluation** | A different model scores accuracy, coherence, style, source integrity, and returns a **claim-level support matrix** (supported / partial / unsupported per claim) |
| **Revision** | Evaluation → revision brief → rewrite, looping until pass, plateau, or max cycles; `--double-judge` averages a second model's scores |
| **Metrics & assembly** | Mechanical style metrics (readability, sentence rhythm, passive ratio, hedges, nominalizations) plus word count, then `essay.md` |

Prompts are ordered research-first so the large, repeated research block is a
stable prefix for provider prompt caching (OpenRouter/DeepSeek/Xiaomi cache
reads are a fraction of normal input price). The CLI reports token usage and
cache hits at the end of every run.

---

## Quality & safety mechanisms

- **Structured outputs.** Evaluations and outline reviews request a JSON Schema
  response (`response_format`); the tolerant brace-balanced parser remains as a
  fallback for endpoints without structured-output support.
- **Anti-self-review.** Writer ≠ evaluator (and judge ≠ both), enforced before
  any API spend. Offline mocks skip the check because they cannot report routes.
- **Source verification.** Crossref lookups (DOI, then title search) mark every
  source; the evaluator receives the verification status and is told to treat
  mismatched/unverifiable sources as weak evidence. Set `CROSSREF_EMAIL` for the
  polite pool; `--no-verify-sources` skips the network step.
- **Claims, not just scores.** The claim matrix makes "which sentence is
  unsupported" inspectable, and unsupported claims flow into the revision brief.
- **Mechanical checks.** Readability/hedge/nominalization metrics guard against
  the classic AI-prose failure modes independently of the LLM judge.
- **Length guard.** A section that stops on `finish_reason == "length"` is
  continued (up to two rounds) instead of being treated as complete.
- **Drafts are safe.** Each draft is written to disk before evaluation, so a
  failed model call never loses paid-for text; `revise` archives the prior
  version before overwriting.

---

## Output layout

```
projects/<name>/
  seed.md              run configuration
  research.md          research notes (source of truth for evaluation)
  outline.md           thesis + section map
  outline-review.json  outline scores and issues (when the gate ran)
  sources.json         extracted sources + verification verdicts
  sections/sec_01.md   final text per section
  revisions/           archived prior versions
  state.json           phase, status, scores, claims, metrics, cycles, flags
  essay.md             assembled finished document
  improvements.md      prompt-patch proposals (after `autoessay improve`)
  panel.md             reader-panel reviews (after `autoessay panel`)
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

| Module | Contents |
|---|---|
| `autoessay-core` | Domain types (style profiles, evaluation + claims + outline review, project/source state, mechanical style metrics), program.md loader |
| `autoessay-providers` | LLM registry + HTTP client (OpenAI-compatible & Anthropic, structured outputs, cached-token usage), role→tier routing, retries |
| `autoessay-cli` | Pipeline phases, Crossref source verification, orchestrator, clap CLI |

---

## What's Not Built Yet

Deliberately out of scope (see `../../ARCHITECTURE.md` for the fuller vision):

- **RAG exemplar layer** (FAISS / embeddings) — style profiles carry the voice guidance for now.
- **Interactive style discovery survey** and the **style fingerprint wizard**.
- **LaTeX / ePub / landing-page export** — output is markdown for now.
- **Resume / idempotency** — a re-run starts a project over; use a new `--project-dir` or copy the old one first.
- **Git commits per phase** — the file layout is ready, but the pipeline does not shell out to git.
- **Automatic prompt self-editing** — `improve` writes proposals; a human applies them.
- **Citation checkers beyond Crossref** — Semantic Scholar / OpenAlex / arXiv full-text quote verification are candidates.
- **Dollar estimates** — token and cache counts are printed; pricing stays provider-side.

---

## Development

```sh
cargo test --workspace          # 152 tests, all offline (no network, no API keys)
cargo clippy --all-targets -- -D warnings
```

Tests never call a real LLM: phases take an `LlmClient` trait object, with
`MockLlm` for tests and `ProviderLlm` (wrapping `autoessay-providers`) in
production. Crossref verification is behind a `MetadataResolver` trait and faked
in tests. Workspace lints deny `unwrap`/`expect`/`panic` in library code — errors
are values, not panics.

## API keys & safety

- `.env` and `.env.*` are **gitignored** here and at the repo root. Never commit a
  real key; only `.env.example` placeholders belong in git.
- Use a dedicated OpenRouter key with a spend limit rather than an account-wide key.
  `CROSSREF_EMAIL` (optional) is only used for Crossref's polite pool, not a secret.
- The binary reads keys from the environment/`.env`; it never writes them into
  project output, logs, or error messages. Keys are only sent in request headers.
- If a key is ever committed or pasted somewhere public, revoke it in the provider
  dashboard immediately.

## License

MIT — see [`../LICENSE`](../LICENSE).
