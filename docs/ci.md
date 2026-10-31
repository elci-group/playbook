# The CI Contract: Evidence, Not Opinion

## Point

Playbook's CI is not a build script with opinions. It is a federation of
specialist authorities whose observations converge on one canonical
`CI_EVIDENCE/1` envelope, from which the verdict is mechanically derived.
No single tool is a universal judge; no prose overrides the evidence
(§16); and a change to the judge is not an ordinary change (§34).

## Evidence

The runner is `crates/playbook-ci` (`playbook-ci` binary, three
subcommands). The envelope contract lives in `crates/ci-evidence` and is
the single source of truth for schema, outcomes, waivers and admission.

```
                 +------------------------------------------+
                 |  playbook-ci freeze --policy-version v1  |
                 |  commit / parent / branch / diff digest  |
                 |  Cargo.lock digest / toolchain / config  |
                 +------------------------+-----------------+
                                          | RevisionFreeze
                                          v
  +------------------+   +------------------+   +------------------+
  | cargo fmt --check|   | cargo check      |   | cargo clippy -D  |
  |   ci/fmt         |   |   ci/check       |   |   ci/clippy      |
  +--------+---------+   +--------+---------+   +--------+---------+
           |                      |                      |
  +--------v---------+   +--------v---------+   +--------v---------+
  | cargo test       |   | deliver --strict |   | replay gates     |
  |   ci/test        |   |   ci/deliver     |   | ci/replay,       |
  |                  |   |                  |   | ci/mvp-replay,   |
  |                  |   |                  |   | ci/value-lab     |
  +--------+---------+   +--------+---------+   +--------+---------+
           |                      |                      |
           +----------------------+----------------------+
                                  |  GateRecord each
                                  v
                 +------------------------------------------+
                 | playbook-ci run --freeze freeze.json     |
                 | executes matrix -> CI_EVIDENCE.json      |
                 +------------------------+-----------------+
                                          v
                 +------------------------------------------+
                 | playbook-ci verify --commit ...          |
                 | verify_admission: identity, mandatory    |
                 | gates, mechanical re-derivation (§17)    |
                 +------------------------+-----------------+
                                          v
                              ADMITTED or REJECTED
                              (Kaptaind admits, §36)
```

### Gate classifications (§4)

| Class | Meaning | Failure effect |
| --- | --- | --- |
| `blocking` | Must pass. | Rejection, waiver is the only rescue. |
| `conditional` | Blocks only when policy declares it required. | Rejection only if required and not waived. |
| `advisory` | One voice in the federation. | Never independently blocks. |
| `observational` | Measurement (§4). | Never admission-relevant. |

### Failure semantics (§30) — closed, six outcomes

`PASS`, `FAIL`, `ERROR`, `SKIPPED_NOT_APPLICABLE`, `SKIPPED_AUTHORISED`,
`INCONCLUSIVE`. There is no silent collapse between them:

- `ERROR` never equals `PASS`. A gate that fails to spawn is `ERROR`
  (fail closed) — the runner records `gate/spawn` and the envelope
  rejects.
- `INCONCLUSIVE` never equals `PASS`.
- Only `PASS` satisfies a gate (`.satisfies()`).
- Only `PASS`/`FAIL`/`ERROR` count as executed (`.executed()`). A
  **skipped mandatory gate is rejection**, not a dodge — skipping is
  recorded, never silent.

### The §14 Playbook-specific determinism gates

| Gate | Class | Authority | Command | Status |
| --- | --- | --- | --- | --- |
| `ci/fmt` | blocking | cargo | `cargo fmt --all --check` | **wired** |
| `ci/check` | blocking | cargo | `cargo check --workspace --all-targets --offline` | **wired** |
| `ci/clippy` | blocking | cargo | `cargo clippy --workspace --all-targets --offline -- -D warnings` | **wired** |
| `ci/test` | blocking | cargo | `cargo test --workspace --offline` | **wired** |
| `ci/deliver` | blocking | deliver | `deliver --spec deliver.toml --strict --progress never` | **wired** |
| `ci/replay` | blocking | cargo | `cargo test -p playbook-state --offline` | **wired** |
| `ci/mvp-replay` | blocking | playbook | `target/debug/playbook mvp --basic` (binary built first) | **wired** |
| `ci/value-lab` | blocking | cargo | `cargo test -p value-lab --offline` | **wired** |
| `ci/traci` | advisory | traci | `traci check . --format human` (if binary present, else skip) | **wired** |
| `ci/amber` | conditional | amber | dependency-surface gate: runs only if `Cargo.toml`/`Cargo.lock` changed vs parent | **wired (placeholder command)** |
| `ci/verity` | conditional | verity | claims-surface gate: runs only if `README.md`/`docs/` changed vs parent; `true` placeholder with finding `claim audit pending CLAIMS/1` | **pending real audit** |

Verification (§17) requires execution of:
`ci/fmt`, `ci/check`, `ci/clippy`, `ci/test`, `ci/deliver`, `ci/replay`.

### Anti-gaming rules (§29) and waivers

- Evidence is revision-bound: a freeze pins commit, parent, diff digest,
  lock digest, toolchain, policy version and config hashes. Evidence
  generated against revision A never authorises revision B — `verify`
  rejects on commit mismatch before any gate is inspected.
- The command string is hashed into `input_hash`; what ran is what is
  attested. Editing the matrix is visible in review, because the matrix
  is a data table in `src/registry.rs`, not scattered code paths.
- A waiver is **evidence**, not an override: it records `gate_id`,
  `reason`, `author`, `expires_at_ms`, and the revision it was recorded
  against. An expired waiver does not rescue a Fail. An authorised
  waiver does rescue it — but the gate is still recorded as
  failed-with-waiver, never rewritten into a pass.
- The verdict is re-derived mechanically at verification time; a recorded
  verdict field would be a lie waiting to happen, so there isn't one.

### §34: a change to the judge is not an ordinary change

`playbook-ci` is part of the machinery that judges everything else.
Editing the gate matrix, the failure semantics, the freeze construction
or the required-gate list is a constitutional change: it invalidates the
comparison between old and new evidence and demands explicit review of
the whole pipeline, bumping the implementation version frozen into every
envelope. Treat diffs to `crates/playbook-ci` and `crates/ci-evidence`
with the suspicion they have earned.

## Visualisation

Think of admission as a courtroom, not a checkpoint. The freeze is the
chain of custody. Each authority testifies into one `GateRecord`; the
envelope is the transcript; `derive_verdict` is the statute applied
without a jury of vibes. A waiver is a signed pardon with an expiry date
stapled to it. Kaptaind is the judge who reads the transcript — and a
judge who wrote the transcript would be recused (§34).

## Evaluation

- `playbook-ci freeze|run|verify` implement the full loop; the envelope
  parses as `CI_EVIDENCE/1` and rejects/admits per the closed semantics.
- The §33 adversarial campaign (`tests/adversarial.rs`) demonstrates
  rejection of: blocking `FAIL`, blocking `ERROR`, blocking
  `INCONCLUSIVE`, evidence from the wrong commit, a missing mandatory
  gate, an expired waiver, and a skipped mandatory gate; and acceptance
  of a fully-passing envelope, a waiver-rescued Fail, and a failed
  advisory gate that correctly does not block.
- Pending: `ci/verity`'s real claim audit (CLAIMS/1), and `ci/amber`'s
  full dependency audit behind its placeholder command.
