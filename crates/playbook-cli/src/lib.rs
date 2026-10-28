//! Playbook command line. `run` is the testable entry point.

#![forbid(unsafe_code)]

use playbook_core::PlaybookError;
use playbook_observability::{audit_text, stage_line};
use playbook_policy::Policy;
use playbook_sim::{compare_policies_with, fold, run_mvp, ApplyMode, DriveEvent, MvpReport};

pub fn run(args: &[impl AsRef<str>]) -> i32 {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    match args.first().copied() {
        Some("mvp") => command(print_mvp(&args[1..])),
        Some("sim") => command(print_sim(&args[1..])),
        Some("sprint") => command(print_sprint()),
        Some("priors") => command(print_priors()),
        Some("explain") => command(print_explain()),
        Some("status") => command(print_status()),
        Some("policy") => command(print_policy(args.get(1).copied())),
        Some("recover") => command(print_recover(args.get(1).copied())),
        Some("otlp") => command(print_otlp(&args[1..])),
        Some("otlp-bench") => command(print_otlp_bench(&args[1..])),
        _ => {
            eprintln!("usage: playbook <mvp|sim|sprint|priors|explain|status|policy|recover|otlp|otlp-bench>");
            eprintln!("  mvp [--journal <path>] runs the acceptance drive, optionally through a durable journal");
            eprintln!("  recover <journal> folds a recovered drive and reports what the host owes");
            eprintln!("  otlp <endpoint> exports the acceptance drive trace over OTLP/HTTP JSON");
            eprintln!("  otlp-bench --spans N [--endpoint url] [--runs R] measures encode and export throughput");
            2
        }
    }
}

fn command(result: Result<(), PlaybookError>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn print_mvp(args: &[&str]) -> Result<(), PlaybookError> {
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
    for stage in &report.stages {
        println!("{stage}");
    }
    println!("replay_match {}", report.replay_match);
    if let Some(matched) = report.journal_replay_match {
        println!("journal_replay_match {matched}");
    }
    println!(
        "disposition {}",
        report
            .disposition
            .map(|item| item.machine())
            .unwrap_or("none")
    );
    println!("false_completion_claims {}", report.false_completion_claims);
    println!("duplicate_failed_plays {}", report.duplicate_failed_plays);
    println!(
        "control_duplicate_failed_plays {}",
        report.control_duplicate_failed_plays
    );
    Ok(())
}

fn print_sim(args: &[&str]) -> Result<(), PlaybookError> {
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
    println!("trace {trace_name}");
    println!("policy {}", policy.version);
    println!("ytg_match {}", comparison.ytg_match);
    println!("budget_match {}", comparison.budget_match);
    println!("completion_match {}", comparison.completion_match);
    println!("down_labels_differ {}", comparison.down_labels_differ);
    println!("completion_status {}", comparison.completion_status);
    println!("ytg_trajectory {:?}", comparison.ytg_trajectory);
    println!("budget_remaining_ppm {}", comparison.budget_remaining_ppm);
    println!("tool_failures {}", comparison.tool_failures);
    println!("model_calls {}", comparison.model_calls);
    println!("personnel_changes {}", comparison.personnel_changes);
    println!("fourth_down_index {:?}", comparison.fourth_down_index);
    println!("wall_clock_ms {}", comparison.wall_clock_ms);
    println!("monetary_cost_micros {}", comparison.monetary_cost_micros);
    println!("gates_satisfied {}", comparison.gates_satisfied);
    println!("yards_gained_ppm {}", comparison.yards_gained_ppm);
    println!("budget_spent_ppm {}", comparison.budget_spent_ppm);
    println!(
        "efficiency_ratio_milli {}",
        comparison.efficiency_ratio_milli
    );
    Ok(())
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

fn print_sprint() -> Result<(), PlaybookError> {
    let report = accepted_report()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report.events).map_err(PlaybookError::json)?
    );
    Ok(())
}

fn print_priors() -> Result<(), PlaybookError> {
    let report = accepted_report()?;
    let priors = report.final_state.priors();
    println!("samples {}", priors.samples());
    for (personnel, row) in priors.rows() {
        println!(
            "{personnel} samples={} realized_reduction_sum_milli={}",
            row.samples, row.realized_reduction_sum_milli
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(priors).map_err(PlaybookError::json)?
    );
    Ok(())
}

fn print_explain() -> Result<(), PlaybookError> {
    let report = accepted_report()?;
    println!(
        "{}",
        audit_text(&report.final_state.snapshot(&Policy::builtin()))
    );
    Ok(())
}

fn print_status() -> Result<(), PlaybookError> {
    let report = accepted_report()?;
    let snapshot = report.final_state.snapshot(&Policy::builtin());
    println!("{}", stage_line(&snapshot));
    println!(
        "{}",
        serde_json::to_string(&snapshot).map_err(PlaybookError::json)?
    );
    Ok(())
}

fn print_policy(variant: Option<&str>) -> Result<(), PlaybookError> {
    let policy = match variant {
        Some("pass-heavy") => Policy::pass_heavy_contrast(),
        _ => Policy::builtin(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&policy).map_err(PlaybookError::json)?
    );
    Ok(())
}

fn print_recover(path: Option<&str>) -> Result<(), PlaybookError> {
    let Some(path) = path else {
        return Err(PlaybookError::Evidence(
            "recover requires a journal path".into(),
        ));
    };
    let recovery = playbook_journal::Journal::open(std::path::Path::new(path))?;
    if recovery.events.is_empty() {
        return Err(PlaybookError::Evidence("journal holds no drive".into()));
    }
    if recovery.repaired_tail {
        println!("repaired_tail true");
    }
    let policy = Policy::builtin();
    let state = fold(&recovery.events, &policy, ApplyMode::Audit)?;
    println!("drive_id {}", state.drive_id().as_str());
    println!("status {:?}", state.status());
    println!("down {}", state.down().machine_id());
    println!("ytg_milli {}", state.ytg_milli());
    println!(
        "budget_remaining_ppm {}",
        state.budget_remaining_ppm(&policy)
    );
    println!("events {}", recovery.events.len());
    match state.open_execution() {
        Some(execution) => {
            println!("open_execution {}", execution.play_id.as_str());
            println!(
                "reconcile_required {}",
                execution.idempotency_key.as_deref().unwrap_or("none")
            );
        }
        None => println!("open_execution none"),
    }
    println!("unresolved_effects {}", state.unresolved_effects().len());
    Ok(())
}

fn print_otlp(args: &[&str]) -> Result<(), PlaybookError> {
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
    println!("endpoint {endpoint}");
    println!("spans {}", summary.spans);
    println!("bytes_sent {}", summary.bytes_sent);
    println!("status {}", summary.status);
    println!("retries {}", summary.retries);
    if let Some(partial_error) = &summary.partial_error {
        println!("partial_error {partial_error}");
    }
    Ok(())
}

fn print_otlp_bench(args: &[&str]) -> Result<(), PlaybookError> {
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
    println!("spans {span_count}");
    println!("runs {runs}");
    println!("bytes_per_span {bytes_per_span}");
    println!("encode_total_micros {}", encode_nanos / 1_000);
    println!("encode_per_span_nanos {encode_per_span_nanos}");
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
        println!("export_total_micros {}", export_nanos / 1_000);
        println!("export_per_call_micros {export_per_call_micros}");
        println!("exports_ok {exports_ok}");
        println!("export_errors {export_errors}");
    }
    Ok(())
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
        let journal =
            std::env::temp_dir().join(format!("playbook-cli-journal-{}.jsonl", std::process::id()));
        let journal = journal.to_str().unwrap().to_string();
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
        let path = std::env::temp_dir().join(format!(
            "playbook-cli-recover-empty-{}.jsonl",
            std::process::id()
        ));
        std::fs::write(&path, b"").unwrap();
        let path = path.to_str().unwrap().to_string();
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
    fn otlp_requires_exactly_one_endpoint() {
        assert_eq!(run(&["otlp"]), 1);
        assert_eq!(run(&["otlp", "https://nope"]), 1);
    }

    #[test]
    fn otlp_bench_defaults_and_rejects_unknown_arguments() {
        assert_eq!(run(&["otlp-bench"]), 0);
        assert_eq!(run(&["otlp-bench", "--spans", "50", "--runs", "2"]), 0);
        assert_eq!(run(&["otlp-bench", "--bogus"]), 1);
    }

    #[test]
    fn otlp_round_trips_a_local_collector() {
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
        assert_eq!(
            run(&["otlp", &format!("http://127.0.0.1:{port}/v1/traces")]),
            0
        );
        server.join().unwrap();
    }
}
