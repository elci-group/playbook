//! The live draftboard: the roster and draftboard exposed to a real model
//! list (Project VALUE, H5 in miniature). Every free model on the provider
//! is registered as franchise personnel, plays the same small corpus of
//! mechanically verified tasks, and the measured record — completion,
//! tokens, latency — is what the draftboard ranks. Self-report counts for
//! nothing: a task is complete only when its validator passes.

use serde::{Deserialize, Serialize};

/// One mechanically verified task: exact expected output after light
/// normalisation (trim, code-fence strip, case-fold for word answers).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveTask {
    pub id: String,
    pub prompt: String,
    pub expected: String,
    /// Word answers compare case-insensitively; code and JSON are exact.
    pub case_insensitive: bool,
}

pub fn corpus() -> Vec<LiveTask> {
    vec![
        LiveTask {
            id: "arithmetic".into(),
            prompt: "Reply with exactly the number: 17 * 23. No other text.".into(),
            expected: "391".into(),
            case_insensitive: false,
        },
        LiveTask {
            id: "capital".into(),
            prompt: "Reply with exactly one word: the capital of France.".into(),
            expected: "paris".into(),
            case_insensitive: true,
        },
        LiveTask {
            id: "instruction_following".into(),
            prompt: "Output exactly this string and nothing else: PLAYBOOK-EVIDENCE-GATE".into(),
            expected: "PLAYBOOK-EVIDENCE-GATE".into(),
            case_insensitive: false,
        },
        LiveTask {
            id: "counting".into(),
            prompt: "Reply with exactly the number of letters in the word 'orchestra'.".into(),
            expected: "9".into(),
            case_insensitive: false,
        },
        LiveTask {
            id: "json".into(),
            prompt: "Output exactly this JSON and nothing else: {\"status\":\"ok\"}".into(),
            expected: "{\"status\":\"ok\"}".into(),
            case_insensitive: false,
        },
        LiveTask {
            id: "planet".into(),
            prompt: "Reply with exactly one word: the second planet from the sun.".into(),
            expected: "venus".into(),
            case_insensitive: true,
        },
    ]
}

fn normalise(text: &str, case_insensitive: bool) -> String {
    let mut out = text.trim().to_string();
    // Strip markdown code fences the models like to wrap answers in.
    if out.starts_with("```") {
        out = out
            .trim_start_matches("```")
            .trim_start_matches(|c: char| c.is_alphanumeric())
            .trim()
            .to_string();
        if let Some(end) = out.rfind("```") {
            out = out[..end].trim().to_string();
        }
    }
    if (out.starts_with('"') && out.ends_with('"') && out.len() > 1)
        || (out.starts_with('\'') && out.ends_with('\'') && out.len() > 1)
    {
        out = out[1..out.len() - 1].to_string();
    }
    let out = out.trim().trim_end_matches('.').trim().to_string();
    if case_insensitive {
        out.to_lowercase()
    } else {
        out
    }
}

/// Whether the model's answer satisfies the task's Deliver Gate.
pub fn validated(task: &LiveTask, answer: &str) -> bool {
    normalise(answer, task.case_insensitive) == normalise(&task.expected, task.case_insensitive)
}

/// One raw completion from the provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub content: String,
    pub total_tokens: u64,
    pub latency_ms: u64,
    /// `ok` | the failure reason (http status, timeout, parse error).
    pub status: String,
}

/// One chat completion over the wire via the system `curl` — the same
/// external-command pattern every Playbook adapter uses. The key is read
/// from the file, never logged.
pub fn chat(key: &str, model: &str, prompt: &str, timeout_secs: u64) -> Completion {
    use std::time::Instant;
    let started = Instant::now();
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": 200,
        "temperature": 0,
    });
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "--max-time",
            &timeout_secs.to_string(),
            "https://openrouter.ai/api/v1/chat/completions",
            "-H",
            "content-type: application/json",
            "-H",
            &format!("authorization: Bearer {key}"),
            "-d",
            &body.to_string(),
        ])
        .output();
    let latency_ms = started.elapsed().as_millis() as u64;
    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(parsed) => {
                    let content = parsed["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    let tokens = parsed["usage"]["total_tokens"].as_u64().unwrap_or(0);
                    if content.is_empty() && parsed["error"].is_object() {
                        Completion {
                            content: String::new(),
                            total_tokens: tokens,
                            latency_ms,
                            status: format!(
                                "provider error: {}",
                                parsed["error"]["message"].as_str().unwrap_or("unknown")
                            ),
                        }
                    } else {
                        Completion {
                            content,
                            total_tokens: tokens,
                            latency_ms,
                            status: "ok".into(),
                        }
                    }
                }
                Err(error) => Completion {
                    content: String::new(),
                    total_tokens: 0,
                    latency_ms,
                    status: format!("unparseable response: {error}"),
                },
            }
        }
        Ok(output) => Completion {
            content: String::new(),
            total_tokens: 0,
            latency_ms,
            status: format!("http {}", output.status),
        },
        Err(error) => Completion {
            content: String::new(),
            total_tokens: 0,
            latency_ms,
            status: format!("spawn failed: {error}"),
        },
    }
}

/// The free models the provider currently lists (§10: the frozen pool).
pub fn free_models(key: &str, limit: usize) -> Vec<String> {
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "20",
            "https://openrouter.ai/api/v1/models",
            "-H",
            &format!("authorization: Bearer {key}"),
        ])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
    let mut models: Vec<String> = parsed["data"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry["id"].as_str())
                .filter(|id| id.ends_with(":free"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    models.truncate(limit);
    models
}

/// One model's measured record over the corpus — its draftboard row and
/// its roster entry, from measurement alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftRow {
    /// The provider model id.
    pub model: String,
    /// The roster-safe personnel id (provider ids contain `/`, which the
    /// roster's id grammar forbids).
    pub personnel_id: String,
    pub completed: u32,
    pub total: u32,
    pub total_tokens: u64,
    pub median_latency_ms: u64,
    pub completion_rate_ppm: u32,
    pub failures: Vec<String>,
}

/// Play the corpus: every model plays every task, every answer is
/// mechanically validated. Sequential and honest — failures are recorded,
/// never retried into success.
pub fn play(key: &str, models: &[String], tasks: &[LiveTask]) -> Vec<DraftRow> {
    let mut rows = Vec::new();
    for model in models {
        let mut completed = 0u32;
        let mut total_tokens = 0u64;
        let mut latencies: Vec<u64> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        for task in tasks {
            let completion = chat(key, model, &task.prompt, 60);
            total_tokens += completion.total_tokens;
            latencies.push(completion.latency_ms);
            if completion.status == "ok" && validated(task, &completion.content) {
                completed += 1;
            } else {
                failures.push(format!(
                    "{}: {} (answer: {:?})",
                    task.id,
                    completion.status,
                    truncate(&completion.content, 60)
                ));
            }
        }
        latencies.sort();
        rows.push(DraftRow {
            model: model.clone(),
            personnel_id: model.replace('/', "--"),
            completed,
            total: tasks.len() as u32,
            total_tokens,
            median_latency_ms: latencies
                .get(latencies.len() / 2)
                .copied()
                .unwrap_or(0),
            completion_rate_ppm: ((u64::from(completed) * 1_000_000)
                / tasks.len().max(1) as u64) as u32,
            failures,
        });
    }
    rows.sort_by(|a, b| {
        b.completed
            .cmp(&a.completed)
            .then(a.median_latency_ms.cmp(&b.median_latency_ms))
    });
    rows
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validators_are_strict_where_the_gate_is_strict() {
        let json_task = &corpus()[4];
        assert!(validated(json_task, "{\"status\":\"ok\"}"));
        assert!(validated(json_task, "```json\n{\"status\":\"ok\"}\n```"));
        assert!(!validated(json_task, "{\"status\":\"OK\"}"));
        let capital = &corpus()[1];
        assert!(validated(capital, "Paris."));
        assert!(validated(capital, "  paris  "));
        assert!(!validated(capital, "Lyon"));
        let arithmetic = &corpus()[0];
        assert!(validated(arithmetic, "391"));
        assert!(!validated(arithmetic, "The answer is 391."));
    }

    #[test]
    fn personnel_ids_are_roster_safe() {
        for model in ["nvidia/nemotron:free", "cohere/north-mini-code:free"] {
            let id = model.replace('/', "--");
            assert!(!id.contains('/'));
            assert!(playbook_core::PersonnelId::new(&id).is_ok());
        }
    }
}
