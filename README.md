# Playbook

Playbook is Lucid's tactical control plane. It classifies the state of an agentic work cycle and selects the next admissible intervention.

The football words are presentation metadata. The machine state is:

```text
S_t = (Y_t, ΔY_t, B_t, G_t, A_t, T_t, C_t, P_t, E_t)
```

Models may propose progress. Only evidence establishes it. Yards-to-go is the weighted, evidence-backed distance across deliver gates. Budget is multidimensional. Fourth down hands the drive to the Lastdown doctrine: convert, field goal, punt, reset, or abandon.

Lucid remains the orchestrator. Playbook issues an execution contract. ELCI is the capability plane. This repository does not call a model.

## Layout

```text
crates/     Rust control plane
tov/        replaceable adapters, checked with `tov check --json`
schemas/    draft-07 contracts
fixtures/   versioned policy bundles
```

`playbook-core` holds the nouns. `playbook-state` folds the event log. `playbook-policy` is data. `playbook-elci` is the `CapabilityPlane` trait plus the deterministic fixture used by tests. `playbook-lastdown` scores fourth-down dispositions and seals a `PLAYBOOK/1` continuation package. That package is not a `LASTDOWN/1` blob.

## Run

```bash
cargo test
cargo run -p playbook-cli -- mvp
cargo run -p playbook-cli -- sim --trace mvp --policy playbook-policy-pass-heavy
cargo run -p playbook-cli -- explain
cargo run -p playbook-cli -- status
cargo run -p playbook-cli -- sprint
```

`playbook sprint` writes the acceptance drive's event log as JSON. The checked-in
`fixtures/sprint_transcript.json` is replayed by the simulation tests. Applications
can persist logs with `playbook_state::export_log`, load them with `import_log`,
and recover state with `fold` in audit mode.

`playbook priors` prints the historical prior table the acceptance drive learned:
one row per personnel id, holding the number of measured outcomes and the summed
realized yards-to-go reduction.

The `sim` report includes completion status, the yards-to-go trajectory, remaining
budget, tool failures, model calls, personnel changes, the first fourth-down
index, elapsed budgeted time, monetary cost, satisfied gates, and integer drive
efficiency. Efficiency measures progress from the trajectory's maximum
yards-to-go to its final value, so the initial event before gate declaration
does not hide the work completed. Budget consumption compares the first and
last trajectory points across the complete drive, including renewed possessions.

`playbook mvp` runs the acceptance drive: gate decomposition, capability discovery, first down, evidence-backed yardage, second down gain, third down pass, an induced tool failure, a refused blind retry, budget pressure, fourth down, a punt, a fresh possession with an independent yards estimate, completion, and an audit replay. The replayed state equals the live state.

## Delivery status

Phases 0 through 6 are in this tree: schemas, the deterministic core, the ELCI envelope, play admissibility, Lastdown dispositions, isolated Tov adapters, and trace replay with failure injection.

Phase 7 is the contract seam. `PlayExecution`, `PlayResult`, and `TacticalSnapshot` are what Lucid would consume. This repository does not edit Lucid.

Phase 8 is in this tree: the historical prior table learns from deterministic
ground truth. When a play finishes, the fold attributes the evidence-backed
yards-to-go drop that happened while it ran to the executing personnel id; a
failed play records a measured zero. `HistoricalPriorTable` holds those rows and
`fold` reproduces them exactly on replay, in audit and resimulate modes. Self
reports (`own_ytg_milli`) never enter the table. Once a row holds at least
`routing.prior_min_samples` outcomes, routing scores the personnel by its
measured average residual instead of its self-assessed estimate; below the
threshold the self-report still fills the gap.

Phase 9 is in this tree: the history crosses drives. The table lives in
`playbook-core`, a `PriorsLoaded` event seeds a later drive with it through
`Authority::load_priors`, and `export_priors`/`import_priors` persist it as a
versioned bundle checked against `schemas/prior_table.json`. Because the load
is an event, replays reproduce it exactly, and the drive keeps learning on top
of the imported rows. Two drives with identical rosters route differently when
one carries the measured record.

Observability emits an OpenTelemetry-shaped trace: one drive, play spans, tool spans, and the metric names from the directive. An OTLP exporter is not wired.

Tov modules are compile-checked, versioned as source, and failure-isolated. If a module is missing or fails, the Rust normaliser still scores the evidence. The Windows control plane is the Rust crates.

Run `deliver --spec deliver.toml --strict` for formatting, Clippy, Rust tests,
and Tov checks and tests. Run `uni --fail-under 80 .` for the graded analysis
snapshot.

The capability plane in tests is `FixturePlane`. A live ELCI daemon is not required for the state machine.

## Invariants the tests hold

Evidence-free claims leave yards-to-go unchanged. Budget remaining does not grow inside a possession. A mandatory failed gate cannot complete the drive. The same measurement and policy classify as the same down. Fourth down records a Lastdown disposition. A tool failure cannot be stored as a tool success. Replay reproduces the state. A tampered witness is a replay divergence. An identical failed play is rejected until the field changes. A waiver needs an authority and a reason. Budget pressure does not authorize deployment. The continuation package omits the athletic-amnesia keys. Priors learn only from measured outcomes, and a personnel's self-report stops moving its routing score once its measured record reaches the policy threshold. A loaded prior table is part of the log, routes by measured record over identical self-reports, and never carries model identity or rankings.
