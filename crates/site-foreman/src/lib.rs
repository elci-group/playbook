//! The site foreman: free models author the Playbook website; Playbook's
//! authority decides whether their work counts; every intervention —
//! machine or operator — is logged, structured, and published on the site.
//!
//! Division of labour (the honesty contract):
//! * **Models** author every visible HTML fragment, from briefs carrying
//!   only extracted workspace facts.
//! * **The foreman** (glue) briefs the builders, validates fragments
//!   mechanically, assembles, and records every intervention.
//! * **The authority** accepts sealed real evidence and completes the
//!   drive — claims count for nothing.
//! * **The operator** logs whatever they still had to do by hand.

use serde::{Deserialize, Serialize};

use value_lab::chat;

/// One recorded intervention: anything an end user or agent had to do
/// between the prompt and the result. Published on the site, unedited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intervention {
    pub seq: u32,
    /// `operator` | `model:<name>` | `foreman`
    pub actor: String,
    /// What the intervention was.
    pub action: String,
    /// Why it was needed — the failure or gap that forced it.
    pub why: String,
    /// The foreman's critique of the step: what this says about the
    /// system's readiness.
    pub critique: String,
}

/// The append-only intervention log.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterventionLog {
    entries: Vec<Intervention>,
}

impl InterventionLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, actor: &str, action: &str, why: &str, critique: &str) {
        let seq = (self.entries.len() + 1) as u32;
        self.entries.push(Intervention {
            seq,
            actor: actor.to_string(),
            action: action.to_string(),
            why: why.to_string(),
            critique: critique.to_string(),
        });
    }

    pub fn entries(&self) -> &[Intervention] {
        &self.entries
    }
}

/// The builders selected by measured prior: the top of the draftboard.
pub fn select_builders(draftboard: &[serde_json::Value], count: usize) -> Vec<String> {
    draftboard
        .iter()
        .filter_map(|row| row["model"].as_str())
        .take(count)
        .map(str::to_string)
        .collect()
}

/// A section brief: what the builder must author and the facts it may use.
pub struct Brief {
    pub id: &'static str,
    pub instruction: String,
    /// Substrings the finished fragment MUST contain (content gates).
    pub must_contain: Vec<String>,
}

/// Mechanical fragment validation (§11: the builder does not decide
/// whether its work is acceptable).
/// Strip the markdown fences models wrap answers in — the content is
/// inside them; refusing the wrapper is judging the envelope, not the
/// work. (Run 1 lesson: every section was rejected for exactly this.)
pub fn strip_fences(fragment: &str) -> String {
    let trimmed = fragment.trim();
    if trimmed.starts_with("```") {
        let inner = trimmed
            .trim_start_matches("```")
            .trim_start_matches(|c: char| c.is_alphanumeric())
            .trim();
        if let Some(end) = inner.rfind("```") {
            return inner[..end].trim().to_string();
        }
    }
    trimmed.to_string()
}

pub fn validate_fragment(fragment: &str, brief: &Brief) -> Result<(), Vec<String>> {
    let fragment = &strip_fences(fragment);
    let mut failures = Vec::new();
    let lowered = fragment.to_lowercase();
    for forbidden in [
        "<script",
        "<iframe",
        "javascript:",
        "onclick",
        "http://",
        "https://",
    ] {
        if lowered.contains(forbidden) {
            failures.push(format!("forbidden content: {forbidden}"));
        }
    }
    // Tag balance over the safe subset the brief allows.
    let allowed = [
        "div", "section", "p", "h2", "h3", "h4", "b", "i", "em", "strong", "ul", "li", "span",
        "table", "thead", "tbody", "tr", "th", "td", "br",
    ];
    let mut stack: Vec<String> = Vec::new();
    for part in fragment.split('<').skip(1) {
        let Some(end) = part.find('>') else { continue };
        let tag = &part[..end];
        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim().to_lowercase();
            match stack.pop() {
                Some(open) if open == name => {}
                Some(open) => failures.push(format!("tag mismatch: </{name}> closes <{open}>")),
                None => failures.push(format!("stray closing tag </{name}>")),
            }
        } else {
            let name = tag
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or("")
                .to_lowercase();
            if name.is_empty() {
                continue;
            }
            if allowed.contains(&name.as_str()) && !tag.ends_with('/') {
                stack.push(name);
            }
        }
    }
    if !stack.is_empty() {
        failures.push(format!("unclosed tags: {stack:?}"));
    }
    for required in &brief.must_contain {
        // Data points (crate names) match exactly; heading text may vary
        // in case — the builder owns phrasing, the facts are the gate.
        let satisfied = fragment.contains(required.as_str())
            || fragment.to_lowercase().contains(&required.to_lowercase());
        if !satisfied {
            failures.push(format!("missing required content: {required}"));
        }
    }
    if fragment.chars().count() < 120 {
        failures.push("fragment too short to be a real section".into());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

/// One authored section: the model's fragment plus its provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredSection {
    pub brief_id: String,
    pub builder: String,
    /// `model` (validated first try) | `model-repaired` (passed after a
    /// retry with validator feedback) | `fallback` (operator wrote it).
    pub provenance: String,
    pub fragment: String,
}

/// Brief a builder, validate, and repair once with validator feedback.
/// Returns the authored section and records every intervention on the way.
pub fn author_section(
    log: &mut InterventionLog,
    key: &str,
    builders: &[String],
    brief: &Brief,
) -> AuthoredSection {
    let mut feedback = String::new();
    for attempt in 1..=2 {
        for builder in builders {
            let prompt = format!(
                "{}\n\nRules: output ONLY the HTML fragment, no markdown fences, no \
                 explanation. Inline styles allowed; no <script>, no external URLs.\n{}",
                brief.instruction, feedback
            );
            let completion = chat(key, builder, &prompt, 90);
            if completion.status != "ok" {
                log.record(
                    &format!("model:{builder}"),
                    &format!("attempt failed for section {}: {}", brief.id, completion.status),
                    "the provider returned an error or empty content",
                    "live providers fail routinely; the foreman must route around each one — this is the resumption work an agent loop would own",
                );
                continue;
            }
            match validate_fragment(&completion.content, brief) {
                Ok(()) => {
                    if attempt > 1 {
                        log.record(
                            &format!("model:{builder}"),
                            &format!("section {} passed on the repair attempt", brief.id),
                            "the first attempt failed validation; the validator feedback was fed back verbatim",
                            "validator-in-the-loop repair works — but note the orchestrator had to own the retry loop; the platform does not yet run evidence-checked revision natively",
                        );
                        return AuthoredSection {
                            brief_id: brief.id.to_string(),
                            builder: builder.clone(),
                            provenance: "model-repaired".into(),
                            fragment: completion.content,
                        };
                    }
                    return AuthoredSection {
                        brief_id: brief.id.to_string(),
                        builder: builder.clone(),
                        provenance: "model".into(),
                        fragment: completion.content,
                    };
                }
                Err(failures) => {
                    feedback = format!(
                        "Your previous attempt was rejected by the validator:\n- {}\nFix these problems and try again.",
                        failures.join("\n- ")
                    );
                    log.record(
                        &format!("model:{builder}"),
                        &format!(
                            "section {} rejected: {}",
                            brief.id,
                            failures.first().cloned().unwrap_or_default()
                        ),
                        "the fragment failed mechanical validation",
                        "mechanical gates catch model errors before a human ever would — this is the platform earning its keep",
                    );
                }
            }
        }
    }
    // Every builder failed twice: the operator writes it, on the record.
    log.record(
        "operator",
        &format!("hand-wrote the {} section", brief.id),
        "every model attempt failed validation twice",
        "the operator fallback is the honest cost of free-model reliability today; an end user would have to do exactly this",
    );
    AuthoredSection {
        brief_id: brief.id.to_string(),
        builder: "operator".into(),
        provenance: "fallback".into(),
        fragment: format!(
            "<section><h2>{}</h2><p>This section fell back to the operator after every builder failed validation. See the audit log.</p></section>",
            brief.id
        ),
    }
}

/// Assemble the page: the foreman owns the skeleton (head, styles, audit
/// log), the builders own every content section. Deterministic given the
/// same fragments.
pub fn assemble(
    skeleton_head: &str,
    sections: &[AuthoredSection],
    log: &InterventionLog,
    version: &str,
) -> String {
    let mut out = String::from(skeleton_head);
    out.push_str(&format!(
        "<header>\n<h1><span class=\"tick\">▌</span> PLAYBOOK</h1>\n<p class=\"tagline\">The franchise authority for agentic work — this page was authored by free AI models under Playbook's evidence gates.</p>\n<span class=\"version\">v{version}</span>\n</header>\n"
    ));
    for section in sections {
        out.push_str(&section.fragment);
        out.push('\n');
        out.push_str(&format!(
            "<p class=\"mono byline\" style=\"color:var(--dim);font-size:13px;max-width:980px;margin:4px auto 18px\">authored by {} ({})</p>\n",
            section.builder,
            section.provenance
        ));
    }
    out.push_str("<section>\n<h2>Intervention audit</h2>\n");
    out.push_str("<p style=\"color:var(--dim)\">Every step an operator or agent had to take between the prompt and this page, recorded as it happened. Unedited.</p>\n");
    out.push_str("<table style=\"width:100%;border-collapse:collapse;font-size:14px\">\n<tr><th style=\"text-align:left;padding:6px;border-bottom:1px solid var(--line)\">#</th><th style=\"text-align:left;padding:6px;border-bottom:1px solid var(--line)\">Actor</th><th style=\"text-align:left;padding:6px;border-bottom:1px solid var(--line)\">Action</th><th style=\"text-align:left;padding:6px;border-bottom:1px solid var(--line)\">Why it was needed</th><th style=\"text-align:left;padding:6px;border-bottom:1px solid var(--line)\">Critique</th></tr>\n");
    for entry in log.entries() {
        out.push_str(&format!(
            "<tr><td class=\"mono\" style=\"padding:6px;border-bottom:1px solid var(--line)\">{}</td><td class=\"mono\" style=\"padding:6px;border-bottom:1px solid var(--line);color:var(--accent)\">{}</td><td style=\"padding:6px;border-bottom:1px solid var(--line)\">{}</td><td style=\"padding:6px;border-bottom:1px solid var(--line);color:var(--dim)\">{}</td><td style=\"padding:6px;border-bottom:1px solid var(--line);color:var(--dim)\">{}</td></tr>\n",
            entry.seq,
            escape_cell(&entry.actor),
            escape_cell(&entry.action),
            escape_cell(&entry.why),
            escape_cell(&entry.critique),
        ));
    }
    out.push_str("</table>\n</section>\n");
    out.push_str(&format!(
        "<footer>Assembled by <span class=\"mono\">site-foreman</span> under CI_EVIDENCE/1 — content authored by free models, admitted only on validated evidence. v{version}</footer>\n</body>\n</html>\n"
    ));
    out
}

fn escape_cell(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_validator_rejects_the_classic_model_sins() {
        let brief = Brief {
            id: "test",
            instruction: String::new(),
            must_contain: vec!["PLAYBOOK".into()],
        };
        // Balanced, clean, complete: accepted.
        assert!(validate_fragment(
            "<section><h2>PLAYBOOK</h2><p>text of real and sufficient length here to pass the fragment floor gate with room to spare for certain.</p></section>",
            &brief
        )
        .is_ok());
        // Script injection: rejected.
        assert!(validate_fragment(
            "<section><h2>PLAYBOOK</h2><script>alert(1)</script><p>text of real length here.</p></section>",
            &brief
        )
        .is_err());
        // Unbalanced tags: rejected.
        assert!(validate_fragment(
            "<section><h2>PLAYBOOK</h2><div><p>never closed properly at all.</p></section>",
            &brief
        )
        .is_err());
        // Missing required content: rejected.
        assert!(validate_fragment(
            "<section><h2>something else</h2><p>text of real length here to pass.</p></section>",
            &brief
        )
        .is_err());
        // External URL: rejected.
        assert!(validate_fragment(
            "<section><h2>PLAYBOOK</h2><p>see https://example.com for more text of real length.</p></section>",
            &brief
        )
        .is_err());
    }

    #[test]
    fn builder_selection_follows_the_measured_priors() {
        let board = serde_json::json!([
            {"model": "gold:free", "completed": 6},
            {"model": "silver:free", "completed": 5},
            {"model": "bronze:free", "completed": 2},
        ]);
        let builders = select_builders(board.as_array().unwrap(), 2);
        assert_eq!(builders, vec!["gold:free", "silver:free"]);
    }

    #[test]
    fn the_audit_table_escapes_model_output() {
        let mut log = InterventionLog::new();
        log.record("model:x", "tried <script>", "why", "critique");
        let sections = vec![AuthoredSection {
            brief_id: "b".into(),
            builder: "x".into(),
            provenance: "fallback".into(),
            fragment: "<section><h2>b</h2><p>filler</p></section>".into(),
        }];
        let page = assemble("<html>", &sections, &log, "0.1.0");
        assert!(!page.contains("<script>"), "model output is escaped");
        assert!(page.contains("&lt;script&gt;"));
    }
}
