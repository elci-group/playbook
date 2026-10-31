//! Playbook command line. `run` is the testable entry point.
//!
//! Every command renders in one of three output styles, selected by a flag
//! read before the subcommand runs: the default human-readable prose, `--form3`
//! (one 3form event line per fact, in the [`form3`] grammar, ready for the
//! HUD), or `--basic` (the plainest text a pipe can read). The styles are
//! mutually exclusive; asking for both is a usage error. The banner help is
//! never restyled. `sprint`, `policy` and `otlp-bench` are machine contracts
//! already, so `--basic` leaves them exactly as they are.

#![forbid(unsafe_code)]

use playbook_core::PlaybookError;
use playbook_observability::{audit_text, stage_line};
use playbook_policy::Policy;
use playbook_sim::{
    compare_policies_with, fold, phase_eight_study, run_mvp, study_table, ApplyMode, DriveEvent,
    HuddleRegime, MvpReport, PolicyComparison,
};

mod form3;

/// The output style a command renders in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputStyle {
    /// The default human-readable output.
    Plain,
    /// 3form HUD event lines on stdout.
    Form3,
    /// The plainest text: bare values and key=value lines, no decorations.
    Basic,
}

pub fn run(args: &[impl AsRef<str>]) -> i32 {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    let (code, output) = execute(&args);
    print!("{output}");
    code
}

/// Run one command line and return the exit code plus everything it writes to
/// stdout. Diagnostics — usage, style conflicts, command errors — go to
/// stderr as before. Tests read stdout through this function.
fn execute(args: &[&str]) -> (i32, String) {
    let (style, args) = match select_style(args) {
        Ok(selected) => selected,
        Err(message) => {
            eprintln!("{message}");
            return (2, String::new());
        }
    };
    match args.first().copied() {
        Some("--help") | Some("-h") | Some("help") => {
            print_usage();
            (0, String::new())
        }
        Some("mvp") => rendered(print_mvp(style, &args[1..])),
        Some("sim") => rendered(print_sim(style, &args[1..])),
        Some("sprint") => rendered(print_sprint()),
        Some("priors") => rendered(print_priors(style)),
        Some("explain") => rendered(print_explain(style)),
        Some("status") => rendered(print_status(style)),
        Some("policy") => rendered(print_policy(style, args.get(1).copied())),
        Some("recover") => rendered(print_recover(style, args.get(1).copied())),
        Some("otlp") => rendered(print_otlp(style, &args[1..])),
        Some("otlp-bench") => rendered(print_otlp_bench(style, &args[1..])),
        Some("huddle-study") => rendered(print_huddle_study(style)),
        Some("cards") => rendered(print_cards(style, &args[1..])),
        _ => {
            print_usage();
            (2, String::new())
        }
    }
}

/// Turn a rendered command into an exit code plus its stdout text.
fn rendered(result: Result<String, PlaybookError>) -> (i32, String) {
    match result {
        Ok(output) => (0, output),
        Err(err) => {
            eprintln!("{err}");
            (1, String::new())
        }
    }
}

/// Pull the style flags out of the argument list, wherever they appear, and
/// return the selected style plus the remaining arguments. Naming both
/// `--form3` and `--basic` is a usage error.
fn select_style<'a>(args: &[&'a str]) -> Result<(OutputStyle, Vec<&'a str>), &'static str> {
    let mut form3 = false;
    let mut basic = false;
    let mut rest = Vec::with_capacity(args.len());
    for arg in args {
        match *arg {
            "--form3" => form3 = true,
            "--basic" => basic = true,
            other => rest.push(other),
        }
    }
    if form3 && basic {
        return Err("--form3 and --basic are mutually exclusive");
    }
    let style = if form3 {
        OutputStyle::Form3
    } else if basic {
        OutputStyle::Basic
    } else {
        OutputStyle::Plain
    };
    Ok((style, rest))
}

/// Append one output line to the buffer.
fn line(out: &mut String, text: impl std::fmt::Display) {
    out.push_str(&text.to_string());
    out.push('\n');
}

fn print_usage() {
    eprintln!("usage: playbook <mvp|sim|sprint|priors|explain|status|policy|recover|otlp|otlp-bench|huddle-study|cards>");
    eprintln!("  cards [--out <dir>] [--threeform] renders the animated X-and-O playbook gallery; with --threeform, prints one 3form event line per card");
    eprintln!(
        "  mvp [--journal <path>] runs the acceptance drive, optionally through a durable journal"
    );
    eprintln!("  recover <journal> folds a recovered drive and reports what the host owes");
    eprintln!("  otlp <endpoint> exports the acceptance drive trace over OTLP/HTTP JSON");
    eprintln!(
        "  otlp-bench --spans N [--endpoint url] [--runs R] measures encode and export throughput"
    );
    eprintln!("  huddle-study runs the Phase 8 comparative huddle programme and reports completion per regime");
    eprintln!("  --form3 | --basic restyles any command: 3form HUD event lines, or the plainest text; the two are mutually exclusive");
}

/// The acceptance drive. Plain mode reads as the coach's summary; form3 mode
/// emits one `event=mvp_stage` per stage then `event=mvp_complete`; basic
/// mode is the stage names then `replay_match true|false` — nothing else.
fn print_mvp(style: OutputStyle, args: &[&str]) -> Result<String, PlaybookError> {
    let mut journal: Option<&str> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--journal" => {
                journal = Some(args.get(index + 1).copied().ok_or_else(|| {
                    PlaybookError::Evidence("--journal requires a file path".into())
                })?);
                index += 2;
            }
            other => {
                return Err(PlaybookError::Evidence(format!(
                    "unknown mvp argument {other}"
                )));
            }
        }
    }
    let report = match journal {
        Some(path) => playbook_sim::run_mvp_with_journal(std::path::Path::new(path))?,
        None => playbook_sim::run_mvp()?,
    };
    let mut out = String::new();
    match style {
        OutputStyle::Plain => {
            for stage in &report.stages {
                line(&mut out, stage);
            }
            line(&mut out, format!("replay_match {}", report.replay_match));
            if let Some(matched) = report.journal_replay_match {
                line(&mut out, format!("journal_replay_match {matched}"));
            }
            line(
                &mut out,
                format!(
                    "disposition {}",
                    report
                        .disposition
                        .map(|item| item.machine())
                        .unwrap_or("none")
                ),
            );
            line(
                &mut out,
                format!("false_completion_claims {}", report.false_completion_claims),
            );
            line(
                &mut out,
                format!("duplicate_failed_plays {}", report.duplicate_failed_plays),
            );
            line(
                &mut out,
                format!(
                    "control_duplicate_failed_plays {}",
                    report.control_duplicate_failed_plays
                ),
            );
        }
        OutputStyle::Basic => {
            for stage in &report.stages {
                line(&mut out, stage);
            }
            line(&mut out, format!("replay_match {}", report.replay_match));
        }
        OutputStyle::Form3 => {
            for (index, stage) in report.stages.iter().enumerate() {
                line(
                    &mut out,
                    form3::event(
                        "mvp_stage",
                        &[("stage", stage.clone()), ("index", index.to_string())],
                    ),
                );
            }
            let mut pairs = vec![
                ("replay_match", form3::flag(report.replay_match).to_string()),
                (
                    "disposition",
                    report
                        .disposition
                        .map(|item| item.machine())
                        .unwrap_or("none")
                        .to_string(),
                ),
                (
                    "false_completion_claims",
                    report.false_completion_claims.to_string(),
                ),
                (
                    "duplicate_failed_plays",
                    report.duplicate_failed_plays.to_string(),
                ),
            ];
            if let Some(matched) = report.journal_replay_match {
                pairs.push(("journal_replay_match", form3::flag(matched).to_string()));
            }
            line(&mut out, form3::event("mvp_complete", &pairs));
        }
    }
    Ok(out)
}

/// The policy comparison. Plain mode keeps the coach's column; form3 mode
/// emits one `event=sim_comparison` carrying every comparison field; basic
/// mode prints the same fields as strict `key=value` lines (trajectories
/// comma-joined, absent indices as `none`).
fn print_sim(style: OutputStyle, args: &[&str]) -> Result<String, PlaybookError> {
    let mut policy_name = "playbook-policy-pass-heavy";
    let mut trace_name = "mvp";
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--policy" => {
                policy_name = args.get(index + 1).copied().ok_or_else(|| {
                    PlaybookError::Evidence("--policy requires a policy name".into())
                })?;
                index += 2;
            }
            "--trace" => {
                trace_name = args.get(index + 1).copied().ok_or_else(|| {
                    PlaybookError::Evidence("--trace requires mvp or an event-log path".into())
                })?;
                index += 2;
            }
            other => {
                return Err(PlaybookError::Evidence(format!(
                    "unknown sim argument {other}"
                )));
            }
        }
    }
    let policy = resolve_policy(policy_name)?;
    let events = load_trace(trace_name)?;
    let comparison = compare_policies_with(&events, &policy)?;
    let mut out = String::new();
    match style {
        OutputStyle::Plain => {
            line(&mut out, format!("trace {trace_name}"));
            line(&mut out, format!("policy {}", policy.version));
            line(&mut out, format!("ytg_match {}", comparison.ytg_match));
            line(
                &mut out,
                format!("budget_match {}", comparison.budget_match),
            );
            line(
                &mut out,
                format!("completion_match {}", comparison.completion_match),
            );
            line(
                &mut out,
                format!("down_labels_differ {}", comparison.down_labels_differ),
            );
            line(
                &mut out,
                format!("completion_status {}", comparison.completion_status),
            );
            line(
                &mut out,
                format!("ytg_trajectory {:?}", comparison.ytg_trajectory),
            );
            line(
                &mut out,
                format!("budget_remaining_ppm {}", comparison.budget_remaining_ppm),
            );
            line(
                &mut out,
                format!("tool_failures {}", comparison.tool_failures),
            );
            line(&mut out, format!("model_calls {}", comparison.model_calls));
            line(
                &mut out,
                format!("personnel_changes {}", comparison.personnel_changes),
            );
            line(
                &mut out,
                format!("fourth_down_index {:?}", comparison.fourth_down_index),
            );
            line(
                &mut out,
                format!("wall_clock_ms {}", comparison.wall_clock_ms),
            );
            line(
                &mut out,
                format!("monetary_cost_micros {}", comparison.monetary_cost_micros),
            );
            line(
                &mut out,
                format!("gates_satisfied {}", comparison.gates_satisfied),
            );
            line(
                &mut out,
                format!("yards_gained_ppm {}", comparison.yards_gained_ppm),
            );
            line(
                &mut out,
                format!("budget_spent_ppm {}", comparison.budget_spent_ppm),
            );
            line(
                &mut out,
                format!(
                    "efficiency_ratio_milli {}",
                    comparison.efficiency_ratio_milli
                ),
            );
        }
        OutputStyle::Basic => {
            for (key, value) in sim_pairs(trace_name, &policy, &comparison) {
                line(&mut out, format!("{key}={value}"));
            }
        }
        OutputStyle::Form3 => {
            line(
                &mut out,
                form3::event(
                    "sim_comparison",
                    &sim_pairs(trace_name, &policy, &comparison),
                ),
            );
        }
    }
    Ok(out)
}

/// The comparison as ordered key/value pairs for the machine styles.
fn sim_pairs(
    trace_name: &str,
    policy: &Policy,
    comparison: &PolicyComparison,
) -> Vec<(&'static str, String)> {
    let trajectory = if comparison.ytg_trajectory.is_empty() {
        "none".to_string()
    } else {
        comparison
            .ytg_trajectory
            .iter()
            .map(|ytg| ytg.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    vec![
        ("trace", trace_name.to_string()),
        ("policy", policy.version.clone()),
        ("ytg_match", form3::flag(comparison.ytg_match).to_string()),
        (
            "budget_match",
            form3::flag(comparison.budget_match).to_string(),
        ),
        (
            "completion_match",
            form3::flag(comparison.completion_match).to_string(),
        ),
        (
            "down_labels_differ",
            form3::flag(comparison.down_labels_differ).to_string(),
        ),
        ("completion_status", comparison.completion_status.clone()),
        ("ytg_trajectory", trajectory),
        (
            "budget_remaining_ppm",
            comparison.budget_remaining_ppm.to_string(),
        ),
        ("tool_failures", comparison.tool_failures.to_string()),
        ("model_calls", comparison.model_calls.to_string()),
        (
            "personnel_changes",
            comparison.personnel_changes.to_string(),
        ),
        (
            "fourth_down_index",
            comparison
                .fourth_down_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "none".to_string()),
        ),
        ("wall_clock_ms", comparison.wall_clock_ms.to_string()),
        (
            "monetary_cost_micros",
            comparison.monetary_cost_micros.to_string(),
        ),
        ("gates_satisfied", comparison.gates_satisfied.to_string()),
        ("yards_gained_ppm", comparison.yards_gained_ppm.to_string()),
        ("budget_spent_ppm", comparison.budget_spent_ppm.to_string()),
        (
            "efficiency_ratio_milli",
            comparison.efficiency_ratio_milli.to_string(),
        ),
    ]
}

fn resolve_policy(name: &str) -> Result<Policy, PlaybookError> {
    match name {
        "builtin" | "playbook-policy-1" => Ok(Policy::builtin()),
        "pass-heavy" | "playbook-policy-pass-heavy" => Ok(Policy::pass_heavy_contrast()),
        _ => Err(PlaybookError::Evidence(format!("unknown policy {name}"))),
    }
}

fn load_trace(name: &str) -> Result<Vec<DriveEvent>, PlaybookError> {
    if name == "mvp" {
        return Ok(accepted_report()?.events);
    }
    let text = std::fs::read_to_string(name).map_err(|error| {
        PlaybookError::Evidence(format!("could not read trace {name}: {error}"))
    })?;
    serde_json::from_str(&text).map_err(PlaybookError::json)
}

/// The event transcript. A machine contract: identical in every style.
fn print_sprint() -> Result<String, PlaybookError> {
    let report = accepted_report()?;
    let mut out = String::new();
    line(
        &mut out,
        serde_json::to_string_pretty(&report.events).map_err(PlaybookError::json)?,
    );
    Ok(out)
}

/// The learned prior table. Plain mode keeps the summary plus the JSON table;
/// form3 mode emits `event=prior_row` per row then `event=priors_total`;
/// basic mode is the same rows as `key=value` lines without the JSON.
fn print_priors(style: OutputStyle) -> Result<String, PlaybookError> {
    let report = accepted_report()?;
    let priors = report.final_state.priors();
    let mut out = String::new();
    match style {
        OutputStyle::Plain => {
            line(&mut out, format!("samples {}", priors.samples()));
            for (personnel, row) in priors.rows() {
                line(
                    &mut out,
                    format!(
                        "{personnel} samples={} realized_reduction_sum_milli={}",
                        row.samples, row.realized_reduction_sum_milli
                    ),
                );
            }
            line(
                &mut out,
                serde_json::to_string_pretty(priors).map_err(PlaybookError::json)?,
            );
        }
        OutputStyle::Basic => {
            line(&mut out, format!("samples={}", priors.samples()));
            for (personnel, row) in priors.rows() {
                line(
                    &mut out,
                    format!(
                        "personnel={personnel} samples={} realized_reduction_sum_milli={}",
                        row.samples, row.realized_reduction_sum_milli
                    ),
                );
            }
        }
        OutputStyle::Form3 => {
            for (personnel, row) in priors.rows() {
                line(
                    &mut out,
                    form3::event(
                        "prior_row",
                        &[
                            ("personnel", personnel.clone()),
                            ("samples", row.samples.to_string()),
                            (
                                "realized_reduction_sum_milli",
                                row.realized_reduction_sum_milli.to_string(),
                            ),
                        ],
                    ),
                );
            }
            line(
                &mut out,
                form3::event("priors_total", &[("samples", priors.samples().to_string())]),
            );
        }
    }
    Ok(out)
}

/// The audit explanation. Plain mode keeps the coach's paragraph; form3 mode
/// emits `event=audit_explanation` then one `event=audit_reason` per reason;
/// basic mode is the bare down and the bare reasons, no labels.
fn print_explain(style: OutputStyle) -> Result<String, PlaybookError> {
    let report = accepted_report()?;
    let snapshot = report.final_state.snapshot(&Policy::builtin());
    let mut out = String::new();
    match style {
        OutputStyle::Plain => line(&mut out, audit_text(&snapshot)),
        OutputStyle::Basic => {
            line(&mut out, snapshot.current_down.machine_id());
            for reason in &snapshot.explanation {
                line(&mut out, reason);
            }
        }
        OutputStyle::Form3 => {
            line(
                &mut out,
                form3::event(
                    "audit_explanation",
                    &[
                        ("down", snapshot.current_down.machine_id().to_string()),
                        ("reason_count", snapshot.explanation.len().to_string()),
                    ],
                ),
            );
            for reason in &snapshot.explanation {
                line(
                    &mut out,
                    form3::event("audit_reason", &[("text", reason.clone())]),
                );
            }
        }
    }
    Ok(out)
}

/// The tactical snapshot. Plain mode keeps the reading plus the JSON; form3
/// mode emits one `event=tactical_status`; basic mode is the bare down,
/// yards-to-go, budget and play — one value per line, no labels.
fn print_status(style: OutputStyle) -> Result<String, PlaybookError> {
    let report = accepted_report()?;
    let snapshot = report.final_state.snapshot(&Policy::builtin());
    let mut out = String::new();
    match style {
        OutputStyle::Plain => {
            line(&mut out, stage_line(&snapshot));
            line(
                &mut out,
                serde_json::to_string(&snapshot).map_err(PlaybookError::json)?,
            );
        }
        OutputStyle::Basic => {
            line(&mut out, snapshot.current_down.machine_id());
            line(&mut out, snapshot.ytg_milli);
            line(&mut out, snapshot.budget_remaining_ppm);
            line(&mut out, snapshot.current_play.as_deref().unwrap_or("none"));
        }
        OutputStyle::Form3 => {
            line(
                &mut out,
                form3::event(
                    "tactical_status",
                    &[
                        ("drive_id", snapshot.drive_id.clone()),
                        ("down", snapshot.current_down.machine_id().to_string()),
                        ("ytg_milli", snapshot.ytg_milli.to_string()),
                        (
                            "budget_remaining_ppm",
                            snapshot.budget_remaining_ppm.to_string(),
                        ),
                        (
                            "play",
                            snapshot
                                .current_play
                                .clone()
                                .unwrap_or_else(|| "none".to_string()),
                        ),
                    ],
                ),
            );
        }
    }
    Ok(out)
}

/// One policy variant. Plain and basic mode print the JSON document; form3
/// mode emits one `event=policy <field>=<value>` per top-level field, using
/// the document's own key names.
fn print_policy(style: OutputStyle, variant: Option<&str>) -> Result<String, PlaybookError> {
    let policy = match variant {
        Some("pass-heavy") => Policy::pass_heavy_contrast(),
        _ => Policy::builtin(),
    };
    let mut out = String::new();
    match style {
        OutputStyle::Form3 => {
            let document = serde_json::to_value(&policy).map_err(PlaybookError::json)?;
            if let serde_json::Value::Object(fields) = document {
                for (key, field) in fields {
                    line(
                        &mut out,
                        form3::event("policy", &[(key.as_str(), policy_field_text(&field))]),
                    );
                }
            }
        }
        OutputStyle::Plain | OutputStyle::Basic => {
            line(
                &mut out,
                serde_json::to_string_pretty(&policy).map_err(PlaybookError::json)?,
            );
        }
    }
    Ok(out)
}

/// One top-level policy field as HUD-safe text: strings bare, everything
/// else as compact JSON.
fn policy_field_text(field: &serde_json::Value) -> String {
    match field {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// A recovered drive. Plain mode keeps the coach's column; form3 mode emits
/// `event=recovered_drive` plus `event=open_execution` when an execution is
/// open; basic mode is the same facts as strict `key=value` lines.
fn print_recover(style: OutputStyle, path: Option<&str>) -> Result<String, PlaybookError> {
    let Some(path) = path else {
        return Err(PlaybookError::Evidence(
            "recover requires a journal path".into(),
        ));
    };
    let recovery = playbook_journal::Journal::open(std::path::Path::new(path))?;
    if recovery.events.is_empty() {
        return Err(PlaybookError::Evidence("journal holds no drive".into()));
    }
    let policy = Policy::builtin();
    let state = fold(&recovery.events, &policy, ApplyMode::Audit)?;
    let mut out = String::new();
    match style {
        OutputStyle::Plain => {
            if recovery.repaired_tail {
                line(&mut out, "repaired_tail true");
            }
            line(&mut out, format!("drive_id {}", state.drive_id().as_str()));
            line(&mut out, format!("status {:?}", state.status()));
            line(&mut out, format!("down {}", state.down().machine_id()));
            line(&mut out, format!("ytg_milli {}", state.ytg_milli()));
            line(
                &mut out,
                format!(
                    "budget_remaining_ppm {}",
                    state.budget_remaining_ppm(&policy)
                ),
            );
            line(&mut out, format!("events {}", recovery.events.len()));
            match state.open_execution() {
                Some(execution) => {
                    line(
                        &mut out,
                        format!("open_execution {}", execution.play_id.as_str()),
                    );
                    line(
                        &mut out,
                        format!(
                            "reconcile_required {}",
                            execution.idempotency_key.as_deref().unwrap_or("none")
                        ),
                    );
                }
                None => line(&mut out, "open_execution none"),
            }
            line(
                &mut out,
                format!("unresolved_effects {}", state.unresolved_effects().len()),
            );
        }
        OutputStyle::Basic => {
            if recovery.repaired_tail {
                line(&mut out, "repaired_tail=true");
            }
            line(&mut out, format!("drive_id={}", state.drive_id().as_str()));
            line(&mut out, format!("status={:?}", state.status()));
            line(&mut out, format!("down={}", state.down().machine_id()));
            line(&mut out, format!("ytg_milli={}", state.ytg_milli()));
            line(
                &mut out,
                format!(
                    "budget_remaining_ppm={}",
                    state.budget_remaining_ppm(&policy)
                ),
            );
            line(&mut out, format!("events={}", recovery.events.len()));
            match state.open_execution() {
                Some(execution) => {
                    line(
                        &mut out,
                        format!("open_execution={}", execution.play_id.as_str()),
                    );
                    line(
                        &mut out,
                        format!(
                            "reconcile_required={}",
                            execution.idempotency_key.as_deref().unwrap_or("none")
                        ),
                    );
                }
                None => line(&mut out, "open_execution=none"),
            }
            line(
                &mut out,
                format!("unresolved_effects={}", state.unresolved_effects().len()),
            );
        }
        OutputStyle::Form3 => {
            let mut pairs = vec![
                ("drive_id", state.drive_id().as_str().to_string()),
                ("status", format!("{:?}", state.status()).to_lowercase()),
                ("down", state.down().machine_id().to_string()),
                ("ytg_milli", state.ytg_milli().to_string()),
                (
                    "budget_remaining_ppm",
                    state.budget_remaining_ppm(&policy).to_string(),
                ),
                ("events", recovery.events.len().to_string()),
            ];
            if recovery.repaired_tail {
                pairs.push(("repaired_tail", "true".to_string()));
            }
            pairs.push((
                "unresolved_effects",
                state.unresolved_effects().len().to_string(),
            ));
            line(&mut out, form3::event("recovered_drive", &pairs));
            if let Some(execution) = state.open_execution() {
                line(
                    &mut out,
                    form3::event(
                        "open_execution",
                        &[
                            ("play_id", execution.play_id.as_str().to_string()),
                            (
                                "key",
                                execution
                                    .idempotency_key
                                    .clone()
                                    .unwrap_or_else(|| "none".to_string()),
                            ),
                        ],
                    ),
                );
            }
        }
    }
    Ok(out)
}

/// The trace export. Plain and basic mode keep the summary column (basic as
/// strict `key=value`); form3 mode emits one `event=otlp <field>=<value>`
/// per summary field, using the existing key names.
fn print_otlp(style: OutputStyle, args: &[&str]) -> Result<String, PlaybookError> {
    if args.len() != 1 {
        return Err(PlaybookError::Evidence(
            "otlp requires an http endpoint".into(),
        ));
    }
    let endpoint = args[0];
    let report = accepted_report()?;
    let drive_id = report.final_state.drive_id().to_string();
    let spans = playbook_observability::spans(&drive_id, &report.events);
    let start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| PlaybookError::Io(error.to_string()))?
        .as_nanos() as u64;
    let end = start.saturating_add(report.events.len() as u64 * 1_000_000);
    let summary = playbook_otlp::export(
        &playbook_otlp::OtlpConfig {
            endpoint: endpoint.into(),
            service_name: "playbook".into(),
        },
        &spans,
        playbook_otlp::TimeWindow {
            start_unix_nano: start,
            end_unix_nano: end,
        },
    )?;
    let mut fields: Vec<(&'static str, String)> = vec![
        ("endpoint", endpoint.to_string()),
        ("spans", summary.spans.to_string()),
        ("bytes_sent", summary.bytes_sent.to_string()),
        ("status", summary.status.to_string()),
        ("retries", summary.retries.to_string()),
    ];
    if let Some(partial_error) = &summary.partial_error {
        fields.push(("partial_error", partial_error.clone()));
    }
    let mut out = String::new();
    match style {
        OutputStyle::Plain => {
            for (key, value) in &fields {
                line(&mut out, format!("{key} {value}"));
            }
        }
        OutputStyle::Basic => {
            for (key, value) in &fields {
                line(&mut out, format!("{key}={value}"));
            }
        }
        OutputStyle::Form3 => {
            for (key, value) in fields {
                line(&mut out, form3::event("otlp", &[(key, value)]));
            }
        }
    }
    Ok(out)
}

/// The export benchmark. Plain mode keeps the coach's column; form3 mode
/// emits one `event=otlp_bench <field>=<value>` per measured field; basic
/// mode leaves the benchmark column exactly as plain prints it.
fn print_otlp_bench(style: OutputStyle, args: &[&str]) -> Result<String, PlaybookError> {
    let mut span_count = 1_000usize;
    let mut runs = 50usize;
    let mut endpoint: Option<&str> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--spans" => {
                let value = args
                    .get(index + 1)
                    .copied()
                    .ok_or_else(|| PlaybookError::Evidence("--spans requires a count".into()))?;
                span_count = value.parse().map_err(|_| {
                    PlaybookError::Evidence(format!("invalid --spans count {value}"))
                })?;
                index += 2;
            }
            "--runs" => {
                let value = args
                    .get(index + 1)
                    .copied()
                    .ok_or_else(|| PlaybookError::Evidence("--runs requires a count".into()))?;
                runs = value.parse().map_err(|_| {
                    PlaybookError::Evidence(format!("invalid --runs count {value}"))
                })?;
                index += 2;
            }
            "--endpoint" => {
                endpoint =
                    Some(args.get(index + 1).copied().ok_or_else(|| {
                        PlaybookError::Evidence("--endpoint requires a url".into())
                    })?);
                index += 2;
            }
            other => {
                return Err(PlaybookError::Evidence(format!(
                    "unknown otlp-bench argument {other}"
                )));
            }
        }
    }
    let report = accepted_report()?;
    let drive_id = report.final_state.drive_id().to_string();
    let base = playbook_observability::spans(&drive_id, &report.events);
    let mut batch = Vec::with_capacity(span_count);
    for i in 0..span_count {
        let mut span = base[i % base.len()].clone();
        span.span_id = format!("{}:k{i}", span.span_id);
        if let Some(parent) = &span.parent_span_id {
            span.parent_span_id = Some(format!("{parent}:k{i}"));
        }
        batch.push(span);
    }
    let start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| PlaybookError::Io(error.to_string()))?
        .as_nanos() as u64;
    let window = || playbook_otlp::TimeWindow {
        start_unix_nano: start,
        end_unix_nano: start.saturating_add(span_count as u64 * 1_000_000),
    };
    let mut encode_nanos = 0u128;
    let mut bytes = 0usize;
    for _ in 0..runs {
        let began = std::time::Instant::now();
        let encoded = playbook_otlp::encode_request("playbook", &batch, window())?;
        encode_nanos += began.elapsed().as_nanos();
        bytes = encoded.len();
    }
    let bytes_per_span = bytes.checked_div(span_count).unwrap_or(0);
    let encode_per_span_nanos = (runs as u128)
        .checked_mul(span_count as u128)
        .and_then(|total| encode_nanos.checked_div(total))
        .unwrap_or(0);
    let mut fields: Vec<(&'static str, String)> = vec![
        ("spans", span_count.to_string()),
        ("runs", runs.to_string()),
        ("bytes_per_span", bytes_per_span.to_string()),
        ("encode_total_micros", (encode_nanos / 1_000).to_string()),
        ("encode_per_span_nanos", encode_per_span_nanos.to_string()),
    ];
    if let Some(endpoint) = endpoint {
        let config = playbook_otlp::OtlpConfig {
            endpoint: endpoint.into(),
            service_name: "playbook".into(),
        };
        let mut export_nanos = 0u128;
        let mut exports_ok = 0usize;
        let mut export_errors = 0usize;
        for _ in 0..runs {
            let began = std::time::Instant::now();
            let result = playbook_otlp::export(&config, &batch, window());
            export_nanos += began.elapsed().as_nanos();
            match result {
                Ok(summary) => {
                    if summary.status == 200 || summary.status == 202 {
                        exports_ok += 1;
                    }
                }
                Err(_) => export_errors += 1,
            }
        }
        let export_per_call_micros = export_nanos.checked_div(runs as u128).unwrap_or(0) / 1_000;
        fields.push(("export_total_micros", (export_nanos / 1_000).to_string()));
        fields.push(("export_per_call_micros", export_per_call_micros.to_string()));
        fields.push(("exports_ok", exports_ok.to_string()));
        fields.push(("export_errors", export_errors.to_string()));
    }
    let mut out = String::new();
    match style {
        OutputStyle::Plain | OutputStyle::Basic => {
            for (key, value) in &fields {
                line(&mut out, format!("{key} {value}"));
            }
        }
        OutputStyle::Form3 => {
            for (key, value) in fields {
                line(&mut out, form3::event("otlp_bench", &[(key, value)]));
            }
        }
    }
    Ok(out)
}

/// The X-and-O gallery. Plain mode keeps today's behaviour — event lines,
/// then the HTML page or a written file; form3 mode is exactly the card
/// event lines; basic mode is the tactic names, one per line, nothing else.
fn print_cards(style: OutputStyle, args: &[&str]) -> Result<String, PlaybookError> {
    let mut out: Option<&str> = None;
    let mut threeform = false;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--out" => {
                out =
                    Some(args.get(index + 1).copied().ok_or_else(|| {
                        PlaybookError::Evidence("--out requires a directory".into())
                    })?);
                index += 2;
            }
            "--threeform" => {
                threeform = true;
                index += 1;
            }
            other => {
                return Err(PlaybookError::Evidence(format!(
                    "unknown cards argument {other}"
                )));
            }
        }
    }
    let mut page = String::new();
    match style {
        OutputStyle::Form3 => {
            for card_line in playbook_cards::threeform_lines() {
                line(&mut page, card_line);
            }
            return Ok(page);
        }
        OutputStyle::Basic => {
            for spec in playbook_cards::all_cards() {
                line(&mut page, spec.name);
            }
            return Ok(page);
        }
        OutputStyle::Plain => {
            for card_line in playbook_cards::threeform_lines() {
                line(&mut page, card_line);
            }
        }
    }
    if threeform {
        return Ok(page);
    }
    let gallery = playbook_cards::render_gallery();
    match out {
        Some(dir) => {
            std::fs::create_dir_all(dir)
                .map_err(|error| PlaybookError::Io(format!("cannot create {dir}: {error}")))?;
            let path = std::path::Path::new(dir).join("playbook.html");
            std::fs::write(&path, &gallery).map_err(|error| {
                PlaybookError::Io(format!("cannot write {}: {error}", path.display()))
            })?;
            line(&mut page, format!("wrote {}", path.display()));
            line(
                &mut page,
                format!("cards {}", playbook_cards::all_cards().len()),
            );
        }
        None => page.push_str(&gallery),
    }
    Ok(page)
}

/// The Phase 8 huddle programme. Plain mode keeps the study table; form3 mode
/// emits one `event=huddle_scenario` per scenario-regime cell; basic mode
/// drops the table and keeps one `scenario=<name> regime=yes|no ...` line
/// per scenario.
fn print_huddle_study(style: OutputStyle) -> Result<String, PlaybookError> {
    let regimes = [
        HuddleRegime::Control,
        HuddleRegime::Periodic,
        HuddleRegime::FlatTriggered,
        HuddleRegime::HierarchicalTriggered,
    ];
    let scenarios = phase_eight_study();
    let mut out = String::new();
    match style {
        OutputStyle::Plain => line(&mut out, study_table(&scenarios)),
        OutputStyle::Basic | OutputStyle::Form3 => {}
    }
    for scenario in &scenarios {
        let completions: Vec<String> = regimes
            .iter()
            .map(|regime| {
                format!(
                    "{}={}",
                    regime.name(),
                    if scenario.run(*regime).completed {
                        "yes"
                    } else {
                        "no"
                    }
                )
            })
            .collect();
        match style {
            OutputStyle::Plain => line(
                &mut out,
                format!("{}: {}", scenario.name, completions.join(" ")),
            ),
            OutputStyle::Basic => {
                line(
                    &mut out,
                    format!("scenario={} {}", scenario.name, completions.join(" ")),
                );
            }
            OutputStyle::Form3 => {
                for regime in &regimes {
                    line(
                        &mut out,
                        form3::event(
                            "huddle_scenario",
                            &[
                                ("scenario", scenario.name.to_string()),
                                ("regime", regime.name().to_string()),
                                (
                                    "completed",
                                    form3::flag(scenario.run(*regime).completed).to_string(),
                                ),
                            ],
                        ),
                    );
                }
            }
        }
    }
    Ok(out)
}

fn accepted_report() -> Result<MvpReport, PlaybookError> {
    let report = run_mvp()?;
    let required = [
        "FIRST_DOWN",
        "SECOND_DOWN_GAIN",
        "THIRD_DOWN_PASS",
        "SECOND_DOWN_LOSS",
        "FOURTH_DOWN",
        "THIRD_DOWN_RUSH",
    ];
    for name in required {
        if !report.stages.iter().any(|stage| stage == name) {
            return Err(PlaybookError::ReplayDivergence(format!(
                "missing stage {name}"
            )));
        }
    }
    if !report.replay_match || report.package_leaks || report.duplicate_failed_plays != 0 {
        return Err(PlaybookError::ReplayDivergence(
            "mvp acceptance failed".into(),
        ));
    }
    if report.false_completion_claims == 0 || report.control_duplicate_failed_plays == 0 {
        return Err(PlaybookError::ReplayDivergence(
            "mvp counters failed".into(),
        ));
    }
    Ok(report)
}

trait DispositionName {
    fn machine(self) -> &'static str;
}

impl DispositionName for playbook_core::Disposition {
    fn machine(self) -> &'static str {
        match self {
            Self::Convert => "convert",
            Self::FieldGoal => "field_goal",
            Self::Punt => "punt",
            Self::Reset => "reset",
            Self::Abandon => "abandon",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_and_mvp_commands_exit_clean() {
        assert_eq!(run(&["policy"]), 0);
        assert_eq!(run(&["policy", "pass-heavy"]), 0);
        assert_eq!(run(&["mvp"]), 0);
        let journal = temp_path("playbook-cli-journal");
        assert_eq!(run(&["mvp", "--journal", &journal]), 0);
        assert!(std::path::Path::new(&journal).exists());
        assert_eq!(run(&["recover", &journal]), 0);
        assert_eq!(run(&["mvp", "--journal", &journal, "extra"]), 1);
        std::fs::remove_file(&journal).ok();
        assert_eq!(run(&["sprint"]), 0);
        assert_eq!(run(&["priors"]), 0);
        assert_eq!(
            run(&[
                "sim",
                "--trace",
                "mvp",
                "--policy",
                "playbook-policy-pass-heavy"
            ]),
            0
        );
        assert_eq!(run(&["sim", "--policy", "builtin", "--trace", "mvp"]), 0);
        assert_eq!(run(&["explain"]), 0);
        assert_eq!(run(&["status"]), 0);
    }

    #[test]
    fn sim_rejects_unknown_and_missing_arguments() {
        assert_eq!(run(&["sim", "--bogus"]), 1);
        assert_eq!(run(&["sim", "--trace"]), 1);
        assert_eq!(run(&["sim", "--policy"]), 1);
        assert_eq!(run(&["sim", "--policy", "mystery"]), 1);
        assert_eq!(run(&["mvp", "--journal"]), 1);
        assert_eq!(run(&["mvp", "--bogus"]), 1);
    }

    #[test]
    fn recover_rejects_missing_and_empty_journals() {
        assert_eq!(run(&["recover"]), 1);
        assert_eq!(run(&["recover", "/tmp/path-does-not-exist-pb"]), 1);
        let path = temp_path("playbook-cli-recover-empty");
        std::fs::write(&path, b"").unwrap();
        assert_eq!(run(&["recover", &path]), 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn sim_loads_json_event_log_paths() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/sprint_transcript.json");
        let trace = load_trace(path.to_str().unwrap()).unwrap();
        assert!(!trace.is_empty());
        assert_eq!(resolve_policy("builtin").unwrap(), Policy::builtin());
        assert_eq!(
            resolve_policy("playbook-policy-pass-heavy").unwrap(),
            Policy::pass_heavy_contrast()
        );
        let builtin = compare_policies_with(&trace, &Policy::builtin()).unwrap();
        let pass_heavy = compare_policies_with(&trace, &Policy::pass_heavy_contrast()).unwrap();
        assert!(!builtin.down_labels_differ);
        assert!(pass_heavy.down_labels_differ);
    }

    #[test]
    fn cards_render_to_stdout_and_to_a_directory() {
        assert_eq!(run(&["cards", "--threeform"]), 0);
        assert_eq!(run(&["cards"]), 0);
        let dir = std::env::temp_dir().join(format!("playbook-cards-{}", std::process::id()));
        let dir = dir.to_str().unwrap().to_string();
        assert_eq!(run(&["cards", "--out", &dir]), 0);
        let page = std::path::Path::new(&dir).join("playbook.html");
        let html = std::fs::read_to_string(&page).unwrap();
        assert!(html.contains("THE PLAYBOOK"));
        assert!(html.contains("event=play name=spread"));
        assert_eq!(run(&["cards", "--bogus"]), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn otlp_requires_exactly_one_endpoint() {
        assert_eq!(run(&["otlp"]), 1);
        assert_eq!(run(&["otlp", "https://nope"]), 1);
    }

    #[test]
    fn huddle_study_is_deterministic_and_labels_regimes() {
        let first = phase_eight_study();
        let second = phase_eight_study();
        assert_eq!(first, second);
        let table = study_table(&first);
        assert!(table.contains("A_control"));
        assert!(table.contains("D_hierarchical_triggered"));
    }

    #[test]
    fn otlp_bench_defaults_and_rejects_unknown_arguments() {
        assert_eq!(run(&["otlp-bench"]), 0);
        assert_eq!(run(&["otlp-bench", "--spans", "50", "--runs", "2"]), 0);
        assert_eq!(run(&["otlp-bench", "--bogus"]), 1);
    }

    #[test]
    fn otlp_round_trips_a_local_collector() {
        let (port, server) = spawn_local_collector();
        assert_eq!(
            run(&["otlp", &format!("http://127.0.0.1:{port}/v1/traces")]),
            0
        );
        server.join().unwrap();
    }

    #[test]
    fn form3_and_basic_flag_matrix_routes_output() {
        assert_eq!(run(&["mvp", "--form3"]), 0);
        let (code, form3) = execute(&["mvp", "--form3"]);
        assert_eq!(code, 0);
        assert!(form3.contains("event=mvp_complete"));
        assert!(form3.contains("event=mvp_stage stage=FIRST_DOWN index=0"));

        assert_eq!(run(&["mvp", "--basic"]), 0);
        let (code, basic) = execute(&["mvp", "--basic"]);
        assert_eq!(code, 0);
        assert!(basic.contains("FIRST_DOWN"));
        assert!(basic.contains("replay_match true"));
        assert!(!basic.contains("event="));

        let (code, empty) = execute(&["mvp", "--form3", "--basic"]);
        assert_eq!(code, 2);
        assert!(empty.is_empty());
        let (code, empty) = execute(&["--basic", "--form3", "mvp"]);
        assert_eq!(code, 2);
        assert!(empty.is_empty());

        let (code, status) = execute(&["status", "--form3"]);
        assert_eq!(code, 0);
        assert!(status.contains("event=tactical_status"));

        let (code, cards) = execute(&["cards", "--form3"]);
        assert_eq!(code, 0);
        assert!(cards.contains("event=play name=spread"));
    }

    #[test]
    fn style_flags_work_before_the_subcommand_too() {
        let (code, form3) = execute(&["--form3", "priors"]);
        assert_eq!(code, 0);
        assert!(form3.contains("event=priors_total"));
        let (code, basic) = execute(&["--basic", "explain"]);
        assert_eq!(code, 0);
        assert!(!basic.contains("event="));
    }

    #[test]
    fn form3_streams_keep_the_hud_grammar() {
        let journal = temp_path("playbook-cli-form3-grammar");
        let (code, _) = execute(&["mvp", "--journal", &journal]);
        assert_eq!(code, 0);
        let commands: Vec<Vec<&str>> = vec![
            vec!["mvp"],
            vec!["sim"],
            vec!["status"],
            vec!["explain"],
            vec!["priors"],
            vec!["recover", &journal],
            vec!["huddle-study"],
            vec!["cards"],
            vec!["policy"],
            vec!["otlp-bench", "--spans", "10", "--runs", "1"],
        ];
        for command in &commands {
            let mut argv = vec!["--form3"];
            argv.extend(command.iter().copied());
            let (code, output) = execute(&argv);
            assert_eq!(code, 0, "command failed: {command:?}");
            assert!(!output.is_empty(), "no form3 output: {command:?}");
            assert_form3_grammar(&output);
        }
        std::fs::remove_file(&journal).ok();
    }

    #[test]
    fn otlp_form3_names_each_summary_field() {
        let (port, server) = spawn_local_collector();
        let (code, output) = execute(&[
            "--form3",
            "otlp",
            &format!("http://127.0.0.1:{port}/v1/traces"),
        ]);
        assert_eq!(code, 0);
        server.join().unwrap();
        assert!(output.contains("event=otlp endpoint="));
        assert!(output.contains("event=otlp spans="));
        assert!(output.contains("event=otlp status="));
        assert_form3_grammar(&output);
    }

    #[test]
    fn basic_mode_strips_the_decorations() {
        let (code, cards) = execute(&["cards", "--basic"]);
        assert_eq!(code, 0);
        let names: Vec<&str> = cards.lines().collect();
        assert_eq!(names.len(), playbook_cards::all_cards().len());
        assert_eq!(names.first().copied(), Some("spread"));
        assert!(names.contains(&"two_high"));
        assert!(!cards.contains("event="));

        let (code, status) = execute(&["status", "--basic"]);
        assert_eq!(code, 0);
        let values: Vec<&str> = status.lines().collect();
        assert_eq!(values.len(), 4);
        assert!(values.iter().all(|value| !value.contains('=')));
        assert!(values[1].chars().all(|c| c.is_ascii_digit()));
        assert!(values[2].chars().all(|c| c.is_ascii_digit()));

        let (code, huddle) = execute(&["huddle-study", "--basic"]);
        assert_eq!(code, 0);
        assert!(!huddle.is_empty());
        assert!(huddle.lines().all(|cell| cell.starts_with("scenario=")));

        let (code, sim) = execute(&["sim", "--basic"]);
        assert_eq!(code, 0);
        assert!(sim.starts_with("trace=mvp\n"));
        assert!(sim.contains("policy="));
        assert!(sim.contains("ytg_match="));
        assert!(sim.contains("efficiency_ratio_milli="));
    }

    /// A temp file path unique to this test process.
    fn temp_path(stem: &str) -> String {
        std::env::temp_dir()
            .join(format!("{stem}-{}.jsonl", std::process::id()))
            .to_str()
            .unwrap()
            .to_string()
    }

    /// A one-shot local OTLP collector: the port plus the thread that
    /// answers a single POST.
    fn spawn_local_collector() -> (u16, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = Vec::new();
            std::io::Read::read_to_end(&mut stream, &mut buffer).ok();
            std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}",
            )
            .ok();
        });
        (port, server)
    }

    /// Split one line into tokens, honouring double-quoted values.
    fn form3_tokens(line: &str) -> Vec<&str> {
        let mut tokens = Vec::new();
        let mut start = 0;
        let mut in_quotes = false;
        for (index, character) in line.char_indices() {
            match character {
                '"' => in_quotes = !in_quotes,
                ' ' if !in_quotes => {
                    if start < index {
                        tokens.push(&line[start..index]);
                    }
                    start = index + 1;
                }
                _ => {}
            }
        }
        if start < line.len() {
            tokens.push(&line[start..]);
        }
        tokens
    }

    /// Assert every line of a form3 stream follows the HUD grammar: an
    /// `event=` prefix, `key=value` tokens throughout, no bare spaces inside
    /// a value, quotes only in wrapping pairs.
    fn assert_form3_grammar(output: &str) {
        for line in output.lines() {
            assert!(line.starts_with("event="), "missing event prefix: {line}");
            for token in form3_tokens(line).into_iter().skip(1) {
                let Some((key, value)) = token.split_once('=') else {
                    panic!("token without key=value: {token}");
                };
                assert!(
                    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    "unsafe key: {key}"
                );
                let quotes = value.matches('"').count();
                if value.contains(' ') {
                    assert!(
                        value.starts_with('"') && value.ends_with('"') && quotes == 2,
                        "bare space in token: {token}"
                    );
                } else {
                    assert!(
                        quotes == 0
                            || (value.starts_with('"') && value.ends_with('"') && quotes == 2),
                        "stray quote in token: {token}"
                    );
                }
            }
        }
    }
}
