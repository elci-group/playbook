//! `dev-foreman` — the nemotron swarm authors the §46 fixture families
//! through Playbook's governance: dispatch via the CapabilityPlane,
//! engine-verified validation, RRP-driven repair of the authoring
//! conditions, operator fallback only as recorded escalation.

use capability_plane::{CapabilityPlane, OpenRouterPlane};
use playbook_resolution::{classify, prescribe, Action, RefusalObservation, ResolutionPolicy};

use dev_foreman::fixture::{validate, Fixture, FAMILIES};

const USAGE: &str = "dev-foreman — the nemotron swarm authors the §46 fixture corpus

usage:
  dev-foreman --key-file <path> --out <fixtures-dir> [--per-family N] [--draftboard <path>]";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let Some(key_file) = flag("--key-file") else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let out_dir = flag("--out").unwrap_or_else(|| ".".into());
    let per_family = flag("--per-family")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(4);
    let draftboard_path = flag("--draftboard").unwrap_or_else(|| "draftboard.json".into());
    let plane = OpenRouterPlane::from_key_file(std::path::Path::new(&key_file))
        .expect("plane initialisation");

    // The swarm is nemotron: workers selected from the measured draftboard
    let workers: Vec<String> = std::fs::read_to_string(&draftboard_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|board| {
            let rows = board.as_array()?;
            let nemotron: Vec<String> = rows
                .iter()
                .filter_map(|row| row["model"].as_str())
                .filter(|model| model.contains("nemotron"))
                .map(str::to_string)
                .collect();
            (!nemotron.is_empty()).then_some(nemotron)
        })
        .unwrap_or_else(|| {
            vec![
                "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free".into(),
                "nvidia/nemotron-3.5-lightning:free".into(),
            ]
        });
    println!("▌ DEV FOREMAN — nemotron swarm: {workers:?}");

    let policy = ResolutionPolicy::default();
    let mut authored: Vec<Fixture> = Vec::new();
    let mut refusals_classified = 0u32;
    let mut repairs_applied = 0u32;
    let mut escalations = 0u32;
    let mut operator_interventions = 0u32;

    for family in FAMILIES {
        println!("[foreman] family: {family} ({per_family} work units)");
        let brief = family_brief(family, per_family);
        let mut feedback = String::new();
        let envelope = policy.token_envelope.max(4_000);
        for index in 0..per_family {
            let mut attempts = 0u32;
            let mut accepted_fixtures: Option<Vec<Fixture>> = None;
            let mut envelope_local = envelope;
            while attempts < policy.budget.max_remediation_attempts && accepted_fixtures.is_none() {
                attempts += 1;
                let prompt = format!(
                    "{}\n{}\n\nOutput ONLY a JSON array with exactly {} element(s), no fences.",
                    brief, feedback, 1
                );
                let envelope_struct = capability_plane::ExecutionEnvelope {
                    request_id: "{family}-{index}-{attempts}".to_string(),
                    model: workers[(attempts as usize) % workers.len()].clone(),
                    prompt,
                    max_tokens: u32::try_from(envelope_local).unwrap_or(u32::MAX),
                    temperature: 0.3,
                    timeout_secs: 120,
                };
                let handle = plane.dispatch(&envelope_struct);
                let outcome = plane.reconcile(handle);
                let content = outcome.content.clone().unwrap_or_default();
                let parsed: Option<Vec<serde_json::Value>> = {
                    let stripped = work_adaptation::normalise_observed_answer(&content);
                    let stripped = stripped
                        .trim_start_matches("```json")
                        .trim_start_matches("```")
                        .trim_end_matches("```")
                        .trim()
                        .to_string();
                    serde_json::from_str(&stripped).ok()
                };
                let Some(entries) = parsed else {
                    let observation = RefusalObservation {
                        finish_reason: outcome.finish_reason.clone(),
                        provider_status: outcome.provider_status.clone(),
                        exit_code: None,
                        gate_failures: vec!["output is not a JSON array".into()],
                        output_structurally_complete: outcome.finish_reason.as_deref()
                            == Some("stop"),
                        artefact_expected_but_missing: false,
                        output_path_contended: false,
                        version_control_failed: false,
                        prior_attempts_with_same_failures: attempts.saturating_sub(1),
                        semantic_requirements_met_after_normalisation: None,
                        first_required_item_missing: None,
                    };
                    let diagnosis = classify(&observation);
                    let prescription =
                        prescribe(&diagnosis, &policy, attempts, outcome.total_tokens, false);
                    refusals_classified += 1;
                    println!(
                        "[rrp] {family}#{index}: unparseable — {} → {}",
                        diagnosis.primary.key(),
                        prescription
                            .actions
                            .iter()
                            .map(|a| a.key())
                            .collect::<Vec<_>>()
                            .join("+")
                    );
                    if prescription.escalated {
                        escalations += 1;
                        operator_interventions += 1;
                        break;
                    }
                    for action in &prescription.actions {
                        match action {
                            Action::ExpandEnvelope => {
                                envelope_local = prescription
                                    .suggested_envelope
                                    .unwrap_or(envelope_local * 4)
                                    .max(envelope_local);
                            }
                            Action::NormaliseBrief => {
                                feedback = "The previous output was rejected: it was not a bare JSON array. Output ONLY the array.".to_string();
                            }
                            _ => {}
                        }
                    }
                    repairs_applied += 1;
                    continue;
                };
                let mut accepted_here: Vec<Fixture> = Vec::new();
                let mut all_failures: Vec<String> = Vec::new();
                for entry in &entries {
                    let fixture: Result<Fixture, _> = serde_json::from_value(entry.clone());
                    let Ok(fixture) = fixture else {
                        all_failures.push("an element does not match the fixture schema".into());
                        continue;
                    };
                    match validate(&fixture) {
                        Ok(()) => accepted_here.push(fixture),
                        Err(failures) => all_failures.extend(failures),
                    }
                }
                if !accepted_here.is_empty() {
                    for fixture in &accepted_here {
                        println!(
                            "[engine-verified] {} ({} family)",
                            fixture.fixture_id, fixture.family
                        );
                    }
                    accepted_fixtures = Some(accepted_here);
                    break;
                }
                // Every prediction refuted: classify and repair.
                let observation = RefusalObservation {
                    finish_reason: outcome.finish_reason.clone(),
                    provider_status: outcome.provider_status.clone(),
                    exit_code: None,
                    gate_failures: all_failures.clone(),
                    output_structurally_complete: outcome.finish_reason.as_deref() == Some("stop"),
                    artefact_expected_but_missing: false,
                    output_path_contended: false,
                    version_control_failed: false,
                    prior_attempts_with_same_failures: attempts.saturating_sub(1),
                    semantic_requirements_met_after_normalisation: None,
                    first_required_item_missing: None,
                };
                let diagnosis = classify(&observation);
                let prescription =
                    prescribe(&diagnosis, &policy, attempts, outcome.total_tokens, false);
                refusals_classified += 1;
                println!(
                    "[rrp] {family}#{index}: {} → {}",
                    diagnosis.primary.key(),
                    prescription
                        .actions
                        .iter()
                        .map(|a| a.key())
                        .collect::<Vec<_>>()
                        .join("+")
                );
                if prescription.escalated {
                    escalations += 1;
                    operator_interventions += 1;
                    break;
                }
                for action in &prescription.actions {
                    match action {
                        Action::ExpandEnvelope => {
                            envelope_local = prescription
                                .suggested_envelope
                                .unwrap_or(envelope_local * 4)
                                .max(envelope_local);
                        }
                        Action::NormaliseBrief | Action::ImposeAnchors => {
                            feedback = format!(
                                "The previous attempt was refuted by the engine:\n- {}\nFix the authoring conditions and try again. The engine's verdicts are the standard.",
                                all_failures.join("\n- ")
                            );
                        }
                        Action::DecomposeWork => {
                            feedback = format!(
                                "The previous attempt was refuted. Author FEWER, SIMPLER fixtures (one at a time), each fully within the schema:\n- {}",
                                all_failures.join("\n- ")
                            );
                        }
                        Action::ChangeProvider
                        | Action::SubstitutePlayer
                        | Action::AssignOwnership
                        | Action::BoundedRetry
                        | Action::Escalate => {}
                    }
                }
                repairs_applied += 1;
            }
            if let Some(accepted) = accepted_fixtures {
                authored.extend(accepted);
            } else {
                operator_interventions += 1;
            }
        }
    }

    // Persist the engine-verified corpus.
    std::fs::create_dir_all(&out_dir).ok();
    let path = format!("{out_dir}/fixtures.json");
    let corpus = serde_json::to_string_pretty(&authored).expect("corpus serialises");
    std::fs::write(&path, &corpus).expect("write corpus");
    println!();
    println!(
        "▌ DEV FOREMAN — {} engine-verified fixtures → {path}",
        authored.len()
    );
    println!(
        "[rrp] refusals classified: {refusals_classified} · repairs applied: {repairs_applied} · escalations: {escalations} · operator interventions: {operator_interventions}"
    );
}

/// The per-family authoring brief: the schema, the rules, and the
/// requirement that predictions must match the engine's real behaviour.
fn family_brief(family: &str, count: u32) -> String {
    let common = "You are authoring TEST FIXTURES for a deterministic engine. Each fixture PREDICTS what the engine will output; the engine itself is the judge — a wrong prediction is a rejected fixture.\nOutput ONLY a JSON array of fixture objects, no markdown fences.";
    match family {
        "envelope" => format!(
            "{common}\nSchema per fixture: {{\"fixture_id\": \"<slug>\", \"family\": \"envelope\", \"observation\": {{\"finish_reason\": <\"length\"|\"stop\"|null>, \"provider_status\": <string|null>, \"exit_code\": <int|null>, \"gate_failures\": [<string>], \"output_structurally_complete\": <bool>, \"artefact_expected_but_missing\": false, \"output_path_contended\": false, \"version_control_failed\": false, \"prior_attempts_with_same_failures\": <int>, \"semantic_requirements_met_after_normalisation\": <bool|null>, \"first_required_item_missing\": <bool|null>}}, \"expected_cause\": <one of provider_failure|rate_limited|output_truncation|brief_rigidity|work_unit_too_large|artefact_incomplete|anchor_violation|unknown>, \"expected_confidence\": <\"certain\"|\"high\"|\"moderate\"|\"unknown\">}}\nEngine rules you must predict correctly: finish_reason=length + incomplete output → output_truncation/certain. provider status text + incomplete output → provider_failure (or rate_limited if 429)/certain. Content returned but gate failures listed + semantic_requirements_met_after_normalisation=true + structurally complete → brief_rigidity/certain. gate failures + prior_attempts>0 → work_unit_too_large/high. gate failures alone → artefact_incomplete/moderate. no signals → unknown/unknown (do NOT author unknown fixtures).\nAuthor {count} DIVERSE fixtures whose predictions you are confident about."
        ),
        "decomposition" => format!(
            "{common}\nSchema per fixture: {{\"fixture_id\": \"<slug>\", \"family\": \"decomposition\", \"items\": [<string>, ...], \"chunk_size\": <int>, \"expected_chunks\": <int>, \"expected_chunk_anchors\": [{{\"expected_items\": <int>, \"first\": <first item string>, \"last\": <last item string>}}, ... per chunk]}}\nEngine rules: items are split into consecutive chunks of exactly chunk_size (last chunk may be smaller); each contract carries expected_items = chunk length, first = the chunk's first item string, last = the chunk's last item string.\nAuthor {count} fixtures with varied item counts (including exact multiples and remainders) and chunk sizes 2-8. Predict the chunking precisely."
        ),
        "ownership" => format!(
            "{common}\nSchema per fixture: {{\"fixture_id\": \"<slug>\", \"family\": \"ownership\", \"owner\": \"<name>\", \"writer\": \"<name>\", \"exists\": <bool>, \"overwrite_policy\": <\"replace_owned\"|\"forbidden\"|\"append\"|\"merge\"|\"derived_only\"|\"explicit_transfer\">, \"permitted_mutators\": [<string>...], \"transfer_recorded\": <bool>, \"expected_verdict\": <\"allowed\"|\"forbidden_by_policy\"|\"not_a_mutator\"|\"transfer_required\">}}\nEngine rules: not exists → allowed. forbidden → forbidden_by_policy. replace_owned/append: owner or a permitted mutator → allowed, else not_a_mutator. merge → always allowed. derived_only → forbidden_by_policy. explicit_transfer → allowed only if transfer_recorded.\nAuthor {count} fixtures covering every policy and every denial path."
        ),
        "lineage" => format!(
            "{common}\nSchema per fixture: {{\"fixture_id\": \"<slug>\", \"family\": \"lineage\", \"chain_length\": <int>}}\nEngine rule: a refusal chain of N hops produces depth N+1 (the root plus one node per hop).\nAuthor {count} fixtures with chain lengths 1-8."
        ),
        _ => String::new(),
    }
}
