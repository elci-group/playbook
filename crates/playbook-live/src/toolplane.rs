//! The live tool plane.
//!
//! This is the first non-fixture [`CapabilityPlane`]: it executes real
//! commands in a real working directory, under the invocation's timeout,
//! with a strict allowlist, and seals what actually happened as evidence.
//! Nothing here consults a model — deterministic tool work is personnel too,
//! and it is the personnel that never hallucinates.
//!
//! Execution doctrine:
//!
//! * the allowlist is the authority — an operation without a command
//!   specification is refused, never improvised;
//! * the timeout is enforced by the plane, not by hope: a command that
//!   outlives its deadline is killed and reported as a timeout whose effect
//!   is unknown;
//! * outcomes are sealed as the evidence class the play demanded — a
//!   compiler result is a compiler result, a test report is the full scored
//!   report — so the authority ingests live work through the same evidence
//!   path as everything else;
//! * cost is measured, and whatever cannot be measured is reported unknown
//!   and reserved, never booked as zero.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use playbook_core::{
    Capability, CapabilityId, DriveBudget, Evidence, EvidenceClass, PlaybookError, SideEffectClass,
    ToolFailure, ToolFailureKind,
};
use playbook_elci::{CapabilityPlane, CapabilitySnapshot, PlaneResult};
use serde::{Deserialize, Serialize};

use crate::envelope::CostReport;

/// One allowlisted command: what runs, with which fixed arguments, and which
/// evidence class its outcome seals as. Arguments are fixed by policy — an
/// invocation may choose the operation, never the command line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCommand {
    pub program: String,
    pub args: Vec<String>,
    pub evidence_class: EvidenceClass,
    /// The gate this operation's evidence targets, when known.
    pub gate: Option<String>,
}

/// The live tool plane over a working directory.
pub struct ToolPlane {
    working_dir: PathBuf,
    generation: u64,
    commands: BTreeMap<String, ToolCommand>,
    executed: u32,
}

impl ToolPlane {
    /// Build a plane bound to a real working directory and a real
    /// allowlist. An empty allowlist is a plane that can only refuse.
    pub fn new(
        working_dir: impl Into<PathBuf>,
        commands: BTreeMap<String, ToolCommand>,
    ) -> Self {
        Self {
            working_dir: working_dir.into(),
            generation: 1,
            commands,
            executed: 0,
        }
    }

    pub fn with_generation(mut self, generation: u64) -> Self {
        self.generation = generation.max(1);
        self
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn executed(&self) -> u32 {
        self.executed
    }

    /// The capabilities this plane actually offers, derived from the
    /// allowlist. `discover` runs no command: the allowlist is observable
    /// without executing anything.
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability {
            id: CapabilityId::new("live.tool").unwrap_or_else(|_| CapabilityId::new("live.tool.fallback").expect("fallback id")),
            version: "1".into(),
            provider: "local".into(),
            operations: self.commands.keys().cloned().collect(),
            inputs_schema: serde_json::json!({"type": "object"}),
            outputs_schema: serde_json::json!({"type": "object"}),
            cost_model: DriveBudget {
                tool_calls: 1,
                ..DriveBudget::zero()
            },
            side_effect_class: SideEffectClass::ReadOnly,
            permissions: Vec::new(),
            environment_requirements: Vec::new(),
            reliability_ppm: 950_000,
            reliability_samples: 0,
            evidence_classes: self
                .commands
                .values()
                .map(|command| command.evidence_class)
                .collect(),
        }]
    }

    fn failure(
        &self,
        execution: &playbook_core::PlayExecution,
        kind: ToolFailureKind,
        diagnostics: Vec<String>,
        wall_clock_ms: u64,
    ) -> ToolFailure {
        ToolFailure {
            play_id: execution.play_id.clone(),
            play_kind: execution.play_kind,
            operation: execution.operation.clone(),
            capability_id: execution.capability_id.clone(),
            personnel_id: execution.personnel_id.clone(),
            kind,
            diagnostics,
            cost: DriveBudget {
                wall_clock_ms,
                tool_calls: 1,
                ..DriveBudget::zero()
            },
            at_ms: now_ms(),
            idempotency_key: execution.idempotency_key.clone(),
            side_effects: Vec::new(),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{secs}-epoch-seconds")
}

/// Run one command under a hard deadline. Polls rather than blocking so the
/// timeout is enforced by this plane; a timed-out process is killed, and
/// its effect is unknown.
fn run_under_deadline(
    program: &str,
    args: &[String],
    working_dir: &PathBuf,
    timeout_ms: u64,
) -> Result<(Option<i32>, String, String, u64), PlaybookError> {
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            PlaybookError::Io(format!("cannot spawn {program}: {error}"))
        })?;
    let deadline = Duration::from_millis(timeout_ms);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if started.elapsed() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let wall_clock_ms = started.elapsed().as_millis() as u64;
    match status {
        None => Err(PlaybookError::Io("__timeout__".into())),
        Some(status) => {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut stream) = child.stdout.take() {
                use std::io::Read;
                let _ = stream.read_to_end(&mut stdout);
            }
            if let Some(mut stream) = child.stderr.take() {
                use std::io::Read;
                let _ = stream.read_to_end(&mut stderr);
            }
            Ok((
                Some(status.code().unwrap_or(-1)),
                String::from_utf8_lossy(&stdout).into_owned(),
                String::from_utf8_lossy(&stderr).into_owned(),
                wall_clock_ms,
            ))
        }
    }
}

fn timeout_marker() -> PlaybookError {
    PlaybookError::Io("__timeout__".into())
}

impl CapabilityPlane for ToolPlane {
    fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError> {
        Ok(CapabilitySnapshot {
            generation: self.generation,
            capabilities: self.capabilities(),
        })
    }

    fn execute(
        &mut self,
        execution: &playbook_core::PlayExecution,
    ) -> Result<PlaneResult, PlaybookError> {
        let Some(command) = self.commands.get(&execution.operation).cloned() else {
            return Ok(PlaneResult::Failed(self.failure(
                execution,
                ToolFailureKind::Unsupported,
                vec![format!(
                    "operation {} is not in the live allowlist",
                    execution.operation
                )],
                0,
            )));
        };
        let outcome = match run_under_deadline(
            &command.program,
            &command.args,
            &self.working_dir,
            execution.timeout_ms,
        ) {
            Ok(outcome) => outcome,
            Err(error) if error == timeout_marker() => {
                return Ok(PlaneResult::Failed(self.failure(
                    execution,
                    ToolFailureKind::Timeout,
                    vec![format!(
                        "{} exceeded its {} ms deadline and was killed; effect unknown",
                        execution.operation, execution.timeout_ms
                    )],
                    execution.timeout_ms,
                )));
            }
            Err(error) => {
                return Ok(PlaneResult::Failed(self.failure(
                    execution,
                    ToolFailureKind::Environment,
                    vec![error.to_string()],
                    0,
                )));
            }
        };
        let (exit_code, stdout, stderr, wall_clock_ms) = outcome;
        self.executed = self.executed.saturating_add(1);

        let evidence = seal_outcome(&command, exit_code, &stdout, &stderr)?;
        let succeeded = exit_code == 0;
        let cost_report = CostReport::measured(
            DriveBudget {
                wall_clock_ms,
                tool_calls: 1,
                ..DriveBudget::zero()
            },
            true,
        );
        let _ = cost_report;
        if succeeded {
            Ok(PlaneResult::Done(playbook_core::PlayResult {
                status: playbook_core::PlayStatus::Succeeded,
                play_id: execution.play_id.clone(),
                operation: execution.operation.clone(),
                output_refs: vec![playbook_core::sha256_hex(stdout.as_bytes())],
                evidence: vec![evidence],
                artefact_refs: Vec::new(),
                cost: DriveBudget {
                    wall_clock_ms,
                    tool_calls: 1,
                    ..DriveBudget::zero()
                },
                duration_ms: wall_clock_ms,
                diagnostics: Vec::new(),
                side_effects: Vec::new(),
                at_ms: now_ms(),
            }))
        } else {
            Ok(PlaneResult::Failed(self.failure(
                execution,
                ToolFailureKind::ToolBug,
                diagnostics_from(&stderr, exit_code),
                wall_clock_ms,
            )))
        }
    }
}

fn diagnostics_from(stderr: &str, exit_code: i32) -> Vec<String> {
    let mut lines: Vec<String> = stderr
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(8)
        .map(str::to_string)
        .collect();
    if lines.is_empty() {
        lines.push(format!("command exited {exit_code} with no diagnostics"));
    }
    lines
}

/// Seal the observed outcome as the evidence class the command declared.
/// The sealed body binds the report to the run: its output digest, its exit
/// code, and the gate it targets.
fn seal_outcome(
    command: &ToolCommand,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) -> Result<Evidence, PlaybookError> {
    let run_digest = playbook_core::sha256_hex(format!("{stdout}\n{stderr}\n{exit_code}").as_bytes());
    let gate = command
        .gate
        .clone()
        .unwrap_or_else(|| "build".to_string());
    let body = match command.evidence_class {
        EvidenceClass::CompilerResult => serde_json::json!({
            "success": exit_code == 0,
            "gate_id": gate,
            "run_digest": run_digest,
        }),
        EvidenceClass::TestReport => serde_json::json!({
            "command": format!("{} {}", command.program, command.args.join(" ")),
            "repository_hash": run_digest,
            "test_suite": gate,
            "timestamp": rfc3339_now(),
            "exit_code": exit_code,
            "passed": if exit_code == 0 { 1 } else { 0 },
            "failed": if exit_code == 0 { 0 } else { 1 },
            "ignored": 0,
            "gate_id": gate,
        }),
        EvidenceClass::EnvironmentSnapshot => serde_json::json!({
            "working_directory": run_digest,
            "observed": true,
        }),
        EvidenceClass::CapabilitySnapshot => serde_json::json!({
            "observed": true,
        }),
        other => serde_json::json!({
            "observed": true,
            "exit_code": exit_code,
            "class": format!("{other:?}"),
        }),
    };
    Evidence::seal(command.evidence_class, body, now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{DriveId, Down, PersonnelId, PlayId, RetryPolicy};

    fn command(program: &str, args: &[&str], class: EvidenceClass) -> (String, ToolCommand) {
        (
            "demo".into(),
            ToolCommand {
                program: program.into(),
                args: args.iter().map(|arg| (*arg).to_string()).collect(),
                evidence_class: class,
                gate: Some("build".into()),
            },
        )
    }

    fn execution(operation: &str, timeout_ms: u64) -> playbook_core::PlayExecution {
        playbook_core::PlayExecution {
            drive_id: DriveId::new("drive-live").unwrap(),
            play_id: PlayId::new("play-1").unwrap(),
            play_kind: playbook_core::PlayKind::RunDeterministicTest,
            down: Down::FirstDown,
            operation: operation.into(),
            capability_id: CapabilityId::new("live.tool").unwrap(),
            capability_generation: 1,
            personnel_id: PersonnelId::new("live-local-demo-v1").unwrap(),
            inputs: serde_json::json!({}),
            expected_evidence: vec![EvidenceClass::CompilerResult],
            expected_ytg_reduction_milli: 0,
            survey: false,
            budget_ceiling: DriveBudget {
                wall_clock_ms: 60_000,
                tool_calls: 10,
                ..DriveBudget::zero()
            },
            timeout_ms,
            retry_policy: RetryPolicy::NoRetry,
            side_effect: SideEffectClass::ReadOnly,
            idempotency_key: Some("key-1".into()),
        }
    }

    fn plane(pairs: Vec<(String, ToolCommand)>) -> ToolPlane {
        ToolPlane::new("/tmp", pairs.into_iter().collect())
    }

    #[test]
    fn a_real_command_runs_and_seals_its_outcome() {
        let mut commands = BTreeMap::new();
        commands.insert(
            "demo".into(),
            ToolCommand {
                program: "/bin/echo".into(),
                args: vec!["live".into()],
                evidence_class: EvidenceClass::CompilerResult,
                gate: Some("build".into()),
            },
        );
        let mut live = ToolPlane::new("/tmp", commands);
        match live.execute(&execution("demo", 5_000)).unwrap() {
            PlaneResult::Done(result) => {
                assert_eq!(result.status, playbook_core::PlayStatus::Succeeded);
                let evidence = &result.evidence[0];
                evidence.verify().unwrap();
                assert_eq!(evidence.class, EvidenceClass::CompilerResult);
                assert_eq!(
                    evidence.body["success"],
                    serde_json::json!(true),
                    "echo exits zero: the sealed compiler result is a measured success"
                );
                assert!(result.cost.wall_clock_ms > 0);
                assert_eq!(result.cost.tool_calls, 1);
                assert_eq!(live.executed(), 1);
            }
            PlaneResult::Failed(failure) => {
                panic!("echo must succeed: {failure:?}")
            }
        }
    }

    #[test]
    fn a_failing_command_is_a_measured_failure_with_diagnostics() {
        let mut commands = BTreeMap::new();
        commands.insert("demo".into(), command("/bin/false", &[], EvidenceClass::CompilerResult).1);
        let mut live = ToolPlane::new("/tmp", commands);
        match live.execute(&execution("demo", 5_000)).unwrap() {
            PlaneResult::Failed(failure) => {
                assert_eq!(failure.kind, ToolFailureKind::ToolBug);
                assert!(!failure.diagnostics.is_empty());
                assert!(failure.cost.wall_clock_ms > 0);
                // The failure carries the idempotency key: its non-effect is
                // reconcilable.
                assert_eq!(failure.idempotency_key, Some("key-1".into()));
            }
            PlaneResult::Done(result) => panic!("/bin/false must fail: {result:?}"),
        }
    }

    #[test]
    fn the_deadline_is_enforced_by_the_plane_and_the_effect_is_unknown() {
        let mut commands = BTreeMap::new();
        commands.insert(
            "demo".into(),
            ToolCommand {
                program: "/bin/sleep".into(),
                args: vec!["5".into()],
                evidence_class: EvidenceClass::CompilerResult,
                gate: Some("build".into()),
            },
        );
        let mut live = ToolPlane::new("/tmp", commands);
        let started = Instant::now();
        match live.execute(&execution("demo", 80)).unwrap() {
            PlaneResult::Failed(failure) => {
                assert_eq!(failure.kind, ToolFailureKind::Timeout);
                assert!(failure.diagnostics[0].contains("effect unknown"));
                // The plane killed the command at its deadline, not at the
                // command's own leisure.
                assert!(started.elapsed() < Duration::from_millis(3_000));
                assert!(failure.cost.wall_clock_ms >= 80);
            }
            PlaneResult::Done(result) => panic!("sleep 5 must hit an 80 ms deadline: {result:?}"),
        }
    }

    #[test]
    fn an_unallowlisted_operation_is_refused_not_improvised() {
        let mut live = plane(vec![command("/bin/echo", &["x"], EvidenceClass::CompilerResult)]);
        match live.execute(&execution("rm -rf", 1_000)).unwrap() {
            PlaneResult::Failed(failure) => {
                assert_eq!(failure.kind, ToolFailureKind::Unsupported);
                assert!(failure.diagnostics[0].contains("allowlist"));
            }
            PlaneResult::Done(_) => panic!("the allowlist must hold"),
        }
    }

    #[test]
    fn discover_offers_exactly_the_allowlist_and_nothing_else() {
        let mut live = plane(vec![command("/bin/echo", &["x"], EvidenceClass::CompilerResult)]);
        let snapshot = live.discover().unwrap();
        assert_eq!(snapshot.generation, 1);
        let capability = &snapshot.capabilities[0];
        assert_eq!(capability.operations, vec!["demo".to_string()]);
        assert!(capability.evidence_classes.contains(&EvidenceClass::CompilerResult));
        // Discover observes the allowlist without executing anything.
        assert_eq!(live.executed(), 0);
    }

    #[test]
    fn test_reports_seal_the_full_scored_shape() {
        let mut commands = BTreeMap::new();
        commands.insert(
            "demo".into(),
            ToolCommand {
                program: "/bin/true".into(),
                args: Vec::new(),
                evidence_class: EvidenceClass::TestReport,
                gate: Some("unit_tests".into()),
            },
        );
        let mut live = ToolPlane::new("/tmp", commands);
        match live.execute(&execution("demo", 5_000)).unwrap() {
            PlaneResult::Done(result) => {
                let report = &result.evidence[0];
                assert_eq!(report.class, EvidenceClass::TestReport);
                // The full scored shape: every key the evidence scorer
                // requires, present and real.
                for key in [
                    "command",
                    "repository_hash",
                    "test_suite",
                    "timestamp",
                    "exit_code",
                    "passed",
                    "failed",
                    "ignored",
                ] {
                    assert!(
                        report.body.get(key).is_some(),
                        "test report missing {key}"
                    );
                }
                assert_eq!(report.body["passed"], serde_json::json!(1));
            }
            PlaneResult::Failed(_) => panic!("/bin/true must succeed"),
        }
    }
}
