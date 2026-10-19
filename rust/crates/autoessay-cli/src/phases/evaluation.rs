//! Phase 3 — Evaluation: score a drafted section on accuracy, coherence, style, sources.
//!
//! Ports Python pipeline/evaluation.py, including the fragile _parse_eval_response
//! tolerance for imperfect model output. Two 2026 additions:
//!
//! * structured output: the request asks for a JSON Schema response, so
//!   providers that support it return schema-shaped JSON. The salvage parser
//!   remains as the fallback for providers/endpoints without that support.
//! * a claim-level support matrix: the evaluator decomposes the section into
//!   claims and marks each supported / partial / unsupported against the
//!   research notes, instead of only emitting a holistic accuracy score.

use regex::Regex;

use autoessay_core::{
    ClaimStatus, ClaimSupport, EvaluationResult, HallucinationFlag, Severity, StyleMetrics,
    StyleProfile,
};

use crate::docs::load_program_section;
use crate::error::CliResult;
use crate::llm::{ChatMessage, ChatRequest, LlmClient};

/// Fallback evaluation instructions if program.md is unavailable.
/// Verbatim from Python _default_eval_program.
pub const DEFAULT_EVAL_PROGRAM: &str = "You are an editor. Score a section on four dimensions.

Return ONLY a JSON object — no other text:
{
  \"accuracy\": <1-10>,
  \"coherence\": <1-10>,
  \"style\": <1-10>,
  \"source_integrity\": <1-10>,
  \"overall\": \"pass\" | \"fail\",
  \"hallucination_flags\": [
    {\"claim\": \"the unsupported claim\", \"severity\": \"critical|major|minor\", \"note\": \"why\"}
  ],
  \"notes\": \"editorial feedback — what's strong, what needs work, specific paragraph references\"
}

Rules:
1. Factual accuracy — do claims match the research notes? Flag anything hallucinated.
2. Argument coherence — does the logic flow? Are counterarguments engaged?
3. Style adherence — does the writing match the target style?
4. Source integrity — are citations used appropriately?
5. Be specific in notes — cite paragraph numbers.";

/// Instructions appended for the claim-level support matrix.
const CLAIM_MATRIX_RULES: &str = "\n\n## Claim Support Matrix\n\
    Decompose the section into up to 15 factual claims (skip pure opinion and \
    rhetorical framing). For each claim return:\n\
    {\"claim\": \"the claim as stated\", \"source_id\": \"src_01\" (empty string when uncited), \
    \"status\": \"supported\" | \"partial\" | \"unsupported\", \"note\": \"short reason\"}\n\
    Rules:\n\
    - \"supported\" only when the research notes / cited source explicitly supports it.\n\
    - \"partial\" when the source supports part of the claim or is weaker than the claim implies.\n\
    - \"unsupported\" when nothing in the provided material supports it.\n\
    - Do not invent source ids; use only ids that appear in the research notes.";

/// Assemble the evaluator's system prompt with thresholds from the style profile.
/// Mirrors Python _build_eval_system_prompt, plus claim-matrix instructions.
pub fn build_eval_system_prompt(style: &StyleProfile) -> String {
    let mut program = load_program_section("Phase 3: Evaluation (evaluate.py)");
    if program.is_empty() {
        program = DEFAULT_EVAL_PROGRAM.to_string();
    }
    program.push_str(CLAIM_MATRIX_RULES);

    let accuracy_display = style
        .accuracy_threshold
        .map(format_score)
        .unwrap_or_else(|| "N/A".to_string());
    let source_integrity_display = style
        .source_integrity_threshold
        .map(format_score)
        .unwrap_or_else(|| "N/A".to_string());

    let thresholds = format!(
        "\n\n## Quality Thresholds for {}\n- Accuracy: {} (minimum score to pass)\n- Coherence: {}\n- Style adherence: {}\n- Source integrity: {}\n\nScoring rules:\n- If any active threshold is not met → overall = fail\n- Hallucination gate: {}\n  * strict: flag every unsupported claim as critical → auto-fail\n  * lenient: flag major issues only → pass possible with minor flags\n  * off: skip hallucination detection entirely\n",
        style.name,
        accuracy_display,
        format_score(style.coherence_threshold),
        format_score(style.style_threshold),
        source_integrity_display,
        gate_label(&style.hallucination_gate),
    );

    program + &thresholds
}

/// Assemble the user prompt with the section to evaluate and source material.
///
/// Research notes come first (they are stable across every section, which keeps
/// the prompt prefix cache-friendly); the section and its mechanical metrics
/// follow.
pub fn build_eval_user_prompt(section_text: &str, research_notes: &str) -> String {
    let metrics = StyleMetrics::measure(section_text);
    format!(
        "## Research Notes (source of truth)\n\n{research_notes}\n\n## Section to Evaluate\n\n{section_text}\n\n## Mechanical Style Metrics\n\n{}\n\nEvaluate the section and return ONLY the JSON object (no other text).",
        metrics.summary()
    )
}

/// Score a drafted section on four quality dimensions plus a claim matrix.
///
/// Uses a different LLM provider/model than drafting (anti-self-review bias)
/// when exclude_provider is set.
pub async fn evaluate_section<C: LlmClient>(
    llm: &C,
    section_text: &str,
    research_notes: &str,
    style: &StyleProfile,
    exclude_provider: Option<&str>,
) -> CliResult<EvaluationResult> {
    evaluate_section_for_role(
        llm,
        section_text,
        research_notes,
        style,
        "evaluation",
        exclude_provider,
    )
    .await
}

/// Evaluate with a second judge role and merge the two evaluations.
///
/// The second call is best-effort: if it fails, the primary evaluation is
/// returned so a judge hiccup cannot break a run. Scores are averaged; the
/// merged result passes only when both judges pass.
pub async fn evaluate_section_ensemble<C: LlmClient>(
    llm: &C,
    section_text: &str,
    research_notes: &str,
    style: &StyleProfile,
    exclude_provider: Option<&str>,
    secondary_role: &str,
) -> CliResult<EvaluationResult> {
    let primary =
        evaluate_section(llm, section_text, research_notes, style, exclude_provider).await?;
    match evaluate_section_for_role(
        llm,
        section_text,
        research_notes,
        style,
        secondary_role,
        None,
    )
    .await
    {
        Ok(secondary) => Ok(merge_evaluations(primary, secondary)),
        Err(_) => Ok(primary),
    }
}

async fn evaluate_section_for_role<C: LlmClient>(
    llm: &C,
    section_text: &str,
    research_notes: &str,
    style: &StyleProfile,
    role: &str,
    exclude_provider: Option<&str>,
) -> CliResult<EvaluationResult> {
    let system_prompt = build_eval_system_prompt(style);
    let user_prompt = build_eval_user_prompt(section_text, research_notes);

    let mut request = ChatRequest::for_role(role, vec![ChatMessage::user(user_prompt)])
        .system(system_prompt)
        .temperature(0.3) // low temp for consistent scoring
        .max_tokens(2048)
        .response_format(eval_response_format());

    if let Some(exclude) = exclude_provider {
        request = request.exclude(exclude);
    }

    let resp = llm.chat(request).await?;
    Ok(parse_eval_response(&resp.content, style))
}

/// Merge a primary and secondary evaluation for ensemble judging.
pub fn merge_evaluations(
    primary: EvaluationResult,
    secondary: EvaluationResult,
) -> EvaluationResult {
    let average = |a: f64, b: f64| (a + b) / 2.0;
    let mut flags = primary.hallucination_flags.clone();
    flags.extend(secondary.hallucination_flags.clone());
    let mut claims = primary.claims.clone();
    claims.extend(secondary.claims.clone());
    EvaluationResult {
        accuracy: average(primary.accuracy, secondary.accuracy),
        coherence: average(primary.coherence, secondary.coherence),
        style: average(primary.style, secondary.style),
        source_integrity: average(primary.source_integrity, secondary.source_integrity),
        overall: if primary.passed() && secondary.passed() {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
        hallucination_flags: flags,
        claims,
        notes: format!(
            "{}\n\n--- Second judge ---\n{}",
            primary.notes, secondary.notes
        ),
    }
}

/// JSON Schema for the evaluator response, sent as `response_format` where the
/// endpoint supports structured outputs (our OpenRouter models do).
pub fn eval_response_format() -> autoessay_providers::ResponseFormat {
    autoessay_providers::ResponseFormat::JsonSchema {
        name: "evaluation".to_string(),
        strict: true,
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "accuracy": {"type": "number"},
                "coherence": {"type": "number"},
                "style": {"type": "number"},
                "source_integrity": {"type": "number"},
                "overall": {"type": "string", "enum": ["pass", "fail"]},
                "hallucination_flags": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "claim": {"type": "string"},
                            "severity": {"type": "string", "enum": ["critical", "major", "minor"]},
                            "note": {"type": "string"}
                        },
                        "required": ["claim", "severity", "note"],
                        "additionalProperties": false
                    }
                },
                "claims": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "claim": {"type": "string"},
                            "source_id": {"type": "string"},
                            "status": {"type": "string", "enum": ["supported", "partial", "unsupported"]},
                            "note": {"type": "string"}
                        },
                        "required": ["claim", "source_id", "status", "note"],
                        "additionalProperties": false
                    }
                },
                "notes": {"type": "string"}
            },
            "required": [
                "accuracy", "coherence", "style", "source_integrity",
                "overall", "hallucination_flags", "claims", "notes"
            ],
            "additionalProperties": false
        }),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Response parsing — the fragile part, ported exactly
// ═══════════════════════════════════════════════════════════════════════════

/// Extract structured evaluation from LLM output.
///
/// Handles JSON in code fences, bare JSON, and partial JSON. Mirrors Python
/// _parse_eval_response: on unparseable output it returns a failed evaluation
/// with a descriptive note rather than raising.
pub fn parse_eval_response(raw: &str, style: &StyleProfile) -> EvaluationResult {
    // Try to extract JSON from the response
    let json_str = extract_json(raw);

    let data: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => {
            // Fallback: return a failed evaluation
            let mut snippet: String = raw.chars().take(200).collect();
            if raw.chars().count() > 200 {
                snippet.push_str("...");
            }
            return failed_with_notes(format!(
                "Evaluation failed to produce valid JSON. Raw output: {snippet}"
            ));
        }
    };

    let flags: Vec<HallucinationFlag> = data
        .get("hallucination_flags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f.as_object())
                .map(|obj| HallucinationFlag {
                    claim: obj
                        .get("claim")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    severity: obj
                        .get("severity")
                        .and_then(|v| v.as_str())
                        .map(parse_severity)
                        .unwrap_or(Severity::Minor),
                    note: obj
                        .get("note")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let claims: Vec<ClaimSupport> = data
        .get("claims")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_object())
                .map(|obj| ClaimSupport {
                    claim: obj
                        .get("claim")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    source_id: obj
                        .get("source_id")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_string),
                    status: obj
                        .get("status")
                        .and_then(|v| v.as_str())
                        .map(parse_claim_status)
                        .unwrap_or(ClaimStatus::Unsupported),
                    note: obj
                        .get("note")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    // Determine pass/fail based on style profile thresholds
    let mut overall = "pass".to_string();
    let gate = style.hallucination_gate;
    if gate == autoessay_core::HallucinationGate::Strict
        && flags.iter().any(|f| f.severity == Severity::Critical)
    {
        overall = "fail".to_string();
    }
    if gate == autoessay_core::HallucinationGate::Lenient
        && flags.iter().filter(|f| f.severity == Severity::Critical).count() >= 2
    {
        overall = "fail".to_string();
    }
    if let Some(threshold) = style.accuracy_threshold {
        if get_number(&data, "accuracy").unwrap_or(0.0) < threshold {
            overall = "fail".to_string();
        }
    }
    if get_number(&data, "coherence").unwrap_or(0.0) < style.coherence_threshold {
        overall = "fail".to_string();
    }
    if get_number(&data, "style").unwrap_or(0.0) < style.style_threshold {
        overall = "fail".to_string();
    }
    if let Some(threshold) = style.source_integrity_threshold {
        if get_number(&data, "source_integrity").unwrap_or(0.0) < threshold {
            overall = "fail".to_string();
        }
    }

    EvaluationResult {
        accuracy: get_number(&data, "accuracy").unwrap_or(0.0),
        coherence: get_number(&data, "coherence").unwrap_or(0.0),
        style: get_number(&data, "style").unwrap_or(0.0),
        source_integrity: get_number(&data, "source_integrity").unwrap_or(0.0),
        overall,
        hallucination_flags: flags,
        claims,
        notes: data
            .get("notes")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    }
}

/// A failed evaluation carrying a human-readable explanation.
fn failed_with_notes(notes: impl Into<String>) -> EvaluationResult {
    EvaluationResult {
        accuracy: 0.0,
        coherence: 0.0,
        style: 0.0,
        source_integrity: 0.0,
        overall: "fail".to_string(),
        hallucination_flags: Vec::new(),
        claims: Vec::new(),
        notes: notes.into(),
    }
}

/// Extract JSON from LLM output — handles code fences, wrapping prose, and
/// partial fragments.
///
/// Step 1 mirrors Python _extract_json exactly (code-fence stripping, then
/// starts_with("{"), then a DOTALL \{.*\} search). Because \{.*\} is greedy it
/// spans from the first brace to the LAST brace in the text, which would swallow
/// trailing prose when the model appends a note after the closing brace. The
/// task explicitly asks for tolerance of "JSON with trailing prose", so step 2
/// adds a brace-balanced scan that isolates the first complete object. This is a
/// strict improvement over the Python behavior for that input and never changes
/// the outcome when the JSON is well-formed on its own.
pub fn extract_json(raw: &str) -> String {
    // Strip markdown code fences (leading then trailing).
    let stripped = raw.trim();
    let lead_re = Regex::new(r"^\x60\x60\x60(?:json)?\s*").ok();
    let tail_re = Regex::new(r"\s*\x60\x60\x60$").ok();

    let mut cleaned = stripped.to_string();
    if let Some(re) = lead_re {
        cleaned = re.replace(&cleaned, "").to_string();
    }
    if let Some(re) = tail_re {
        cleaned = re.replace(&cleaned, "").to_string();
    }

    let trimmed = cleaned.trim();

    // If it starts with {, use as-is — unless there is trailing prose after a
    // balanced object, in which case trim to the object.
    if trimmed.starts_with('{') {
        return balanced_object(trimmed).unwrap_or_else(|| trimmed.to_string());
    }

    // Try to find a JSON object in the text (spans newlines, like Python's
    // re.DOTALL for r"\{.*\}")
    let obj_re = Regex::new(r"(?s)\{.*\}").ok();
    if let Some(re) = obj_re {
        if let Some(m) = re.find(trimmed) {
            return balanced_object(m.as_str()).unwrap_or_else(|| m.as_str().to_string());
        }
    }

    cleaned
}

/// Return the first brace-balanced JSON object in s, if one exists. Scans while
/// tracking string literals and escapes so braces inside strings do not confuse
/// the count. Returns None when the braces never balance (truncated JSON) or the
/// text does not start with '{'.
fn balanced_object(s: &str) -> Option<String> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a model's severity label into the typed enum. Unknown labels fall back
/// to Minor (matching the Python default).
fn parse_severity(s: &str) -> Severity {
    match s.trim().to_ascii_lowercase().as_str() {
        "critical" => Severity::Critical,
        "major" => Severity::Major,
        _ => Severity::Minor,
    }
}

/// Parse a claim-support status. Unknown labels fall back to Unsupported.
fn parse_claim_status(s: &str) -> ClaimStatus {
    match s.trim().to_ascii_lowercase().as_str() {
        "supported" => ClaimStatus::Supported,
        "partial" => ClaimStatus::Partial,
        _ => ClaimStatus::Unsupported,
    }
}

/// Read a numeric field, tolerating integers or floats in the model output.
fn get_number(data: &serde_json::Value, key: &str) -> Option<f64> {
    data.get(key).and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_i64().map(|i| i as f64))
            .or_else(|| v.as_u64().map(|u| u as f64))
    })
}

/// Format a score/threshold for a prompt. Python used str(float), which renders
/// 7.0 as "7.0" and 8.5 as "8.5"; match that.
pub fn format_score(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// The label for the hallucination gate in the system prompt.
fn gate_label(gate: &autoessay_core::HallucinationGate) -> &'static str {
    match gate {
        autoessay_core::HallucinationGate::Strict => "strict",
        autoessay_core::HallucinationGate::Lenient => "lenient",
        autoessay_core::HallucinationGate::Off => "off",
    }
}
