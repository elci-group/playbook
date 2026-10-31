//! `playbook-ci` — the runner crate for Playbook's evidence-gated CI
//! campaign (§33). A federation of specialist authorities whose results
//! converge on the canonical `CI_EVIDENCE/1` envelope (owned by the
//! `ci-evidence` crate); Kaptaind admits; no single tool is a universal
//! judge.
//!
//! Failure semantics are closed (§30): a gate that fails to SPAWN is
//! `Error` and fails closed. Skipped gates are recorded
//! (`SkippedNotApplicable`), never silently absent (§16).
//!
//! §34: a change to the judge is not an ordinary change. This binary is
//! part of the judged machinery; edits to it demand re-review of the whole
//! evidence pipeline.

#![forbid(unsafe_code)]

mod registry;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ci_evidence::{
    CiEvidence, Finding, GateClassification, GateOutcome, GateRecord, RevisionFreeze, Verdict,
};
use playbook_core::sha256_hex;
use registry::{Condition, GateDef, GATES, REQUIRED_GATES};

/// Where the evidence envelope is written.
const EVIDENCE_FILE: &str = "CI_EVIDENCE.json";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("freeze") => cmd_freeze(&args[1..]),
        Some("run") => cmd_run(&args[1..]),
        Some("verify") => cmd_verify(&args[1..]),
        _ => {
            eprintln!("usage: playbook-cli <freeze|run|verify> [args]");
            eprintln!("  freeze --policy-version <v>");
            eprintln!("  run --freeze <freeze.json> [--policy-version <v>] [--only gate1,gate2]");
            eprintln!("  verify --evidence <file> --commit <sha> --policy-version <v>");
            2
        }
    };
    std::process::exit(code);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// freeze
// ---------------------------------------------------------------------------

fn cmd_freeze(args: &[String]) -> i32 {
    let Some(policy_version) = arg_value(args, "--policy-version") else {
        eprintln!("freeze requires --policy-version");
        return 2;
    };
    let Some(commit) = git(&["rev-parse", "HEAD"]) else {
        eprintln!("freeze: git rev-parse HEAD failed");
        return 2;
    };
    // A shallow or single-commit history has no parent; freeze records the
    // commit itself so change-conditional gates fail closed to "ran".
    let parent = git(&["rev-parse", "HEAD~1"]).unwrap_or_else(|| commit.clone());
    let branch = git(&["branch", "--show-current"]).unwrap_or_default();

    // Determinism per revision: the diff digest covers the full diff; the
    // digest of `git status` output would drift with unrelated working-tree
    // noise, so the diff itself is the hashed artefact.
    let diff = git(&["diff", "HEAD"]).unwrap_or_default();
    let diff_digest = sha256_hex(diff.as_bytes());
    let lock = std::fs::read("Cargo.lock").unwrap_or_default();
    let lock_digest = sha256_hex(&lock);

    let toolchain = match (
        Command::new("cargo").arg("--version").output(),
        Command::new("rustc").arg("--version").output(),
    ) {
        (Ok(c), Ok(r)) => format!(
            "{}; {}",
            String::from_utf8_lossy(&c.stdout).trim(),
            String::from_utf8_lossy(&r.stdout).trim()
        ),
        _ => "unknown".to_string(),
    };

    // Frozen configuration surfaces: the CI contract itself and the gate
    // registry hash in with the evidence.
    let mut config_hashes = BTreeMap::new();
    for path in ["Cargo.lock", "deliver.toml", "goglz.yaml"] {
        let bytes = std::fs::read(path).unwrap_or_default();
        if !bytes.is_empty() {
            config_hashes.insert(path.to_string(), sha256_hex(&bytes));
        }
    }

    let freeze = RevisionFreeze {
        commit,
        parent,
        branch,
        diff_digest,
        lock_digest,
        toolchain,
        policy_version,
        implementation_version: env!("CARGO_PKG_VERSION").to_string(),
        config_hashes,
        frozen_at_ms: now_ms(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&freeze).expect("freeze serialises")
    );
    0
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

/// A child process result. Spawn failure is `Error` — fail closed (§30).
struct Exec {
    outcome: GateOutcome,
    exit_code: Option<i32>,
    output_tail: String,
}

/// Every child runs with PATH prefixed by $HOME/.cargo/bin so cargo/rustc
/// and the project's own tooling resolve identically in CI and locally.
fn child_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    let home = std::env::var("HOME").unwrap_or_default();
    let path = std::env::var("PATH").unwrap_or_default();
    let prefixed = format!("{home}/.cargo/bin:{path}");
    cmd.env("PATH", prefixed);
    cmd
}

fn exec(command: &str) -> Exec {
    let output = match child_command(command).output() {
        Ok(o) => o,
        Err(e) => {
            return Exec {
                outcome: GateOutcome::Error,
                exit_code: None,
                output_tail: format!("spawn failed: {e}"),
            };
        }
    };
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    // Keep the tail — the compiler's verdict is at the end.
    let tail: String = text
        .chars()
        .skip(text.chars().count().saturating_sub(800))
        .collect();
    let outcome = if output.status.success() {
        GateOutcome::Pass
    } else {
        GateOutcome::Fail
    };
    Exec {
        outcome,
        exit_code: output.status.code(),
        output_tail: tail.trim_end().to_string(),
    }
}

fn tool_on_path(tool: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let paths: Vec<String> = std::env::split_paths(&path)
        .map(|p| p.to_string_lossy().to_string())
        .chain(std::iter::once(format!("{home}/.cargo/bin")))
        .collect();
    paths.iter().any(|dir| Path::new(dir).join(tool).exists())
}

/// Whether any dependency surface changed versus the parent revision (§14).
fn deps_changed(freeze: &RevisionFreeze) -> bool {
    let Some(files) = git(&["diff", "--name-only", &freeze.parent, &freeze.commit]) else {
        // Cannot determine: fail closed to "applicable" so the gate runs.
        return true;
    };
    files
        .lines()
        .any(|f| f == "Cargo.lock" || f.ends_with("Cargo.toml") || f.ends_with("/Cargo.toml"))
}

/// Whether the claims surface (README, docs/) changed versus the parent.
fn claims_changed(freeze: &RevisionFreeze) -> bool {
    let Some(files) = git(&["diff", "--name-only", &freeze.parent, &freeze.commit]) else {
        return true;
    };
    files
        .lines()
        .any(|f| f == "README.md" || f.starts_with("docs/"))
}

/// Run `cargo build -p playbook-cli --offline` so the mvp replay binary
/// exists before `ci/mvp-replay` executes.
fn ensure_mvp_binary() -> Option<Exec> {
    let build = exec("cargo build -p playbook-cli --offline");
    if build.outcome != GateOutcome::Pass {
        return Some(build);
    }
    None
}

fn cmd_run(args: &[String]) -> i32 {
    let Some(freeze_path) = arg_value(args, "--freeze") else {
        eprintln!("run requires --freeze <freeze.json>");
        return 2;
    };
    let policy_version = arg_value(args, "--policy-version");
    let only: Option<Vec<String>> =
        arg_value(args, "--only").map(|s| s.split(',').map(|g| g.trim().to_string()).collect());

    let bytes = match std::fs::read(&freeze_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("run: cannot read {freeze_path}: {e}");
            return 2;
        }
    };
    let freeze: RevisionFreeze = match serde_json::from_slice(&bytes) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("run: {freeze_path} is not a RevisionFreeze: {e}");
            return 2;
        }
    };
    let policy = policy_version.unwrap_or_else(|| freeze.policy_version.clone());

    let mut evidence = CiEvidence::new(freeze.clone());
    let started_all = Instant::now();

    // The mvp replay needs its binary built first; build once, and only if
    // the gate is actually in the selected set.
    let mvp_selected = only
        .as_ref()
        .map(|sel| sel.iter().any(|g| g == "ci/mvp-replay"))
        .unwrap_or(true);
    let mvp_build_failure = if mvp_selected {
        ensure_mvp_binary()
    } else {
        None
    };

    for def in GATES {
        if let Some(select) = &only {
            if !select.iter().any(|g| g == def.id) {
                continue;
            }
        }
        let record = run_gate(def, &freeze, &policy, mvp_build_failure.as_ref());
        println!(
            "[gate] {:<14} {:?} -> {:?} ({}ms)",
            record.gate_id, record.classification, record.outcome, record.duration_ms
        );
        evidence.record(record);
    }

    let json = serde_json::to_string_pretty(&evidence).expect("evidence serialises");
    if let Err(e) = std::fs::write(EVIDENCE_FILE, &json) {
        eprintln!("run: cannot write {EVIDENCE_FILE}: {e}");
        return 2;
    }
    let verdict = evidence.derive_verdict(now_ms());
    println!(
        "[run] {} gates recorded in {}ms; derived verdict: {:?}; envelope: {EVIDENCE_FILE}",
        evidence.gates.len(),
        started_all.elapsed().as_millis(),
        verdict
    );
    match verdict {
        Verdict::Admitted => 0,
        Verdict::Rejected => 1,
    }
}

/// Execute one gate definition into a `GateRecord`. Policy lives in the
/// registry table; this function is the mechanism, identical for every gate.
fn run_gate(
    def: &GateDef,
    freeze: &RevisionFreeze,
    policy_version: &str,
    mvp_build_failure: Option<&Exec>,
) -> GateRecord {
    let started_at_ms = now_ms();
    let started = Instant::now();
    let input_hash = sha256_hex(def.command.as_bytes());

    // Condition evaluation: a gate whose condition does not hold is
    // SKIPPED and RECORDED, never absent (§16).
    let skip = match def.condition {
        Condition::Always => None,
        Condition::IfToolExists => {
            if tool_on_path(def.tool) {
                None
            } else {
                Some(GateOutcome::SkippedNotApplicable)
            }
        }
        Condition::IfDependenciesChanged => {
            if deps_changed(freeze) {
                None
            } else {
                Some(GateOutcome::SkippedNotApplicable)
            }
        }
        Condition::IfClaimsSurfaceChanged => {
            if claims_changed(freeze) {
                None
            } else {
                Some(GateOutcome::SkippedNotApplicable)
            }
        }
    };

    let (outcome, exit_code, findings) = if let Some(skip) = skip {
        (skip, None, Vec::new())
    } else if def.id == "ci/mvp-replay" {
        // The replay binary must have been built; a build failure is the
        // gate's error, not a spawn error.
        match mvp_build_failure {
            Some(build) => {
                let finding = Finding {
                    code: "ci/mvp-build".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "playbook-cli build failed: {}",
                        build.output_tail.chars().take(400).collect::<String>()
                    ),
                };
                (GateOutcome::Error, build.exit_code, vec![finding])
            }
            None => {
                let result = exec(def.command);
                let findings = findings_for(def, &result);
                (result.outcome, result.exit_code, findings)
            }
        }
    } else {
        let result = exec(def.command);
        let mut findings = findings_for(def, &result);
        if def.id == "ci/verity" && result.outcome == GateOutcome::Pass {
            findings.push(Finding {
                code: "verity/pending".to_string(),
                severity: "info".to_string(),
                message: "claim audit pending CLAIMS/1".to_string(),
            });
        }
        (result.outcome, result.exit_code, findings)
    };

    GateRecord {
        gate_id: def.id.to_string(),
        authority: def.authority.to_string(),
        authority_version: authority_version(def.authority),
        policy_version: policy_version.to_string(),
        classification: def.classification,
        started_at_ms,
        completed_at_ms: now_ms(),
        input_hash,
        outcome,
        exit_code,
        findings,
        artifact_hashes: Vec::new(),
        duration_ms: started.elapsed().as_millis() as u64,
    }
}

/// Findings carry the authority's voice; the runner never summarises a
/// failure away.
fn findings_for(def: &GateDef, result: &Exec) -> Vec<Finding> {
    match result.outcome {
        GateOutcome::Pass => Vec::new(),
        GateOutcome::Error => vec![Finding {
            code: "gate/spawn".to_string(),
            severity: "error".to_string(),
            message: format!("gate {} failed to execute: {}", def.id, result.output_tail),
        }],
        _ => vec![Finding {
            code: "gate/fail".to_string(),
            severity: "error".to_string(),
            message: format!(
                "gate {} exited non-zero (code {:?}): {}",
                def.id, result.exit_code, result.output_tail
            ),
        }],
    }
}

fn authority_version(authority: &str) -> String {
    if authority == "cargo" {
        if let Ok(out) = Command::new("cargo").arg("--version").output() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    "unversioned".to_string()
}

// ---------------------------------------------------------------------------
// verify
// ---------------------------------------------------------------------------

fn cmd_verify(args: &[String]) -> i32 {
    let Some(evidence_path) = arg_value(args, "--evidence") else {
        eprintln!("verify requires --evidence <file>");
        return 2;
    };
    let Some(commit) = arg_value(args, "--commit") else {
        eprintln!("verify requires --commit <sha>");
        return 2;
    };
    let Some(policy_version) = arg_value(args, "--policy-version") else {
        eprintln!("verify requires --policy-version <v>");
        return 2;
    };

    let bytes = match std::fs::read(&evidence_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("verify: cannot read {evidence_path}: {e}");
            return 2;
        }
    };
    let evidence: CiEvidence = match serde_json::from_slice(&bytes) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("verify: {evidence_path} is not a CI_EVIDENCE envelope: {e}");
            return 2;
        }
    };

    match ci_evidence::verify_admission(
        &evidence,
        &commit,
        &policy_version,
        REQUIRED_GATES,
        now_ms(),
    ) {
        Ok(Verdict::Admitted) => {
            println!("ADMITTED: {commit} satisfies policy {policy_version}");
            0
        }
        Ok(Verdict::Rejected) => {
            println!("REJECTED: {commit} does not satisfy policy {policy_version}");
            1
        }
        Err(reason) => {
            eprintln!("VERIFICATION ERROR: {reason}");
            2
        }
    }
}

// ---------------------------------------------------------------------------
// (registry is data; nothing else to see here)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn _assert_registry_shapes() {
    // Compile-time reminder that the gate matrix is a table, not a tangle.
    let _: fn() -> &'static [GateDef] = || GATES;
    let _: &[&str] = REQUIRED_GATES;
    let _ = GateClassification::Observational;
}
