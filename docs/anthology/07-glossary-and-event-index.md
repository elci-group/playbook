# Playbook Glossary and Event Index

## Purpose of this document

Playbook is a deterministic, evidence-governed organisational control plane written
in Rust, and it uses football vocabulary as its institutional language. Every
football word in this system pairs a familiar name with one precise computational
meaning; the football words are presentation metadata, and the machine state is a
fold of the journal event log. This document is the reference for all readers: it
defines every load-bearing term, indexes every journal event the drive authority
can emit, and states the rule that decides when a football concept is allowed into
the system at all.

This document summarises; the contracts govern. Where this glossary and a contract
document disagree, the contract wins and this glossary should be corrected.

## Glossary

| Term | Computational meaning | Where it lives |
| --- | --- | --- |
| Franchise | The persistent management layer that owns objectives, roster, resource allocations, management decisions and measured history across drives, while players remain disposable executors of bounded assignments. | `crates/playbook-franchise` |
| Objective | An immutable task contract plus acceptance gates referencing one durable drive ledger; many independent objectives coexist in one franchise and outlive any player. | `crates/playbook-franchise` |
| Game / GameContract | The sealed, content-addressed contract for one game — objective, win conditions, loss conditions, budget envelope and evidence policy — amendable only by a new sealed contract naming its predecessor. | `crates/playbook-core/src/contract.rs` |
| Roster | The franchise inventory of registered execution resources, each with a personnel id, position label, optional provider metadata, and an availability of `ready`, `benched` or `unavailable`. | `crates/playbook-franchise` |
| Frozen roster | The per-drive membership captured at kickoff inside the drive's contract; only frozen members can play, and registration during a contest never expands that contest's eligibility. | `crates/playbook-franchise` |
| Front Office | The offline roster portfolio builder that works only from closed-season history, economic constraints and abstract workload features, emitting a static `ROSTER/1` contract staged until an explicit transaction window; bench readiness and deployment stay live coaching decisions. | `crates/playbook-front-office` |
| Drive | One bounded attempt on an objective whose entire state is the fold of its journal event log; snapshots are caches, the log is the truth. | `crates/playbook-state`, `crates/playbook-journal` |
| Drive state `S_t` | The tuple `(Y_t, ΔY_t, B_t, G_t, A_t, T_t, C_t, P_t, E_t)` — yards-to-go, its delta, budget, gates, assessments, tactics, coherence, personnel and evidence — reproduced exactly by replay. | `crates/playbook-state` |
| Down | One of seven deterministic classifications of a drive's field position: `FirstDown`, `SecondDownLoss`, `SecondDownGain`, `ThirdDownRush`, `ThirdDownPass`, `ThirdDownPlayAction`, `FourthDown`; the same measurement and policy always classify the same down. | `crates/playbook-core/src/domain.rs` (`Down`) |
| YTG (yards-to-go) | The weighted, evidence-backed distance in milli-units across deliver gates; evidence-free claims leave it unchanged by construction. | `crates/playbook-ytg` |
| Possession | A budgeted renewal of the drive with a fresh grant reserved against the franchise salary cap; budget remaining never grows inside a possession, and renewal preserves gates, task identity and measured history. | `crates/playbook-franchise`, `crates/playbook-lastdown` |
| Play | A bounded, admissible assignment of one operation to one personnel against one declared capability, with an expected evidence class, a budget ceiling and a side-effect class. | `crates/playbook-core/src/domain.rs` (`PlayDefinition`, `PlayExecution`) |
| PlayKind classes | The seventeen play kinds: survey and discovery (`InspectEnvironment`, `DiscoverRepository`, `DiscoverCapabilities`), decomposition (`DecomposeTask`), validation (`BaselineTest`, `RunDeterministicTest`, `ParallelValidation`), exploitation (`ExploitProvenTool`, `ReplaceInferenceWithTool`), routing (`RouteSpecialist`, `TargetedInference`, `NarrowSearch`), acquisition (`AcquireCapability`), recovery (`RetryCorrected`, `SubstitutePersonnel`), and fourth-down extraction (`FieldGoalExtract`, `PuntPossession`). | `crates/playbook-core/src/domain.rs` (`PlayKind`) |
| Personnel | A capability-classed executor record — cost, latency, measured fit, reliability, quota and evidence quality — whose self-reported distance estimate never outranks its measured record. | `crates/playbook-core/src/domain.rs` (`Personnel`) |
| Personnel kinds | The six capability classes `Model`, `Tool`, `Toolchain`, `SpecialistAgent`, `DeterministicProcedure`, `HumanEscalation`, tiered so deterministic capability outranks model prestige. | `crates/playbook-core/src/domain.rs` (`PersonnelKind`) |
| Drive budget | The eight multidimensional budget ceilings — tokens, monetary micros, wall-clock milliseconds, tool calls, inference calls, retries, context capacity, provider quota — every dimension checked independently. | `crates/playbook-core/src/domain.rs` (`DriveBudget`) |
| Salary cap | The franchise-level cap over the same eight dimensions: lifetime possession grants can never exceed it, failed reservations change nothing, and grants stay reserved after completion or abandonment. | `crates/playbook-franchise` |
| Bench | The deterministic substitution service: an ordered ladder of five classes read from measured fields, where inference never sits on the ladder and a rung fires only when an eligible replacement exists. | `crates/playbook-franchise/src/bench.rs` |
| Five substitution classes | The bench ladder, most urgent first: `Emergency` (declared emergency or provider health floor), `Availability` (starter or provider unavailable), `Performance` (repeated gate failures), `Economic` (half the money spent and a cheaper qualified player exists), `Tactical` (fit below the play's requirement). | `crates/playbook-franchise/src/bench.rs` (`SubstitutionClass`) |
| Huddle | A budgeted, policy-admitted synchronization barrier admitted only when the expected cost of team-state divergence exceeds the expected cost of reconciliation; it may reassign work but may never move gates, evidence or the measured field. | `crates/playbook-huddle`, `docs/huddles.md` |
| Huddle scopes | The five escalation ranks `position` → `unit` → `coordinator` → `team` → `franchise`, climbed only upward and only with scope justification, each with its own budget envelope. | `crates/playbook-huddle/src/domain.rs` (`HuddleScope`) |
| Coherence debt | A deterministic integer 0..=1000 — a weighted sum of seven clamped divergence components (assumption divergence, dependency staleness, contradiction load, personnel turnover, cross-unit change, unresolved blockers, context age) — with monitor/eligible/coordinator/mandatory bands at 200/400/600/800. | `crates/playbook-huddle`, `policy.huddle` |
| Sugar huddle | The minimal huddle access class (`HuddleAccess::Sugar`) that discloses only changed facts, assignment, constraints and termination — synchronization reduced to a controlled whisper. | `crates/playbook-tactics/src/domain.rs` (`HuddleAccess`) |
| No-huddle | The regime that admits no synchronization at all — the control baseline in the huddle study, which wins on trivially independent work and never completes on divergence-prone work. | `crates/playbook-sim` (`huddle_study`) |
| Contradiction | An explicit huddle object with a subject, conflicting claims and a severity of `low`/`medium`/`high`/`critical`, resolved only by evidence, supersession, named authority, deferral — or left `unresolved_blocking`, which forbids unsafe progression; consensus never satisfies a gate. | `crates/playbook-huddle` |
| Blitz | Defensive parallel single-focus inference: under measured defensive pressure, multiple independent workers race on one frozen, content-addressed target, the first independently verified candidate wins, and redundant inference is cancelled the moment recovery is verified. | `crates/playbook-blitz`, `docs/blitz.md` |
| Blitz trigger classes | The two admission classes: `instinctive` — deterministic policy fires a named condition with no inference in the loop — and `strategic` — inference proposes urgency and policy authorises above score floors. | `crates/playbook-blitz` |
| Blitz waves | The wave ladder that preserves independence before buying coherence: wave 1 attacks in isolation, wave 2 receives only deterministic failure evidence, wave 3 emits a huddle recommendation through the seam; huddles never run during wave 1. | `crates/playbook-blitz` |
| Corroboration | The policy mode deciding when a verified candidate may win: `first_pass`, `two_independent_passes`, `n_of_m`, `heterogeneous_confirmation`, `deterministic_only`, or `human_confirmation`; the first response is never automatically the winner. | `crates/playbook-blitz` (`BlitzCorroboration`) |
| Effective parallelism | The count of independent attempts, not dispatched workers — eight correlated workers are roughly 3.7 attempts — and admission refuses a formation below the `min_effective_parallelism_milli` floor. | `crates/playbook-blitz` |
| Manmark | Problem-domain ownership: every persistent problem domain has exactly one defender — a mark recording owner, severity, a deterministic inspection trigger and a remediation class. | `crates/playbook-franchise/src/manmark.rs` |
| Instinctive / strategic remediation | The two defences of a mark: `instinctive` — the host runs a fixed deterministic procedure on breach, consuming no inference — versus `strategic` — a named personnel plays the coverage and escalates through the blitz seam at high severity. | `crates/playbook-franchise/src/manmark.rs` (`Remediation`) |
| Graduation | The strategy-to-instinct path: after enough consecutive successful strategic remediations, the mark's remediation is demoted to an extracted deterministic procedure. | `crates/playbook-franchise/src/manmark.rs` (`ManmarkRegistry::graduate`) |
| CERS | Cognitive exhaust recovery: every materially failed play, closed blitz and provider outage yields a structured, content-addressed recovery artefact of measured facts, disproved assumptions and surviving partial work — residue over narrative, never a claim of success. | `crates/playbook-cers` |
| Lastdown | The fourth-down doctrine that scores the five dispositions `Convert`, `FieldGoal`, `Punt`, `Reset`, `Abandon` against utility, probability and banking, and seals the continuation when the drive changes hands. | `crates/playbook-lastdown` |
| Punt / possession preservation | The disposition that hands possession to a receiver with a sealed `PLAYBOOK/1` continuation package: acceptance gates, task identity and observed history survive; the possession's local budget may reset while franchise allocation never does. | `crates/playbook-lastdown`, `crates/playbook-franchise` |
| Continuation package | The sealed, content-addressed `PLAYBOOK/1` handoff carrying measured field state, validated artefacts, open gates and evidence references, which structurally omits the athletic-amnesia keys (model identity, self-assessed confidence, competitive rankings). | `crates/playbook-core/src/domain.rs` (`ContinuationPackage`) |
| Tactic | A `TacticalDirective` of one of four classes — formation, play (diagnostic/sequencing), adjustment, doctrine — with a deterministic trigger, personnel shape, information topology, budget, termination condition and a declared counter; the conflict matrix over tactics is total, symmetric and fails closed. | `crates/playbook-tactics`, `docs/tactics.md` |
| Spread | Formation of broad parallel ownership: many concurrent lanes on distinct domains, one sole owner each, minimal context overlap (≤ 100 ppm). | `crates/playbook-tactics/src/formation.rs` |
| Heavy (Max Protect) | Formation of concentration: one primary executor wrapped in deterministic support, reserves and verification — for danger or the endgame. | `crates/playbook-tactics/src/formation.rs` |
| Trips | Formation of concentrated parallel responsibility: three personnel on one dominant domain, the remainder covering globally. | `crates/playbook-tactics/src/formation.rs` |
| Bunch | Formation of concentrated cooperative interaction: a tight micro-team sharing one context (overlap ≥ 800 ppm) under full huddle access. | `crates/playbook-tactics/src/formation.rs` |
| Empty | Formation of total commitment: every deployed hand executes and zero reserves are held; its natural counter is Max Protect. | `crates/playbook-tactics/src/formation.rs` |
| Nickel | Formation of specialist substitution: one generalist executor swapped for one specialist reviewer as risk rises. | `crates/playbook-tactics/src/formation.rs` |
| Dime | Formation of maximum specialist review density: several specialist reviewers around a reduced execution core. | `crates/playbook-tactics/src/formation.rs` |
| Bear | Formation of deterministic controls concentrated over one central, tightly coupled shared core — and nothing else. | `crates/playbook-tactics/src/formation.rs` |
| Two-High | Formation of divergent hedging: two independent reserves held against two different threat classes. | `crates/playbook-tactics/src/formation.rs` |
| Motion | Pre-dispatch adjustment: personnel probe the field before the snap, revealing measured context while changing nothing they inspect. | `crates/playbook-tactics/src/adjustment.rs` |
| Shift | Pre-dispatch adjustment: responsibility moves across a constant roster without substituting personnel. | `crates/playbook-tactics/src/adjustment.rs` |
| Audible | Pre-dispatch adjustment: the plan is replaced before irreversible execution only — a post-dispatch audible is structurally refused. | `crates/playbook-tactics/src/adjustment.rs` |
| Check-With-Me | Pre-dispatch adjustment: deterministic selection among prepared alternatives at the line, never improvisation. | `crates/playbook-tactics/src/adjustment.rs` |
| Mesh | Diagnostic pattern of crossed-domain traversal: two agents cross through the same reconciliation domain from opposite sides, and the crossing point is the reconciliation target. | `crates/playbook-tactics/src/diagnostic.rs` |
| Flood | Diagnostic pattern of depth saturation: agents are assigned to consecutive abstraction levels of one domain, coverage running shallow-to-deep in assignment order. | `crates/playbook-tactics/src/diagnostic.rs` |
| Levels | Diagnostic pattern of progressive granularity: the same agents return round after round at progressively deeper granularity, walking the depth ladder. | `crates/playbook-tactics/src/diagnostic.rs` |
| Spy | Surveillance tactic: an observe-only watcher with a drift band whose intervention envelope stops at recommendation. | `crates/playbook-tactics/src/surveillance.rs` |
| Bracket | Surveillance tactic: two different controls over one critical domain, so no single observer's blindness is fatal. | `crates/playbook-tactics/src/surveillance.rs` |
| Banjo | Exchange tactic: a joint ownership pair with deterministic switching rules over who covers which domain. | `crates/playbook-tactics/src/exchange.rs` |
| Scrape Exchange | Exchange tactic: an explicit, journalled responsibility handoff that rejects a stale owner taking back a domain it no longer holds. | `crates/playbook-tactics/src/exchange.rs` |
| Prevent | Conservative doctrine: classify the risk state and block large unverified changes, accepting slower progress for bounded downside. | `crates/playbook-tactics/src/conservative.rs` |
| Make Them Drive | Conservative doctrine: stage only small validated changes and force any regressions to be re-earned step by step rather than gambling on one large advance. | `crates/playbook-tactics/src/conservative.rs` |
| Evidence | A sealed observation whose id is the SHA-256 digest of its canonical class, body and timestamp; a tampered witness is a replay divergence. | `crates/playbook-core/src/domain.rs` (`Evidence`) |
| Evidence classes | The eleven classes (`CompilerResult`, `TestReport`, `ArtefactValidation`, `ToolInvocation`, `Benchmark`, `StaticAnalysis`, `RepositoryMutation`, `GateTransition`, `CapabilitySnapshot`, `EnvironmentSnapshot`, `NarrativeClaim`), of which `NarrativeClaim`, `GateTransition` and `CapabilitySnapshot` can never satisfy a gate. | `crates/playbook-core/src/domain.rs` (`EvidenceClass`) |
| Gate | A weighted, dependency-ordered acceptance criterion declaring the evidence classes it requires; a mandatory failed gate cannot complete the drive. | `crates/playbook-core/src/domain.rs` (`Gate`) |
| Waiver | A gate closed by fiat instead of evidence, requiring a named authority and a reason — an empty one is refused by construction. | `crates/playbook-core/src/domain.rs` (`Waiver`) |
| Journal | The durable write-ahead store (`playbook-journal`) that fsyncs every committed event to a checksummed log before the authority commits it; recovery verifies digests and sequence numbers and truncates at the first torn, tampered or out-of-sequence record. | `crates/playbook-journal` |
| ExecutionEnvelope | The complete authority-cleared wrapper for one live invocation — execution id, allowed operations and side effects, a budget envelope that may narrow but never widen the play ceiling, timeout, working directory and a mandatory idempotency key. | `crates/playbook-live/src/envelope.rs` |
| CapabilityPlane | The execution seam (`playbook-elci`) hosts implement to supply real work; tests use the deterministic `FixturePlane`, live execution uses `ToolPlane` and `ProviderPlane`. | `crates/playbook-elci`, `crates/playbook-live` |
| FaultPlane | The adversarial scripting plane that replays the directive's full fault list over any `CapabilityPlane`, each fault with a documented state transition. | `crates/playbook-live/src/faults.rs` |
| TT (Thinker/Tinker) | The directive discipline where a Thinker's specification is sealed content-addressably and a different personnel, the Tinker, implements it without silent reinterpretation; an invalid premise returns control to Playbook — the Tinker never rewrites the strategy. | `crates/playbook-core/src/domain.rs` (`TtDirective`, `TinkerReport`) |
| Evaluation branch | The one named branch per evaluation — Blitz → CersRecovery → Huddle → Substitute → Lastdown → Advance — decided defence before coherence before personnel before possession before progress. | `crates/playbook-core/src/domain.rs` (`EvaluationBranch`) |
| Scouting | Measured history crossing drives: the `HistoricalPriorTable` of per-personnel outcomes and realized yards-to-go reduction, imported as an event, never carrying model identity or rankings. | `crates/playbook-core`, `crates/playbook-franchise` |

## The journal event index

Every drive event is a variant of `DriveEvent` in `crates/playbook-events/src/lib.rs`,
serialized with a snake_case `type` tag and named by `DriveEvent::name()`. The enum
holds exactly **80 variants**, indexed below in full. The journal is the only state:
the fold in `crates/playbook-state` reconstructs the entire drive from these events
alone, and nothing is a side channel.

### Drive lifecycle and field state (8 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `drive_started` | Drive id, task contract, initial budget, timestamp. | Core authority |
| `down_changed` | New `Down` classification and the reasons for it. | Core authority |
| `ytg_recalculated` | New and previous yards-to-go in milli-units. | Core authority |
| `possession_started` | Fresh personnel, the possession's budget grant, timestamp. | Core authority |
| `completion_rejected` | Timestamp and reason a completion claim was refused (open mandatory gates). | Core authority |
| `drive_completed` | Timestamp of the terminal completed state. | Core authority |
| `abandon_issued` | Reason the drive was abandoned. | Core authority |
| `side_effect_reconciled` | Idempotency key and the explicit boolean of whether the effect occurred. | Core authority / live runtime |

### Gates, evidence and capability discovery (9 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `gate_declared` | A pristine `Gate`: weight, dependencies, required evidence classes, mandatory flag. | Core authority |
| `preflight_completed` | Missing evidence classes and the capability generation inspected. | Core authority |
| `capability_observed` | A discovered `Capability` and its generation. | Core authority |
| `capability_requirement_issued` | A `CapabilityRequirement`: blocking gate, provision strategy, estimated cost, reusability. | Core authority |
| `evidence_accepted` | Sealed `Evidence`: class, body, digest id, observation time. | Core authority |
| `artefact_produced` | An `ArtefactRef` with id, digest and validation flags. | Core authority |
| `artefact_validated` | Artefact id bound to the evidence id that validated it. | Core authority |
| `artefact_rejected` | Artefact id bound to the evidence id that rejected it. | Core authority |
| `waiver_granted` | Gate id, named authority, reason. | Core authority |

### Plays and execution (7 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `play_selected` | The `DecisionRecord`: policy version, input state hash, classification, chosen play, candidate plays and rejection reasons. | Core authority |
| `personnel_changed` | The `Personnel` now on the field. | Core authority |
| `tool_invoked` | The `PlayExecution`: operation, capability and generation, personnel, expected evidence, budget ceiling, timeout, retry policy, idempotency key — persisted before the plane is called. | Core authority / live runtime |
| `tool_succeeded` | The `PlayResult`: status, output refs, evidence, artefact refs, cost, side-effect notes. | Core authority |
| `tool_failed` | The `ToolFailure`: failure kind, diagnostics, cost, idempotency key. | Core authority |
| `tool_outcome_rejected` | Optional result or failure plus the reason the outcome could not be committed; the observation is preserved for audit. | Core authority / live runtime |
| `budget_consumed` | The consumed `DriveBudget` delta and timestamp. | Core authority |

### Routing memory and scouting (2 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `assessments_recorded` | The assessed `Personnel` records with their measured fields. | Core authority |
| `priors_loaded` | An imported `HistoricalPriorTable` seeding routing with measured history. | Core authority |

### Fourth-down doctrine (3 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `fourth_down_declared` | Convert probability, policy threshold, and the chosen `Disposition`. | Lastdown |
| `field_goal_selected` | The gate id whose banked value is taken. | Lastdown |
| `punt_issued` | The sealed `ContinuationPackage` handed to the next possession. | Lastdown |

### Bench and substitution (2 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `bench_decision` | Personnel id, benched flag, reason code — the Bench verdict, including huddle-review benchings. | Franchise (Bench) |
| `player_substituted` | Substitution class, outgoing personnel id, optional replacement id, reasons. | Franchise (Bench) |

### Huddle (12 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `huddle_requested` | Huddle id, scope, trigger, timestamp, coherence debt reading, rework-avoided estimate. | Huddle |
| `huddle_admitted` | Budget envelope, barrier mode, participants. | Huddle |
| `huddle_rejected` | Admission refusal reason code. | Huddle |
| `huddle_opened` | Report collection begins. | Huddle |
| `huddle_participant_invited` | One participant with its disclosure profile. | Huddle |
| `huddle_report_submitted` | One normalized participant report. | Huddle |
| `huddle_contradiction_detected` | One explicit contradiction object. | Huddle |
| `huddle_budget_consumed` | One consumption delta against the envelope. | Huddle |
| `huddle_outcome_committed` | The reconciled outcome and the measured post-reconciliation debt. | Huddle |
| `huddle_escalation_requested` | Target scope and justification. | Huddle |
| `huddle_escalated` | New huddle id and target scope — a new admission, never a budget extension. | Huddle |
| `huddle_closed` | Terminal reason code (`Committed`, `Partial`, `ForcedSynthesis`, `Rejected`, `Aborted`). | Huddle |

### Blitz (26 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `blitz_proposed` | Blitz id, trigger, frozen `TargetPackage`, `DefensivePressure`, timestamp. | Blitz |
| `blitz_authorised` | Budget envelope, corroboration mode, resource mode, timestamp. | Blitz |
| `blitz_rejected` | Admission refusal reason code. | Blitz |
| `blitz_target_frozen` | The sealed content-addressed target hash. | Blitz |
| `blitz_worker_dispatched` | Worker slot and the formation's effective parallelism in milli-units. | Blitz |
| `blitz_worker_started` | Worker id and start timestamp. | Blitz |
| `blitz_worker_exited` | Worker id, exit kind, timestamp. | Blitz |
| `blitz_worker_spent` | One worker's `WorkerSpent` cost record. | Blitz |
| `blitz_wave_closed` | Wave number and diagnostics — a later wave may only start after this. | Blitz |
| `blitz_candidate_received` | The streaming `Candidate` and its target hash attribution. | Blitz |
| `blitz_verification_started` | Candidate id whose gate verification began. | Blitz |
| `blitz_candidate_rejected` | Candidate id, failed `VerificationKind` gate, reason code. | Blitz |
| `blitz_candidate_verified` | Candidate id and the `VerificationKind` gates it passed. | Blitz |
| `blitz_verification_unavailable` | Candidate id whose verifier was unreachable — it stays untrusted forever. | Blitz |
| `blitz_human_confirmed` | Candidate id and the confirming authority (the `human_confirmation` mode). | Blitz |
| `blitz_winner_selected` | Candidate id, winning worker id, timestamp — exactly one per recovered blitz. | Blitz |
| `blitz_cancel_started` | Cancellation of redundant inference begins. | Blitz |
| `blitz_worker_cancelled` | Worker id plus the tokens and cost the stop avoided. | Blitz |
| `blitz_cancel_complete` | Cancelled count, cancellation latency, total avoided tokens and cost. | Blitz |
| `blitz_recovery_confirmed` | Evidence references proving recovery, timestamp. | Blitz |
| `blitz_budget_consumed` | One `BlitzCost` consumption delta against the envelope. | Blitz |
| `blitz_exhausted` | The `BlitzFailure` class that ended a race with no verified winner. | Blitz |
| `blitz_escalated` | The `HuddleRecommendation` emitted through the wave-3 seam. | Blitz |
| `blitz_superseded` | The successor blitz id that replaced a drifted target. | Blitz |
| `blitz_failed` | Closure reason code (e.g. cancellation failure). | Blitz |
| `blitz_aborted` | Closure reason code for a deliberate abort. | Blitz |

### Game contract, evaluation, CERS and TT (6 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `game_contract_sealed` | The sealed `GameContract` and timestamp. | Core contract |
| `game_contract_amended` | The amended `GameContract` naming its predecessor and timestamp. | Core contract |
| `evaluation_decided` | The chosen `EvaluationBranch` and its reasons. | Franchise coordinator |
| `cers_recovered` | Artefact id, recovery source, confirmed facts, failed assumptions, reuse hint. | CERS |
| `directive_sealed` | The `TtDirective` with its specification hash. | TT |
| `directive_executed` | The `TinkerReport`: conformance, named deviation, evidence references. | TT |

### Tactics (5 events)

| Event name | Payload essence | Subsystem |
| --- | --- | --- |
| `tactic_admitted` | Tactic name and timestamp — the trigger fired and net expected value cleared the floor. | Tactics |
| `tactic_rejected` | Tactic name, machine reason code, timestamp. | Tactics |
| `tactic_completed` | Tactic name, success flag, yards-to-go before and after, tokens spent, timestamp. | Tactics |
| `tactic_failed` | Tactic name, failure mode, timestamp. | Tactics |
| `tactic_countered` | Tactic name and the counter that was declared against it. | Tactics |

**Total: 80 events** — 8 + 9 + 7 + 2 + 3 + 2 + 12 + 26 + 6 + 5. The blitz
projection is total: every blitz race event has exactly one journal form, one to one.

## Reading the vocabulary

The vocabulary is layered, and each layer answers a different question about the
organisation:

1. **Franchise** — who owns what across time. The franchise layer holds objectives,
   the roster and its freezing, the salary cap, the Bench, Manmark and scouting; its
   decisions survive any individual player and any individual drive.
2. **Formation** — how deployed personnel are arranged before any play is called.
   The formation engine (Spread through Two-High) and the huddle scopes decide who
   holds which domain, who may see what, and how much context overlaps.
3. **Play** — what one bounded assignment does. Play selection classifies the down,
   scores admissible plays, and commits a `DecisionRecord` whose inputs are hashed;
   execution is enveloped, idempotent and evidence-yielding.
4. **Adjustment** — how the shape changes in response to measurement. Adjustments
   (Motion, Shift, Audible, Check-With-Me), sugar huddles, blitz waves and tactic
   counters all re-shape the organisation between irreversible steps, never after
   them.

The naming rule: a football concept earns its place in Playbook only when it creates
a distinct, testable, economically meaningful orchestration behaviour. "Blitz" is in
because parallel single-focus racing with early cancellation is a behaviour generic
parallelism, retries, swarms and voting do not have; "Huddle" is in because a
budgeted, escalation-ranked reconciliation barrier is a behaviour a chat channel does
not have. A word that names only atmosphere — no admission rule, no invariant, no
measured trade — stays out. Every term in the glossary above satisfies the rule, and
each points at the crate whose tests hold it there.

## Cross-references

- Normative contracts: [the franchise contract](../franchise.md), [the huddle
  contract](../huddles.md), [the blitz contract](../blitz.md), [the tactics
  contract](../tactics.md), [the live runtime contract](../live-runtime.md),
  [the closure campaign](../closure-campaign.md), and
  [the server directive](../server-directive.md).
- Anthology companions: [00 Executive overview](00-executive-overview.md),
  [01 System architecture](01-system-architecture.md),
  [02 Authority and governance](02-authority-and-governance.md),
  [03 Evidence, journal, and replay](03-evidence-journal-replay.md),
  [04 Operations runbook](04-operations-runbook.md),
  [05 Security and boundaries](05-security-and-boundaries.md),
  [06 Metrics and quality gates](06-metrics-and-quality-gates.md), and
  [08 Personnel and organisation](08-personnel-and-organisation.md).
- Source of record for the event index:
  `crates/playbook-events/src/lib.rs` (the `DriveEvent` enum and its `name()` map);
  the fold lives in `crates/playbook-state` and the write-ahead store in
  `crates/playbook-journal`.
