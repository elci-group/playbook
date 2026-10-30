# Playbook Operations Runbook

## Purpose of this document

This document tells platform operators how to run Playbook: how a game is
registered, sealed, driven, coordinated, defended, and closed; what the HTTP
service exposes; what the system does automatically when a worker, provider,
or budget fails; and what the operator must do by hand because no machine may
decide it. The governing model is fixed: Playbook is a deterministic,
evidence-governed control plane. Models and workers may propose progress; only
sealed evidence establishes it. Every rule below is enforced by the fold in
`playbook-state`; the HTTP adapter can do nothing the event log refuses.

This document summarises. The contracts govern: `docs/franchise.md`,
`docs/huddles.md`, `docs/blitz.md`, `docs/live-runtime.md`, and
`docs/server-directive.md` carry the normative rules.

## Operating model at a glance

The call chain is host → franchise API → drive authority:

1. The **host** owns the process and the capability plane. It supplies
   execution through the `CapabilityPlane` trait (`playbook-elci`). The HTTP
   adapter still wires the deterministic `FixturePlane`; a live ELCI daemon is
   not required for the state machine, and live Lucid, provider, Bench, TT and
   CERS adapters remain integration seams.
2. The **franchise API** (`playbook-franchise`) is the durable management
   layer: roster, availability, objectives, salary cap, possession renewal.
   Franchise state is persisted as checksummed snapshots under
   `<journal-dir>/franchises/<franchise-id>.json`; a corrupt franchise blocks
   startup rather than discarding objectives.
3. The **drive authority** (`playbook-state`) is the official. It accepts and
   checks evidence, computes yards-to-go, enforces admissibility, classifies
   the down, and determines completion. Every event is journaled and fsynced
   before the authority commits it; `ToolInvoked` reaches durable storage
   before the capability plane executes it.

Run **one service owner per journal directory**. The service serialises
franchise operations; this is a single-host store, not a distributed locking
protocol.

### What the server exposes

Start the service with `playbook-server --port 8088`; flags are `--bind`,
`--port`, `--journal-dir DIR`, `--otlp URL`, and the opt-in
`--auto-update --update-remote URL [--update-ref REF]` pair, which runs a
Theosis update check before the port binds. On boot the server recovers
journals, truncating a torn, tampered, or out-of-sequence tail at the first
bad record. The 41 routes below are enumerated from
`crates/playbook-server/src/lib.rs` and `crates/playbook-server/src/franchise.rs`.
All bodies are JSON; unknown fields are rejected. Franchise-managed drives
cannot be accessed or mutated through the legacy `/v1/drives` routes.

| # | Method | Route | Purpose |
| --- | --- | --- | --- |
| 1 | GET | `/healthz` | Liveness; returns `{"ok": true}` |
| 2 | GET | `/readyz` | Readiness; reports hosted drive count and `rejected_journals` |
| 3 | GET | `/v1/drives` | List legacy drive ids |
| 4 | POST | `/v1/drives` | Create a legacy drive (`drive_id` required; optional `statement`, `workload_class`, `tokens`, `quota`, `policy`) |
| 5 | GET | `/v1/drives/{id}` | Official tactical snapshot |
| 6 | GET | `/v1/drives/{id}/events` | Replayable drive events |
| 7 | GET | `/v1/drives/{id}/trace` | Observability spans for the drive |
| 8 | GET | `/v1/drives/{id}/coherence` | Live coherence reading, active condition, huddle metrics |
| 9 | GET | `/v1/drives/{id}/huddles` | Active and closed huddles |
| 10 | GET | `/v1/drives/{id}/huddles/{huddle_id}` | One huddle with phase, budget, reports |
| 11 | POST | `/v1/drives/{id}/huddles/request` | Request a huddle; the machine admits or refuses with a reason code |
| 12 | POST | `/v1/drives/{id}/huddles/{huddle_id}/reports` | Submit a participant report |
| 13 | POST | `/v1/drives/{id}/huddles/{huddle_id}/collect` | Authority-driven report collection over the ELCI plane |
| 14 | POST | `/v1/drives/{id}/huddles/{huddle_id}/commit` | Commit a reconciled outcome |
| 15 | POST | `/v1/drives/{id}/huddles/{huddle_id}/escalate` | Escalate up the scope ladder |
| 16 | POST | `/v1/drives/{id}/preflight` | Capability discovery before dispatch |
| 17 | POST | `/v1/drives/{id}/steps` | Run one classified play |
| 18 | POST | `/v1/drives/{id}/pressure` | Record defensive pressure (legacy body takes `provider_quota`) |
| 19 | POST | `/v1/drives/{id}/punt` | End the possession, preserve the ledger |
| 20 | POST | `/v1/drives/{id}/reconcile` | Answer an open idempotency key with `key` and `occurred` |
| 21 | POST | `/v1/drives/{id}/complete` | Claim completion; result records whether it was accepted |
| 22 | POST | `/v1/front-office/rosters` | Construct a static `ROSTER/1` portfolio contract from `PortfolioInput` |
| 23 | GET | `/v1/franchises` | List franchise ids |
| 24 | POST | `/v1/franchises` | Create a franchise (`franchise_id`, `salary_cap`); 409 on duplicate, 503 at cap |
| 25 | GET | `/v1/franchises/{f}` | Durable franchise, derived record, and scouting |
| 26 | POST | `/v1/franchises/{f}/roster` | Acquire a roster member (`member`, `reason`) |
| 27 | POST | `/v1/franchises/{f}/roster/{p}/availability` | Set `ready`, `benched`, or `unavailable` with a reason |
| 28 | POST | `/v1/franchises/{f}/roster-contracts/stage` | Stage a `ROSTER/1` contract; no live effect |
| 29 | POST | `/v1/franchises/{f}/roster-contracts/activate` | Activate a staged contract; between contests only |
| 30 | POST | `/v1/franchises/{f}/objectives` | Declare an objective with pristine gates |
| 31 | POST | `/v1/franchises/{f}/drives` | Start a drive (`objective_id`, `drive_id`, `budget`) |
| 32 | GET | `/v1/franchises/{f}/drives/{d}` | Objective id and official tactical snapshot |
| 33 | GET | `/v1/franchises/{f}/drives/{d}/events` | Replayable drive events |
| 34 | POST | `/v1/franchises/{f}/drives/{d}/preflight` | Capability discovery |
| 35 | POST | `/v1/franchises/{f}/drives/{d}/steps` | Run one play; optional `player_id` constraint |
| 36 | POST | `/v1/franchises/{f}/drives/{d}/pressure` | Record measured defensive pressure (`cost`) |
| 37 | POST | `/v1/franchises/{f}/drives/{d}/punt` | End the possession |
| 38 | POST | `/v1/franchises/{f}/drives/{d}/receive` | Renew possession (`player_id`, `budget`); reserves against the franchise cap |
| 39 | POST | `/v1/franchises/{f}/drives/{d}/reconcile` | Reconcile an open invocation (`key`, `occurred`) |
| 40 | POST | `/v1/franchises/{f}/drives/{d}/complete` | Claim completion; response carries `completed` |
| 41 | POST | `/v1/franchises/{f}/drives/{d}/abandon` | Abandon with a reason |

Unknown drive actions return 404; wrong methods on `/v1/drives/...` return 405.
Unknown routes return 404. Every mutation is journalled before the response.

## Game lifecycle procedures

Run each game in this order. Do not skip steps; the authority refuses
out-of-order state changes by design.

1. **Register personnel and freeze the roster.** Create the franchise
   (`POST /v1/franchises`) with a salary cap in all eight budget dimensions.
   Acquire members with `POST .../roster`, giving a reason for each
   acquisition. Build the offline portfolio with
   `POST /v1/front-office/rosters`, then stage the `ROSTER/1` contract and
   activate it with `POST .../roster-contracts/activate` — activation requires
   all franchise contests to be terminal and reconciled. Membership freezes at
   kickoff and survives punts and recovery; registering a resource during a
   contest never expands that contest's eligibility. Readiness can change live
   within frozen eligibility: set `ready` on a benched substitute before
   dispatching it, because benched and unavailable players are excluded from
   routing.

2. **Declare the objective and gates — pristine declarations required.**
   `POST .../objectives` with an immutable task contract and acceptance gates
   in dependency order, at least one of them mandatory. Pre-satisfied or
   waived declarations are rejected: creating an objective cannot manufacture
   progress. The objective references one durable drive ledger; the franchise
   does not throw the ledger away when a player fails.

3. **Seal the game contract.** The `GameContract` carries the objective, win
   conditions, loss conditions, budget envelope, evidence policy, and optional
   deadline. Seal it with `GameContract::seal`: win condition ids must be
   unique and at least one must be mandatory, or the contract could never be
   won and could never lose honestly. Sealing is content-addressed; amendment
   happens only through a new sealed contract that names its predecessor, and
   an amendment that does not name the seated predecessor is a replay
   divergence and is refused.

4. **Run the drive — plays, classifications.** Start the drive with
   `POST .../drives` and a budget grant; the grant is reserved against the
   franchise cap. Preflight, then step the drive with `POST .../steps`. Each
   step classifies the situation to exactly one EVALUATING branch — the
   defence-before-coherence-before-personnel-before-possession-before-progress
   ladder: Blitz → CersRecovery → Huddle → Substitute → Lastdown → Advance —
   and the decision is journalled with its reasons. The same measurement and
   policy always classify the same down. Fourth down hands the drive to the
   Lastdown doctrine; see procedure 8.

5. **Coordinate — huddle triggers and scopes.** Request huddles with
   `POST .../huddles/request`; the machine admits only when the expected value
   of reconciliation exceeds its expected cost. Triggers, ordered most severe
   first, are: `repeated_failed_play`, `gate_unlocked_dependents`,
   `fourth_down_disposition`, and `coherence_debt`. Scopes climb the ladder
   position → unit → coordinator → team → franchise; escalation moves only up,
   only with scope justification, and `franchise` scope additionally requires
   manual authority. Every refusal carries a machine reason code
   (`scope_busy`, `unchanged_field`, `no_budget`, `scope_unjustified`,
   `expected_value`) and is journalled, so the audit log shows why
   communication did not happen. Collect reports through the authority
   (`.../collect`), accept participant submissions (`.../reports`), commit the
   reconciled outcome (`.../commit`), or escalate (`.../escalate`). A huddle
   can reassign work; it can never move gates, evidence, or the measured
   field. No committed outcome, no tactical mutation.

6. **Substitute — the five Bench classes.** A journalled substitution carries
   exactly one class: `tactical`, `performance`, `availability`, `economic`,
   or `emergency`, plus the outgoing personnel id, an optional replacement,
   and non-empty reasons. Substitution is a proposed branch of the EVALUATING
   ladder; the Bench verdict is a `BenchDecision` journal event. Replacing a
   player never replaces the objective or the official ledger. A player with
   an unreconciled busy invocation stays excluded from assignment in every
   objective until the host reconciles its key — benching or replacing it does
   not clear the uncertainty.

7. **Defend — Blitz activation and the cancellation guarantee.** Record
   defensive pressure — an outage, an imminent deadline, failing power, quota
   or network — with the `pressure` route. Under material pressure the blitz
   subsystem fans multiple independently operating workers out over one
   frozen, content-addressed target and races them. The first independently
   verified candidate wins, never the first response: candidates are untrusted
   until their required gate set passes (static invariants, compilation, tests,
   benchmarks, runtime health outrank adjudication and human review, and a
   gate only generative adjudication passes is never sufficient alone). The
   winner is atomic — exactly one `SelectedWinner` per recovered blitz — and
   cancellation is mandated, not best-effort: redundant workers are stopped
   the moment recovery is verified, and the stop's latency and avoided spend
   are recorded. A race that cannot stop its losers fails in its own named way
   (`cancellation_failure`). Waves preserve independence: wave 1 shares no
   reasoning or candidates, wave 2 receives only deterministic failure
   evidence, and wave 3 is the seam that emits a `HuddleRecommendation` — the
   blitz never opens a huddle itself, and huddles never run during wave 1.

8. **Preserve — Lastdown and CERS.** On fourth down the Lastdown doctrine
   scores and records exactly one disposition: Convert, Field Goal, Punt,
   Reset, or Abandon. `receive` renews the possession, reserves its new grant
   against the franchise cap before committing, and seals a `PLAYBOOK/1`
   continuation package that omits the athletic-amnesia keys and never carries
   provider or model metadata. Acceptance gates, task identity, and observed
   historical costs survive the transition; the possession's local budget can
   reset, but franchise allocation and spending never do. When cognition
   fails, CERS harvests it: `harvest_play` and `harvest_blitz` turn a tool
   failure or a failed blitz into a recovery artefact, `harvest_outage`
   captures a provider outage, and a `CersRecovered` event must carry at least
   one confirmed fact or failed assumption — a recovery artefact with no
   recoverable cognition is no artefact.

9. **Complete — verification and the refusal of false completion.** Claim
   completion with `POST .../complete`. The authority checks every mandatory
   win condition against measured drive state; `verified_win` is true only
   when every mandatory win condition passed. A completion request with open
   mandatory gates records a rejected claim, returns `completed: false`, and
   increments the drive's false-completion counter. Do not retry the claim
   against an unchanged field; produce evidence, close the gates, then claim.
   Before closing any drive, reconcile open invocations: the host must
   reconcile uncertain outcomes before further execution or closing the drive.
   Terminal drive traces can be exported to an OTLP endpoint via `--otlp`.

## Failure playbooks

Each row: what you observe, what the system does without you, what remains
yours.

- **Worker timeout.** Symptom: a dispatched participant produces no report or
  outcome in its envelope. Automatic: in huddle collection, a timed-out
  participant simply leaves no report; the pass continues with the rest and
  `CollectionSummary { collected, failed, synthesized }` records the outcome.
  A `ToolInvoked` invocation stays durably recorded, so a disappeared worker's
  execution is reconstructed on reopening. Operator action: confirm the worker
  is replaced or benched, then reconcile its idempotency key (below) —
  benching it does not clear the uncertainty.

- **Provider outage.** Symptom: upstream inference failures. Automatic: the
  outage is a defensive pressure input; blitz eligibility opens and the race
  purchases verified-recovery latency under policy envelopes. `harvest_outage`
  preserves the outage as a CERS artefact. Operator action: record pressure if
  the monitor did not (`POST .../pressure` with the measured cost), let the
  race close, and read the blitz closure; only a failed independence attempt
  escalates into a huddle.

- **Unresolved side effect (reconcile before re-execution).** Symptom:
  `open_execution` is set and further plays are refused. Automatic: the drive
  records the refusal, excludes the busy player from all assignments, and
  keeps its original grant reserved; an unknown result is never treated as a
  measured zero-cost success. Operator action: determine the truth and answer
  `POST .../reconcile` with an explicit `occurred` boolean. A key reconciled
  as not occurred may be invoked again; a key reconciled as occurred is
  consumed. If the execution occurred but its cost was lost, the franchise
  atomically charges the remaining possession envelope and reports it as
  `unmeasured_spend_reserved`.

- **Coherence debt rising (huddle is automatic).** Symptom: the
  `/coherence` reading climbs. Automatic: debt is a deterministic 0..=1000
  integer with bands at 200 (monitor), 400 (huddle-eligible), 600
  (coordinator-recommended) and 800 (mandatory reconciliation); at the
  mandatory band the economics are bypassed and reconciliation is forced.
  Operator action: none to admit it — read the committed outcome, and watch
  `huddle_escalations`: escalation beyond `coordinator` scope requires manual
  authority, which is your signature.

- **Budget exhaustion (Lastdown decision).** Symptom: remaining budget
  approaches zero; huddle and blitz admissions begin refusing with
  `no_budget` and `budget_exhausted`. Automatic: budget never grows inside a
  possession; exhaustion forces synthesis from what is already collected, and
  fourth down hands the drive to Lastdown, which scores Convert, Field Goal,
  Punt, Reset, or Abandon from measured state. Operator action: punt or
  receive deliberately — a renewed possession needs a fresh grant from
  unallocated franchise capacity, and lifetime grants can never exceed the cap
  (there is no automatic refund in this implementation). Further dispatch is
  refused once observed franchise spending exceeds the cap.

- **Failed Blitz race (exhaustion classes and the huddle escalation).**
  Symptom: a blitz closes `Exhausted` or `Failed` with a named class —
  `no_candidate`, `invalid_candidates`, `verification_failure`,
  `deadline_exhausted`, `budget_exhausted`, `provider_failure`, `tool_failure`,
  `target_ambiguity`, `target_drift`, `correlated_failure`,
  `cancellation_failure`, `recovery_regression`, or `policy_rejection`.
  Automatic: spend and scouting survive in the ledger; if the target itself
  changed the race is marked `Superseded` and a new blitz opens against the
  new hash; when independence failed, the blitz emits a `HuddleRecommendation`
  with scope key, reason code, evidence references, and the correlated failure
  count. Operator action: map the recommendation onto an actual huddle request
  yourself — the blitz never opens one — and remediate by class: correlated
  failures need a failure-domain fix, `verification_failure` needs verifier
  capacity, `target_ambiguity` means the situation could not be sealed at all.

- **Ambiguous effect occurrence (manual reconciliation required).** Symptom:
  an invocation's side effect cannot be proven either way after a crash or
  dispatch loss. Automatic: nothing executes until you decide; omitted
  information is never treated as proof that an effect did not occur, and no
  timeout, retry, or bench decision settles it. Operator action: check the
  external system the plane called, then answer the open key with
  `POST .../reconcile` and the truth. This is the one decision the machine
  refuses to make for you, because guessing in either direction would let a
  side effect happen twice or erase a real one.

## Observability

**Tactical snapshot.** `GET /v1/drives/{id}` (and its franchise twin) returns
the official snapshot: drive id, current down, presentation label, current
play and personnel, yards-to-go in milli and its delta, budget remaining in
ppm plus the consumed budget, mandatory and blocked gates, active capability,
last play result, completion probability, failure class, drive status, and the
authority's explanation lines. This is the Phase 7 seam — what an orchestrator
consumes. For a text rendering of the same state, run
`playbook-cli -- status` or `-- explain`.

**Span hierarchy.** One drive is a trace: a root `drive` span, one span per
play (the selected operation) beneath it, tool spans beneath the play, and
huddle spans beside them; ids are derived deterministically from the logical
ids. The hierarchy is franchise → objective → drive → play → tool, with the
drive span as the trace root the exporter emits. `GET
/v1/drives/{id}/trace` returns the spans as JSON. `playbook-otlp` encodes the
same trace as OTLP/HTTP JSON and POSTs it to the URL given with `--otlp`
(conventionally `http://127.0.0.1:4318/v1/traces`); it has no opentelemetry
dependency and speaks plain HTTP/1.1. The CLI's `otlp` command exports a
journal's trace on demand.

**Metrics by name.** Drive-level: `drive_completion_rate`,
`mean_budget_utilisation`, `ytg_reduction_per_cost`, `plays_per_drive`,
`punt_rate`, `field_goal_rate`, `fourth_down_conversion_rate`,
`tool_failure_rate`, `capability_acquisition_rate`, `model_handoff_rate`,
`gate_reopen_rate`, `false_completion_rate`. Huddle-level:
`huddles_total`, `huddles_by_scope` (one `*_huddles_total` key per scope),
`huddles_rejected_total`, `huddle_tokens_total`, `huddle_cost_total`,
`huddle_duration_ms`, `huddle_participants`, `huddle_escalations`,
`huddle_contradictions_found`, `huddle_contradictions_resolved`,
`coherence_debt_before`, `coherence_debt_after`, `ytg_velocity_pre_huddle`,
`ytg_velocity_post_huddle`, `rework_avoided_estimate`. Refused or replayed
huddle events cannot inflate the counters: metrics fold only from survived
admissions. Blitz races additionally report a recovery premium — race cost
over recovery seconds — and per-worker scouting rows fold from the ledger.

**Audit recovery.** `playbook recover <journal>` folds a recovered journal in
audit mode and reports the open invocation and unresolved effects the host
still owes. Check `/readyz` after boot: a non-zero `rejected_journals` count
means recovery truncated something — investigate before dispatching work.

## Cross-references

- `docs/franchise.md` — the franchise contract: HTTP API, budget semantics,
  operating limits, durability.
- `docs/front-office.md` — `ROSTER/1` construction, restricted inputs, and
  scoring.
- `docs/huddles.md` — huddle scopes, triggers, admission economics, the HTTP
  surface, and invariants.
- `docs/blitz.md` — pressure eligibility, target freeze, envelopes, the race,
  verification gates, waves, and the failure taxonomy.
- `docs/live-runtime.md` — durable dispatch, reconciliation, the six-state
  cancellation answer, and the fault table.
- `docs/server-directive.md` — the contract of record for `playbook-server`.
- `docs/anthology/01-system-architecture.md` — how the crates compose.
- `docs/anthology/03-evidence-journal-replay.md` — the journal and audit
  guarantees this runbook relies on.
- `README.md` — run commands, delivery status, and the invariants the tests
  hold.
