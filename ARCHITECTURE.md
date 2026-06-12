# autoessay — Architecture & Design Spec

## Overview
An autonomous pipeline for generating non-fiction essays and short papers.
Agent-first UX, modify→evaluate→keep/discard loop, style-aware, provider-agnostic.
Inspired by `autonovel` (NousResearch) and `autoresearch` (Karpathy), rebuilt from scratch for non-fiction.

---

## Why autoessay — Competitive Landscape

The AI writing tool space is crowded, but segmented into gaps that autoessay fills:

| Segment | Representative tools | What they do | What they miss |
|---|---|---|---|
| **Academic research** | AutoResearchClaw, PaperOrchestra (Google), OpenDraft, Open Paper Machine | Idea → LaTeX paper. Heavy on citations, peer review, lit search | Only for formal research papers |
| **Student essays** | EssayGenius (blakeatech), Jenni, Aithor, Paperpal | Outline → draft → citations. Academic integrity focused, SaaS | School assignments only, not for real-world writing |
| **Fiction** | autonovel, Sudowrite, StoryCraftr | World-building, characters, plot beats | Not relevant to non-fiction |
| **General AI writing** | Jasper, Copy.ai, Rytr | Marketing copy, blog posts | Single-shot generation, no pipeline or style control |

**The gap autoessay owns:** opinion essays, think pieces, magazine longform, policy briefs, technical whitepapers, Substack-style personal essays. Everything between "write my term paper" and "write my NeurIPS submission" is unserved by pipeline-based tools.

### Key differentiators

| Feature | autoessay | Anyone else? |
|---|---|---|
| **Interactive style discovery survey** | Yes — ranking-based profile derivation | Not in any tool |
| **Provider-agnostic** | Anthropic, DeepSeek, OpenRouter, Z.ai, and more | Most tools vendor-lock to OpenAI or Anthropic |
| **RAG-based few-shot style injection** | Yes — style exemplars in every prompt | No pipeline tool does this |
| **Source tracking with hallucination gate** | Yes — factual accuracy evaluator | PaperOrchestra does citation verification; nobody does general hallucination detection |
| **CLI-first, local data** | Yes — all data on your machine | Most are SaaS |
| **Multi-format export** | LaTeX PDF, ePub, plain markdown | Some do PDF, none do all three well |

---

## Provider System

autoessay is provider-agnostic. Any LLM provider with an OpenAI-compatible API works.

### Architecture

All LLM calls go through `provider.py` — a thin abstraction that:
- Accepts a provider name + model ID
- Routes to the correct API endpoint
- Handles authentication, retries, rate limiting
- Returns standardized response objects

### Default provider configuration

```json
{
  "providers": {
    "anthropic": {
      "base_url": "https://api.anthropic.com/v1",
      "env_key": "ANTHROPIC_API_KEY",
      "models": {
        "fast": "claude-sonnet-4-20250514",
        "smart": "claude-opus-4-20250514"
      }
    },
    "deepseek": {
      "base_url": "https://api.deepseek.com/v1",
      "env_key": "DEEPSEEK_API_KEY",
      "models": {
        "fast": "deepseek-chat",
        "smart": "deepseek-reasoner"
      }
    },
    "openrouter": {
      "base_url": "https://openrouter.ai/api/v1",
      "env_key": "OPENROUTER_API_KEY",
      "models": {
        "fast": "deepseek/deepseek-chat",
        "smart": "anthropic/claude-sonnet-4"
      }
    },
    "zai": {
      "base_url": "https://api.z.ai/api/v1",
      "env_key": "ZAI_API_KEY",
      "models": {
        "fast": "glm-4-flash",
        "smart": "glm-4-plus"
      }
    }
  }
}
```

### Role-to-model mapping

Each pipeline phase has a recommended model tier, not a specific model:

| Phase | Tier | Why |
|---|---|---|
| `gen_research.py` | smart | Deep reasoning needed for research synthesis |
| `gen_outline.py` | smart | Structural reasoning |
| `draft_section.py` | fast | Volume — many sections, each iterative |
| `evaluate.py` | fast (different provider) | Separate provider from drafter to avoid self-review bias |
| `reader_panel.py` | smart | Multi-persona reasoning |
| `gen_revision.py` | fast | Follows revision brief, not creative from scratch |
| `tighten.py` | fast | Mechanical word reduction |
| `gen_revision_brief.py` | smart | Synthesize feedback into actionable plan |

The user sets `fast_provider` and `smart_provider` in config. Defaults to whatever API keys are present. If only one provider is configured, autoessay uses different models from the same provider for drafter vs evaluator.

### Provider selection logic

```
1. Check .env for available API keys
2. User-specified providers take priority (config.json)
3. Fallback: any available provider
4. Drafting and evaluation MUST use different providers or different models
   (self-review bias prevention)
```

---

## UX Architecture

autoessay has two interfaces — an engine and an agent — with zero new infrastructure.

### Primary UX: Agent Interface

The user never sees the CLI. They talk to an AI agent (Hermes, Claude Code, OpenCode, etc.) in natural language via their existing channels (Mattermost, Telegram, WhatsApp). The agent owns the pipeline:

```
User: "Write me a 2000-word essay on whether remote work killed company culture.
       Magazine style. Here are my thoughts..."
       [attaches notes]

Agent: [runs research → outline]
       "Here's the structure I'm thinking:
         1. The promise of remote work (2019-2020)
         2. What we actually lost
         3. The hybrid compromise
         4. Where culture lives now
        Does this track?"

User: "Drop section 1, merge 2 and 3. Add a counter-section on companies that
       thrived remote-first."

Agent: [updates outline, commits, begins drafting]
       [surfaces each section for feedback]
       [runs evaluation, surfaces scores]
       [iterates through revision cycles]
```

The agent is the editor. The pipeline is the engine. The user never thinks about phases, models, or YAML files.

### Secondary UX: CLI (power users)

The CLI still exists as the engine interface — useful for scripting, cron jobs, and direct control:

```
$ autoessay run --seed "climate policy" --profile magazine --audience 3 --words 2500
$ autoessay evaluate --section sec_02
$ autoessay revise --section sec_02 --feedback "weaker than sec_01"
```

### GitHub as Version Control & Review Surface

Every essay project is a folder of markdown files. Each pipeline phase produces commits:

```
autoessay/
  projects/
    remote-work-culture/
      .git/
      seed.md              ← commit: "v0: seed"
      outline.md           ← commit: "v1: outline"
      sections/
        sec_01.md          ← commit: "v2: draft sec 1-3"
        sec_02.md
        sec_03.md
      revisions/
        v3-revised.md      ← commit: "v3: revision pass 1"
        v4-final.md        ← commit: "v4: revision pass 2"
      output/
        essay.pdf          ← commit: "v5: export"
```

**Workflow:**
1. Agent creates project folder, inits git repo
2. Each pipeline phase → commit with descriptive message
3. User reviews sections on GitHub (markdown renders natively)
4. User leaves feedback via GitHub comments, PR-style diff reviews, or direct chat
5. Agent pulls feedback, regenerates, commits revision
6. Full history preserved — can always diff against any previous draft

**Benefits:**
- Zero new infrastructure. GitHub is the preview, diff, and review tool
- Full version history. Roll back any paragraph to any draft
- Natural collaboration surface. Multiple people can comment on sections
- Portable. Essays are just markdown files in a git repo

---

## Pipeline Phases

### Phase 0: Seed & Configuration
- User provides a topic/seed (1 sentence to 1 paragraph)
- Select style profile (standard or custom)
- Set audience level (general → expert, 1–5 slider)
- Set target length (words), citation density preference
- Configure providers (or accept auto-detection)

### Phase 1: Research
- `gen_research.py` — Deep research pass: generates structured research notes
  - Key claims, supporting evidence, counterarguments, sources
  - Sources tracked with identifiers (URL, title, key quote, access date)
- `gen_outline.py` — Thesis → argument structure → section map
  - Enforces non-fiction structure: intro/thesis → body arguments → counterarguments → conclusion
  - Each section tagged with source references

### Phase 2: Drafting
- `draft_section.py` — Write one section at a time with:
  - Active style profile injected into prompt
  - RAG-retrieved style exemplars (2–3 most relevant)
  - Source materials for that section
  - Anti-slop rules (non-fiction edition)
- `run_drafts.py` — Sequential section drafter with evaluation gates

### Phase 3: Evaluation (the core)
- `evaluate.py` — Four scoring dimensions:
  1. **Factual accuracy** — LLM fact-checker cross-references claims against provided sources. Separate anti-hallucination pass on every claim.
  2. **Argument coherence** — Does the logic flow? Are counterarguments engaged?
  3. **Style adherence** — Does it match the target style profile?
  4. **Source integrity** — Are citations real? (CrossRef / Semantic Scholar verification). Are sources over-used? (embedding deduplication).
- `reader_panel.py` — 3-persona evaluation (domain expert, general reader, editor)
- Score thresholds: draft passes if all dimensions > threshold

### Phase 4: Revision
- `gen_revision_brief.py` — Aggregate feedback into actionable revision brief
- `gen_revision.py` — Rewrite section from brief
- `tighten.py` — Iterative word-count reduction pass (cut fluff, keep substance)
- Loop until scores stabilize or max cycles reached

### Phase 5: Export
- `typeset/` — LaTeX → PDF (academic, magazine, or report layout)
- `build_epub.py` — ePub output
- Optional: landing page, reading time estimate, TL;DR summary card

---

## Style System

### Architecture
Style is a cross-cutting layer. Every generator (outline, draft, revise) receives:
1. The active style profile (markdown doc with rules + examples)
2. 2–3 RAG-retrieved exemplars from the user's style library

Style is NOT a post-processing pass. It's baked into generation.

### Standard Profiles (shipped)

| Profile | Use case | Key traits |
|---|---|---|
| **Academic** | Journal paper, thesis | Formal register, passive voice OK, high citation density, discipline-specific jargon |
| **Magazine** | Longform journalism | Narrative ledes, scene-setting, accessible to educated layperson, quotes and anecdotes |
| **Technical** | Whitepaper, docs | Precise, declarative, minimal adjectives, heavy on examples and code/diagrams |
| **Personal Essay** | Substack, blog | First-person OK, conversational, opinion-forward, emotional arc |
| **Policy Brief** | Think tank, advocacy | Executive summary, numbered recommendations, evidence-weighted, neutral-to-persuasive register |

### Profile Behavior Matrix (pipeline controls per profile)

Each profile carries its own pipeline behavior defaults. No user-facing feature toggles — the style profile IS the configuration:

| Pipeline behavior | Academic | Magazine | Technical | Personal Essay | Policy Brief |
|---|---|---|---|---|---|
| Citation verification | CrossRef | light web | web | **off** | CrossRef |
| Source deduplication | **on** | on | **on** | off | on |
| Hallucination gate | strict | lenient | strict | **off** | strict |
| Require citation/claim | **yes** | no | yes | no | yes |
| Allow unsourced opinion | no | yes | no | **yes** | no |
| Accuracy threshold | 8.5 | 7.0 | 8.5 | **N/A** | 8.5 |
| First person | no | situational | no | **yes** | situational |
| Passive voice | allowed | moderate | allowed | discouraged | discouraged |
| Contractions | no | situational | no | **yes** | no |

### Custom Profile Wizard

Users can create a custom profile by starting from any base profile and overriding individual controls — no YAML editing required:

```
$ autoessay style custom
Starting from: [Magazine ▼]

  Pipeline gates:
  [✓] Source deduplication (FAISS)
  [✓] Hallucination detection      [strict / lenient / off]
  [✓] Citation verification         [crossref / web / off]
  [ ] Require citation per claim
  [✓] Allow unsourced opinion

  Voice constraints:
  [ ] First person allowed
  [ ] Passive voice allowed
  [✓] Contractions allowed

  Quality thresholds:
  Accuracy:  [████████░░] 7.0
  Coherence: [███████░░░] 7.0
  Style:     [█████████░] 9.0

  Save as: policy-blog-hybrid
```

Custom profiles inherit all defaults from the base profile, then layer the user's overrides on top. Saved to `~/.autoessay/styles/custom/<name>.yaml`. The wizard is available as both a TUI (`autoessay style custom`) and a CLI flag interface (`autoessay run --profile magazine --no-hallucination --allow-first-person`).

### Custom Profiles

**Creation flow:**
```
User uploads 3–5 sample texts (their own writing or aspirational)
        ↓
voice_fingerprint.py — extract:
  - Sentence length distribution (mean, variance, burstiness)
  - Readability scores (Flesch-Kincaid, Gunning Fog, SMOG)
  - Vocabulary tier analysis
  - Passive voice ratio
  - Paragraph structure (avg sentences/paragraph, opening/closing patterns)
  - Citation style and density
  - Rhetorical device frequency (anaphora, tricolon, analogy, etc.)
  - Tonal register (formal ↔ conversational, measured ↔ urgent)
        ↓
Generates style.md — a structured profile document injected into prompts
        ↓
Saved to ~/.autoessay/styles/<name>.md
```

**At generation time:** RAG retrieves the 2–3 most stylistically similar exemplars from the user's library and includes them as few-shot examples in the prompt.

### Phase 2: Style Discovery Survey

An interactive onboarding flow for users who don't have samples but know what they *like*:

```
Round 1: Show 6 short paragraphs on the same topic, 3 style pairs.
         "Which reads better to you?"  →  A / B / No preference
         Pairs: Academic vs Magazine, Technical vs Personal, Policy vs Magazine, etc.

Round 2: Show 3 paragraphs in the winning style cluster, vary sub-dimensions.
         Example: within "Magazine" — more narrative vs more analytical,
         more casual vs more formal, more personal vs more detached.

Round 3: Generate 2 custom paragraphs from the derived profile.
         "Does this sound right?" → refine sliders or confirm.

Output: A generated style.md profile + 2-3 auto-generated exemplars seeded into their library.
```

The survey result is saved as a named custom profile — user can return and tweak anytime. The ranking data itself is stored so future versions can improve the mapping from preferences → profile parameters.

---

## Anti-Hallucination System

### Source verification gate

After drafting, a separate LLM call (different provider/model from the drafter) checks every factual claim:

```
For each claim in section:
  1. Is this claim supported by a provided source? → Source must exist in sources.json
  2. Does the source actually say what the claim asserts? → Source text comparison
  3. Is the source real? → CrossRef / Semantic Scholar API verification (academic sources)
  4. Is this claim novel (not hallucinated)? → Factual consistency check
```

Claims that fail any check are flagged with severity (critical / major / minor). Critical flags block the draft from passing evaluation.

### Source deduplication

Using FAISS embeddings (inspired by EssayGenius):
- Every source gets an embedding vector
- Before adding a new source, check similarity against existing sources
- Prevents the same source being cited under slightly different names
- Prevents over-reliance on a single source

---

## File Structure

```
~/.autoessay/
  styles/                    # Style profiles (standard + custom)
    academic.yaml
    magazine.yaml
    technical.yaml
    personal-essay.yaml
    policy-brief.yaml
    custom/                   # User-tweaked profiles
      my-hybrid.yaml
  exemplars/                 # User's style library (for RAG)
  projects/
    <project-name>/
      seed.md                # Topic/concept
      config.json            # Style, audience, length, providers, citations
      research.md            # Research notes
      outline.md             # Thesis + argument structure
      sources.json           # Source registry with IDs + embedding vectors
      sections/
        sec_01.md            # Drafted sections
        sec_02.md
        ...
      state.json             # Pipeline state, scores, iteration
      output/                # Final exports
        essay.pdf
        essay.epub

REPO (framework, reusable):
  program.md                 # Agent instructions per phase
  ANTI-SLOP.md               # Non-fiction edition (weasel words, hedge overload, etc.)
  CRAFT.md                   # Non-fiction craft (argumentation, evidence, structure)
  PIPELINE.md                # Full automation spec
  WORKFLOW.md                # Human guide

  tools/
    seed.py                  # Generate seed topics
    gen_research.py          # Deep research pass
    gen_outline.py           # Thesis → structure
    draft_section.py         # Write one section
    run_drafts.py            # Batch sequential drafter
    evaluate.py              # Factual accuracy + argument + style + source scoring
    reader_panel.py          # 3-persona evaluation
    gen_revision_brief.py    # Aggregate feedback → revision plan
    gen_revision.py          # Rewrite section
    tighten.py               # Word-count reduction pass
    voice_fingerprint.py     # Extract style fingerprint from samples
    run_pipeline.py          # Full orchestrator
    survey.py                # Phase 2: interactive style discovery
    provider.py              # Provider abstraction layer
    source_checker.py        # Hallucination verification + citation validation

  typeset/
    essay.tex                # LaTeX template
    build_tex.py             # Sections → LaTeX
    build_epub.py            # ePub output

  config/
    providers.json           # Provider definitions (endpoints, models, tiers)
    .env.example             # API keys
    pyproject.toml
```

---

## Data Model

### state.json

```json
{
  "phase": "drafting",
  "iteration": 3,
  "style_profile": "magazine",
  "audience_level": 3,
  "target_words": 3000,
  "providers": {
    "fast": {"provider": "deepseek", "model": "deepseek-chat"},
    "smart": {"provider": "anthropic", "model": "claude-sonnet-4-20250514"}
  },
  "sections": [
    {
      "id": "sec_01",
      "title": "Introduction",
      "status": "complete",
      "scores": {
        "accuracy": 8.2,
        "coherence": 7.5,
        "style": 8.9,
        "source_integrity": 9.1
      },
      "revision_cycles": 2,
      "hallucination_flags": []
    }
  ],
  "overall_score": null,
  "plateau_detected": false
}
```

### sources.json

```json
{
  "sources": [
    {
      "id": "src_01",
      "type": "academic",
      "title": "Title of the paper",
      "authors": ["Author Name"],
      "year": 2025,
      "doi": "10.1234/example",
      "url": "https://...",
      "key_quote": "The relevant excerpt...",
      "embedding_vector": [0.123, 0.456, ...],
      "verified": true,
      "verification_source": "crossref"
    }
  ]
}
```

---

## Evaluation Rubric (non-fiction edition)

| Dimension | Sub-scores | Method |
|---|---|---|
| **Factual accuracy** | Source fidelity, claim verification, absence of hallucination | Separate LLM fact-checker cross-references sources. Different provider/model from drafter. |
| **Argument coherence** | Thesis clarity, logical flow, counterargument engagement, conclusion strength | LLM judge + structural checks |
| **Style adherence** | Register match, vocabulary tier, sentence variety, tonal consistency | Fingerprint comparison against style profile |
| **Source integrity** | Citation verifiability, source diversity, absence of source hallucination | CrossRef/Semantic Scholar API + FAISS dedup |
| **Readability** | Flesch-Kincaid, sentence length variance, paragraph structure | Mechanical scoring |

---

## API Dependencies

| Service | Used for | Required? |
|---|---|---|
| Any LLM provider (Anthropic, DeepSeek, OpenRouter, Z.ai, etc.) | Drafting, evaluation, revision | Required (at least one) |
| CrossRef / Semantic Scholar | Academic citation verification | Optional — improves source integrity scoring |
| FAISS (local) | Source embedding deduplication | Included — no external API needed |

Only one LLM provider is required. A second provider (or different model from same provider) is recommended for evaluation to avoid self-review bias.

---

## Key Design Decisions

1. **Agent-first UX.** The primary interface is natural language via an AI agent. Users talk, the agent runs the pipeline. CLI exists for power users and scripting.
2. **GitHub as review surface.** Every phase is a commit. Markdown renders natively. Diffs between drafts. PR-style feedback. Zero new infrastructure.
3. **Provider-agnostic.** Not locked to Anthropic. Works with anything that has an OpenAI-compatible API.
4. **Style is configuration.** Profile determines pipeline behavior — hallucination gates, citation strictness, voice constraints all flow from the style choice. No scattered feature toggles.
5. **Custom profiles via wizard.** Users tweak from a TUI or CLI flags, not YAML files. Fork a base profile, override knobs, save.
6. **Style as generation context.** Every generator call gets the active profile + 2–3 RAG exemplars. Not a post-processing pass.
7. **Sources are first-class citizens.** Every claim has a `source_id`. Hallucination gate checks them. No orphan claims.
8. **Drafter and evaluator must be different.** Separate providers or separate models — no self-review.
9. **Each phase can run independently.** User can re-run just the outline, or just section 3, or just the evaluation.
10. **Scores are transparent.** Every evaluation produces a breakdown. User sees *why* something scored low.
11. **Style survey is our killer differentiator.** No tool in this space does interactive, ranking-based style discovery.

---

## Next Steps

- [ ] Build `provider.py` — provider abstraction layer with DeepSeek, OpenRouter, Z.ai support
- [ ] Ship provider definitions in `config/providers.json`
- [ ] Build `voice_fingerprint.py` first (it's the engine for everything else)
- [ ] Ship 5 standard style profiles
- [ ] Build core pipeline: seed → research → outline → draft → evaluate
- [ ] Build `source_checker.py` with hallucination detection
- [ ] Add revision loop
- [ ] Phase 2: interactive style discovery survey
