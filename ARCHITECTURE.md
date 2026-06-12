# autoessay — Architecture & Design Spec

## Overview
An autonomous pipeline for generating non-fiction essays and short papers.
CLI-first, modify→evaluate→keep/discard loop, style-aware.
Inspired by `autonovel` but rebuilt for non-fiction from scratch.

---

## Pipeline Phases

### Phase 0: Seed & Configuration
- User provides a topic/seed (1 sentence to 1 paragraph)
- Select style profile (standard or custom)
- Set audience level (general → expert, 1–5 slider)
- Set target length (words), citation density preference

### Phase 1: Research
- `gen_research.py` — Deep research pass: generates structured research notes
  - Key claims, supporting evidence, counterarguments, sources
  - Sources tracked with identifiers (URL, title, key quote)
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
- `evaluate.py` — Three scoring dimensions:
  1. **Factual accuracy** — LLM fact-checker cross-references claims against provided sources
  2. **Argument coherence** — Does the logic flow? Are counterarguments engaged?
  3. **Style adherence** — Does it match the target style profile?
- `reader_panel.py` — 3-persona evaluation (domain expert, general reader, editor)
- Score thresholds: draft passes if all dimensions > threshold

### Phase 4: Revision
- `gen_revision_brief.py` — Aggregate feedback into actionable revision brief
- `gen_revision.py` — Rewrite section from brief
- `tighten.py` — Iterative word-count reduction pass (cut fluff, keep substance)
- Loop until scores stabilize or max cycles reached

### Phase 5: Export
- `typeset/` — LaTeX → PDF (academic paper, magazine layout, or plain report)
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

## File Structure

```
~/.autoessay/
  styles/                    # Style profiles (standard + custom)
    academic.md
    magazine.md
    technical.md
    personal-essay.md
    policy-brief.md
    my-voice.md              # user-created
  exemplars/                 # User's style library (for RAG)
  projects/
    <project-name>/
      seed.md                # Topic/concept
      config.json            # Style, audience, length, citations
      research.md            # Research notes
      outline.md             # Thesis + argument structure
      sources.json           # Source registry with IDs
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
    evaluate.py              # Factual accuracy + argument + style scoring
    reader_panel.py          # 3-persona evaluation
    gen_revision_brief.py    # Aggregate feedback → revision plan
    gen_revision.py          # Rewrite section
    tighten.py               # Word-count reduction pass
    voice_fingerprint.py     # Extract style fingerprint from samples
    run_pipeline.py          # Full orchestrator
    survey.py                # Phase 2: interactive style discovery

  typeset/
    essay.tex                # LaTeX template
    build_tex.py             # Sections → LaTeX
    build_epub.py            # ePub output

  config/
    .env.example             # API keys
    pyproject.toml
```

---

## Data Model (state.json)

```json
{
  "phase": "drafting",
  "iteration": 3,
  "style_profile": "magazine",
  "audience_level": 3,
  "target_words": 3000,
  "sections": [
    {
      "id": "sec_01",
      "title": "Introduction",
      "status": "complete",
      "scores": {"accuracy": 8.2, "coherence": 7.5, "style": 8.9},
      "revision_cycles": 2
    }
  ],
  "overall_score": null,
  "plateau_detected": false
}
```

---

## Evaluation Rubric (non-fiction edition)

| Dimension | Sub-scores | Method |
|---|---|---|
| **Factual accuracy** | Source fidelity, claim verification, absence of hallucination | LLM fact-checker cross-references sources |
| **Argument coherence** | Thesis clarity, logical flow, counterargument engagement, conclusion strength | LLM judge + structural checks |
| **Style adherence** | Register match, vocabulary tier, sentence variety, tonal consistency | Fingerprint comparison against style profile |
| **Readability** | Flesch-Kincaid, sentence length variance, paragraph structure | Mechanical scoring |

---

## API Dependencies

| Service | Used for |
|---|---|
| Anthropic (Sonnet) | Drafting, evaluation, revision |
| Anthropic (Opus) | Final review pass (optional, dual-persona) |
| fal.ai | Optional: cover art |
| ElevenLabs | Optional: audio version |

Only Anthropic Sonnet is required. Opus review, art, and audio are optional flags.

---

## Key Design Decisions

1. **Style is not post-processing.** Baked into every generation call.
2. **Sources are first-class citizens.** Every claim has a `source_id`. Factual accuracy gate checks them. No orphan claims.
3. **Each phase can run independently.** User can re-run just the outline, or just section 3, or just the evaluation.
4. **Scores are transparent.** Every evaluation produces a breakdown. User sees *why* something scored low.
5. **Phase 2 survey is a UX differentiator.** Most tools ship style profiles as dropdowns. The survey makes style discovery interactive and personal.

---

## Next Steps

- [ ] Stub out project structure and `pyproject.toml`
- [ ] Build `voice_fingerprint.py` first (it's the engine for everything else)
- [ ] Ship 5 standard style profiles
- [ ] Build core pipeline: seed → research → outline → draft → evaluate
- [ ] Add revision loop
- [ ] Phase 2: style discovery survey
