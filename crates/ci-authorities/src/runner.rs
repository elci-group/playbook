//! The generic gate runner: spawn a specialist authority, capture its
//! observable output (stdout, stderr, exit), and reduce it to a
//! `GateRecord`.
//!
//! Failure semantics are fail-closed (§30): exit 0 is the only path to
//! `Pass`; a nonzero exit is `Fail`; any spawn failure, timeout, or
//! interrupted observation is `Error`. `Error` never equals `Pass`.

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ci_evidence::{Finding, GateOutcome, GateRecord};
use playbook_core::sha256_hex;

use crate::GateSpec;

/// How long the poll loop sleeps between `try_wait` checks.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// What the runner observed from one specialist invocation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedOutput {
    pub stdout: String,
    pub stderr: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The gate's input digest: sha256 over the caller-supplied revision
/// digest, the program, and its arguments, all NUL-joined so no
/// concatenation ambiguity can collide two different invocations.
pub fn input_hash(input_digest: &str, program: &str, args: &[&str]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 2);
    parts.push(input_digest.to_string());
    parts.push(program.to_string());
    parts.extend(args.iter().map(|a| (*a).to_string()));
    sha256_hex(parts.join("\u{0}").as_bytes())
}

/// Whether a binary is resolvable, via `which(1)` when it exists and a
/// direct spawn probe otherwise. Defensive: absence must produce a
/// skip, not a crash.
pub fn binary_available(binary: &str) -> bool {
    if let Ok(output) = Command::new("which").arg(binary).output() {
        return output.status.success();
    }
    // No which(1): probe the binary directly. A spawn that gets far
    // enough to run (any exit status, even an argument error) proves it
    // exists; only an outright spawn error means it is unavailable.
    Command::new(binary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

/// Run one specialist gate to completion and reduce it to evidence.
/// See [`run_gate_capturing`] for the exact semantics; this returns
/// only the record.
pub fn run_gate(
    spec: &GateSpec,
    policy_version: &str,
    input_digest: &str,
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> GateRecord {
    run_gate_capturing(spec, policy_version, input_digest, program, args, timeout).0
}

/// Like [`run_gate`], but also returns the captured stdout/stderr so
/// authority adapters can parse specialist findings out of the output.
///
/// * exit 0 → `Pass`; nonzero exit → `Fail`.
/// * spawn failure or timeout → `Error` (fail closed; never `Pass`).
/// * The child's PATH is prepended with `$HOME/.cargo/bin` so that
///   cargo-installed specialist tools are resolvable under CI shells.
pub fn run_gate_capturing(
    spec: &GateSpec,
    policy_version: &str,
    input_digest: &str,
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> (GateRecord, CapturedOutput) {
    let started = Instant::now();
    let started_at_ms = now_ms();

    let record = GateRecord {
        gate_id: spec.gate_id.clone(),
        authority: spec.authority.clone(),
        // The runner does not query tool versions; adapters that know
        // their tool's version overwrite this.
        authority_version: "unversioned".to_string(),
        policy_version: policy_version.to_string(),
        classification: spec.classification,
        started_at_ms,
        completed_at_ms: started_at_ms,
        input_hash: input_hash(input_digest, program, args),
        outcome: GateOutcome::Error,
        exit_code: None,
        findings: Vec::new(),
        artifact_hashes: Vec::new(),
        duration_ms: 0,
    };

    let finish = |mut record: GateRecord,
                  output: CapturedOutput,
                  outcome: GateOutcome,
                  exit: Option<i32>,
                  findings: Vec<Finding>| {
        record.outcome = outcome;
        record.exit_code = exit;
        record.findings = findings;
        record.completed_at_ms = now_ms();
        record.duration_ms = started.elapsed().as_millis() as u64;
        (record, output)
    };

    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Ok(home) = std::env::var("HOME") {
        let cargo_bin = format!("{home}/.cargo/bin");
        let child_path = match std::env::var("PATH") {
            Ok(existing) => format!("{cargo_bin}:{existing}"),
            Err(_) => cargo_bin,
        };
        command.env("PATH", child_path);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return finish(
                record,
                CapturedOutput::default(),
                GateOutcome::Error,
                None,
                vec![Finding {
                    code: "spawn_failure".to_string(),
                    severity: "error".to_string(),
                    message: format!("failed to spawn {program:?}: {error}"),
                }],
            );
        }
    };

    // Drain stdout/stderr on helper threads so a chatty child cannot
    // deadlock on full pipes while we poll for the deadline.
    let stdout_handle = child.stdout.take().map(|mut s| {
        thread::spawn(move || {
            let mut buf = String::new();
            use std::io::Read;
            let _ = s.read_to_string(&mut buf);
            buf
        })
    });
    let stderr_handle = child.stderr.take().map(|mut s| {
        thread::spawn(move || {
            let mut buf = String::new();
            use std::io::Read;
            let _ = s.read_to_string(&mut buf);
            buf
        })
    });

    // std has no wait_timeout; poll try_wait against a deadline.
    let deadline = started + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if Instant::now() >= deadline {
                    break None;
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let output = join_output(stdout_handle, stderr_handle);
                return finish(
                    record,
                    output,
                    GateOutcome::Error,
                    None,
                    vec![Finding {
                        code: "wait_failure".to_string(),
                        severity: "error".to_string(),
                        message: format!("failed to observe {program:?}: {error}"),
                    }],
                );
            }
        }
    };

    let output = join_output(stdout_handle, stderr_handle);

    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        return finish(
            record,
            output,
            GateOutcome::Error,
            None,
            vec![Finding {
                code: "timeout".to_string(),
                severity: "error".to_string(),
                message: format!("{program:?} exceeded timeout of {timeout:?}"),
            }],
        );
    };

    let outcome = if status.success() {
        GateOutcome::Pass
    } else {
        GateOutcome::Fail
    };
    finish(record, output, outcome, status.code(), Vec::new())
}

/// The standard skip finding recorded when a specialist binary cannot
/// be found in this environment.
pub(crate) fn authority_unavailable(binary: &str) -> Finding {
    Finding {
        code: "authority_binary_unavailable".to_string(),
        severity: "info".to_string(),
        message: format!("authority binary unavailable: `{binary}`"),
    }
}

fn join_output(
    stdout: Option<thread::JoinHandle<String>>,
    stderr: Option<thread::JoinHandle<String>>,
) -> CapturedOutput {
    let stdout = stdout.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = stderr.and_then(|h| h.join().ok()).unwrap_or_default();
    CapturedOutput { stdout, stderr }
}
