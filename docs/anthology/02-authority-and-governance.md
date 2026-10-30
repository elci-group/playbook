# Playbook Authority and Governance Model

## Purpose of this document

Playbook is a deterministic, evidence-governed organisational control plane
for heterogeneous AI and tool personnel. Its value to an enterprise rests on
one question an architecture reviewer must be able to answer precisely: who
may decide what, and where are the boundaries? This document answers it. It
enumerates the decision domains of the system, names the deterministic
mechanism that decides each one, points at where that mechanism lives in the
code, and states what each authority refuses. It then covers the system's
posture toward refusal, the governance of change to the system itself, and
the escalation and override paths that exist — and, just as importantly, the
paths that deliberately do not.

The normative sources behind this summary are the contract documents
(`docs/huddles.md`, `docs/blitz.md`, `docs/tactics.md`, `docs/franchise.md`,
`docs/closure-campaign.md`) and the crate-level documentation in
`crates/*`. Where this document and a contract disagree, the contract wins.

## The constitutional principle

The system's constitutional rule is a four-stage separation:

1. **Inference proposes.** Models, specialist agents, and other cognitive
   personnel generate recommendations: a huddle request, a blitz urgency
   proposal, a tactical directive, a gate proposal, a report in a
   reconciliation.
2. **Deterministic policy authorises.** Admission economics — expected-value
   thresholds, debt bands, pressure scores, net-value arithmetic, conflict
   matrices — decide whether a proposal proceeds. The same state and the
   same policy always produce the same decision.
3. **Authorities execute.** A single writer per drive
   (`crates/playbook-state/src/authority.rs`: "Workers return evidence. This
   authority commits it") performs every state mutation inside the fold.
   Parallel agents never mutate the drive themselves.
4. **Evidence validates.** Only sealed, content-addressed evidence moves
   gate and win-condition state. Narrative claims are stored and then
   ignored by the gate graph (`crates/playbook-evidence`: "A narrative claim
   is stored and then ignored by the gate graph").

There are exactly seven things inference may never control, regardless of
what it recommends:

| Domain | Why inference is excluded | Enforced by |
| --- | --- | --- |
| Evidence acceptance | Whether an observation counts as evidence is decided by sealing (digest chain) and class policy, not by the claimant. | `Evidence::seal/verify` in `crates/playbook-core/src/domain.rs`; `EvidencePolicy::accepts`; the fold's artefact rejection |
| Game completion | A completion claim is checked, never accepted. | `verify_win_conditions` in `crates/playbook-core/src/contract.rs`; `CompletionRejected` at fold time |
| Budget enforcement | Ceilings are policy envelopes; a consumption delta that would pass any ceiling is refused, and no envelope extends itself. | `DriveBudget`, `HuddleBudget`, `BlitzBudget` governors, refusals at fold |
| Roster eligibility | Eligibility comes from the frozen roster and measured availability; self-reported capability never outranks the table. | `playbook-front-office` roster construction and activation; `playbook-franchise::bench` eligibility rules |
| Journal mutation | The event log has one writer and one replay law: reproduce the same state or refuse the log. | `playbook-journal` fold (`ApplyMode::Audit | Resimulate`); out-of-order or unknown events are errors, never silent repairs |
| Gate state | Models may propose gates; only evidence, or an explicit waiver, moves them. | `crates/playbook-gates/src/lib.rs` (`apply_evidence`, `waive`) |
| Win declaration | Victory has exactly one definition the system recognises: every mandatory win condition verified against measured state. | `ContractVerification::verified_win` in `crates/playbook-core/src/contract.rs` |

The same exclusion is written into the tactical layer itself. The frozen
tactical contract declares an `AuthorityPolicy` that enumerates what a tactic
is *allowed* to alter — assignments, budget allocation, information topology,
observation — and its module documentation states the boundary directly: a
tactic "may never alter evidence truth, YTG, gate satisfaction, objective
definition, frozen roster eligibility, the salary cap, or an accepted tool
result — only the existing authorities do that. Tactics recommend;
authorities execute" (`crates/playbook-tactics/src/domain.rs`). The security
layer expresses the same idea from the other side: `authorize` in
`crates/playbook-security/src/lib.rs` takes no budget parameter, because
spending cannot purchase permission — in its own words, "a third-down pass
cannot spend its way past it."

## The authority registry

Every consequential decision in Playbook belongs to exactly one deterministic
authority. The registry below is the complete map an architecture reviewer
should audit against.

| Decision domain | Deciding mechanism | Where it lives in code | What it refuses |
| --- | --- | --- | --- |
| Play selection | Deterministic ranking inside policy: admissibility first, then deterministic personnel before any specialist; "price alone does not win, intelligence alone does not win". The EVALUATING branch ladder names one branch (ADVANCE / HUDDLE / SUBSTITUTE / LASTDOWN / BLITZ / CERS_RECOVERY) with reason codes. | `crates/playbook-routing/src/lib.rs` (`admit`, `select`, `admit_swarm`); `crates/playbook-state/src/classify.rs` (`classify_measurement`); `crates/playbook-state/src/evaluate.rs` (`evaluate_branch`) | Closed drives, unresolved reconciliation, missing preflight, inadmissible plays, authorisation stops, blind retries, unjustified swarms (`SwarmUnjustified`), and `NoPlay` when nothing admissible remains |
| Huddle admission | Expected-value economics on eight deterministic 0..=1000 components — gain (rework avoided, failure reduction, coordination, budget preservation) minus cost (inference, latency, context, opportunity) — admitted when EV clears the policy floor; coherence-debt bands (monitor 200, huddle-eligible 400, coordinator 600, mandatory 800) feed the trigger taxonomy; three named bypasses skip the economics. | `crates/playbook-huddle/src/admission.rs` and `trigger.rs`; envelopes in `crates/playbook-huddle/src/budget.rs`; committed through the authority in `crates/playbook-state/src/authority.rs` | `scope_busy`, `unchanged_field` (huddle-storm guard on a repeated field fingerprint), `no_budget`, `scope_unjustified` (team/franchise scope without mandatory debt or manual authority), `expected_value`; admission also refuses an overlapping state partition lock |
| Blitz eligibility | Instinctive class: six named machine conditions with inference not in the loop (`critical_cve_unpatched`, `outage_rollback_failed`, `health_floor_rollback_failed`, `power_critical`, `quota_exhaustion_imminent`, `connectivity_loss_imminent`). Strategic class: inference proposes urgency; policy admits only above both floors — urgency at `min_urgency_score` (300) and the seven-component weighted pressure score at `min_pressure_score` (500). Difficulty never admits a blitz by itself. | `crates/playbook-blitz/src/trigger.rs` and `admission.rs` | Any expenditure below the policy floors; the closure records `policy_rejection` as a named failure class, so a refused defence is visible in the audit log |
| Blitz authorisation | Envelope solvency: the resource mode selects one of five policy envelopes in `policy.blitz.budgets`; every dimension is monotonically increasing, so the governor cannot be argued with after the fact. Formation floors apply: at least 2 providers, 2 model families, at most 3 workers per family, worker score at `min_worker_score` (2,000), effective parallelism at `min_effective_parallelism_milli` (1,500). | `crates/playbook-blitz/src/budget.rs` and `roster.rs`; enforced at fold in `crates/playbook-blitz/src/apply.rs` | Consumption past any ceiling, formation below diversity or parallelism floors, a second active blitz (single focus is structural), candidates attributable to any target hash other than the frozen one; no blitz extends itself — exhaustion is a closure, not a renegotiation |
| Tactic admission | Trigger evaluation over measured `TacticalContext` (deterministic and total) AND net expected value (benefit − cost − risk in ppm) clearing the policy threshold; composition through the machine-readable, total, symmetric conflict matrix (`Spread + Heavy` exclusive, `Empty + Max Protect` invalid, `Prevent + Verticals` conflicting). | `crates/playbook-tactics/src/admission.rs` and `composition.rs`; the contract types in `crates/playbook-tactics/src/domain.rs` | Triggers that do not fire, net value at or below threshold, conflicting or invalid tactical pairs, unknown tactics in a plan, one name belonging to two classes in one plan — `compose` fails closed — plus shape defects: an unowned critical domain, joint ownership with a single owner, double-booked personnel |
| Substitution | The Bench five-class ladder, evaluated most urgent first: emergency, availability (provider health below `MIN_PROVIDER_HEALTH_PPM` 100,000), performance (`PERFORMANCE_FAILURE_FLOOR` 3 recent gate failures against a strictly better replacement), economic (half the grant spent, a replacement at half the cost), tactical (fit below the play's requirement). Eligibility is measured: available, not carrying an unresolved invocation, never the starter. A total order (fit, provider health, failures, cost, id) makes bench order irrelevant. | `crates/playbook-franchise/src/bench.rs`; huddle-driven verdicts via `Authority::bench_from_review` in `crates/playbook-state/src/authority.rs` | Any substitution without an eligible replacement — the ladder keeps descending, and a ladder that reaches its floor keeps the starter playing (`decide` returns `None`). Prestige, tier, and self-reported capability are never consulted |
| Win declaration | Sealed `GameContract` win conditions verified against measured state: `verify_win_conditions(contract, gates, evidence, artefacts, environment)` produces a per-condition `ConditionResult` with reason codes; `verified_win` is true only when every mandatory condition passed. Contract sealing requires at least one mandatory win condition and unique ids. | `crates/playbook-core/src/contract.rs`; enforced at fold in `crates/playbook-state/src/apply.rs` (`CompletionRejected`) | Any false completion claim against an unmet win condition is refused at fold time; a contract with no mandatory win condition cannot be sealed, so the system can never enter a state where it could not lose honestly |
| Evidence admission | Digest sealing and class policy: `Evidence::seal` requires an object body and derives a SHA-256 content-addressed id over canonical bytes; `verify` recomputes the digest and refuses any mismatch; `EvidencePolicy::accepts` admits only classes that can satisfy gates and appear in the accepted set; `playbook-evidence::score` grades each class deterministically (compiler results, test reports, benchmark and static analysis outcomes), leaving `NarrativeClaim` inert. | `crates/playbook-core/src/domain.rs` (`Evidence`, `EvidenceClass`); `crates/playbook-evidence/src/lib.rs` (`score`); `crates/playbook-gates/src/lib.rs` (`apply_evidence`) | Tampered bodies (`DigestMismatch`), non-object bodies, classes outside the evidence policy, and any narrative claim offered as gate satisfaction — it scores zero and satisfies nothing |

Two structural observations for reviewers. First, every mechanism in the
"deciding mechanism" column is arithmetic or table lookup over measured
state — thresholds, weights, ladders, digests — and the tunable values live
in policy (`policy.huddle`, `policy.blitz.budgets`), never in compiled
business logic, so governance changes thresholds without touching decision
structure. Second, the authorities are layered, not merely listed: the
branch ladder chooses between subsystems, the subsystems admit or refuse,
and the fold is the single point where any of their decisions touches
state.

## Refusal as a first-class outcome

Playbook's posture is uniformly **fail closed**. Every decision point can
refuse, every refusal carries a machine reason code, and every refusal is
journalled. The four rejection events on the drive ledger make the negative
space of the system auditable:

- `TacticRejected` — the tactical admission engine refused a directive
  (trigger, net value, composition, or shape), journalled like the five
  other tactical event kinds.
- `HuddleRejected` — admission economics or scope justification refused a
  huddle; the audit log shows why communication did not happen.
- `BlitzRejected` — policy refused the defensive expenditure at admission;
  the audit log shows why the defence did not spend.
- `CompletionRejected` — the fold refused a completion claim against an
  unmet win condition.

Refusal is not an exceptional path bolted onto the system; it is a designed
outcome with its own semantics. A huddle that cannot commit records why and
the official drive record stays untouched. An unavailable verifier leaves a
blitz candidate untrusted forever. A failed validation is never reinterpreted
as success. The blitz race state machine names "fail closed" as one of its
seven invariants: a failed controller, verifier, or budget governor never
commits an unverified artefact. Replay extends the same posture to the log
itself: a tampered or out-of-order event is an error, never a silent repair,
so the historical record cannot be negotiated with either.

The security crate's test suite states the economic corollary plainly: low
budget does not create a deployment permission. Refusal is not for sale, and
the refusals are deterministic — the same state and policy always produce the
same refusal, for the same reason code.

## Governance of change

The authorities govern the runtime; a separate layer governs changes to the
system itself. Three mechanisms carry it.

**The tov policy-check layer.** Replaceable scoring logic lives in the
`tov/` tree (adapters, agents, blitz, diagnostics, huddle, policies,
tactics, transforms) and is checked with `tov check --json`. The Rust crate
remains the authority: tov modules are compile-checked, versioned as source,
and failure-isolated, and a missing or failing module falls back to the
deterministic Rust scorer. This is the governed channel for evolving the
*scoring* of decisions (coherence-debt components, trigger rankings, worker
scores, report normalisation) without ever moving the decision itself out of
deterministic code — the huddle and blitz modules under `tov/huddle` and
`tov/blitz` are the pattern, and the fallback rule means a compromised or
broken module degrades to the built-in arithmetic rather than to judgement.

**The deliver gate as the merge authority.** `deliver.toml` at the
repository root declares the deliverables a change must present: required
files, minimum sizes, and the `forbid_regex` list of unfinished-work and
stub markers applied to crate sources. The deliver gate verifies, before any
task is declared complete or handed off, that deliverables exist, contain no
forbidden placeholders, and pass the required build and test commands. It is
the merge-time analogue of a win condition: the change itself is not "done"
because someone claims it is, but because a deterministic check against the
declared contract says so.

**The frozen-contract pattern.** Shared contracts are frozen, sealed, and
closed to silent extension. The tactical contract was frozen at Huddle A
("the whole of what the tactical modules share"), and every contract type —
`TacticalDirective`, `TacticalContext`, `GameContract`, `Waiver`,
`Evidence`, `BenchCandidate` — carries `serde(deny_unknown_fields)`, so an
unexpected field is a deserialisation error, never tolerated drift. The
`GameContract` shows the amendment path for anything more substantive:
`seal` computes a SHA-256 over canonical bytes, and `amend` never mutates
the predecessor — it produces a new sealed contract whose
`predecessor_hash` names the one it supersedes. Immutability plus
predecessor naming gives the audit trail of a blockchain ledger without the
apparatus: any contract in hand can prove both its contents and its lineage.
Journal events gain new fields through `serde(default)` so historical logs
replay unchanged — the same respect for frozen history, applied to the event
schema.

## Escalation and override paths

The system is strict, not brittle: it names the paths by which a decision
moves upward or is overridden, and each path has a deterministic gate.

**Huddle escalation.** Escalation moves only up the five-rank ladder —
`position` (0), `unit` (1), `coordinator` (2), `team` (3), `franchise` (4) —
and only with scope justification. Reaching `franchise` scope additionally
requires manual authority; without it the escalation is refused. An
escalation is not a continuation: it closes the escalating huddle and opens
a new admission at the target scope with its own trigger, budget, and
decision. Escalation is never a budget extension.

**Manual authority at admission.** A huddle requested under manual authority
(`HuddleRequest.manual`) is one of the three named bypasses of the admission
economics, alongside mandatory coherence debt and a failure-conditioned
trigger at eligible debt. Manual authority widens what may be *discussed*;
it does not touch what the discussion may *change* — a committed outcome
still may never alter gate satisfaction, historical evidence, or the
measured field.

**Waiver grants with named authority.** The one override of gate state is
explicit and attributed: `Waiver { authority, reason }`, where both fields
must be non-empty or construction fails with `WaiverAuthority`. Applying a
waiver marks the gate's satisfaction source as `Waiver` alongside the
evidence, so the ledger distinguishes forever between a gate satisfied by
proof and a gate satisfied by a named human decision. The gate module's
constitution holds: models may propose gates; only evidence, or an explicit
waiver, moves them.

**Blitz seams.** A blitz that has exhausted independence does not take
matters into its own hands — wave 3 emits a `HuddleRecommendation` that the
caller maps onto an actual huddle request; the blitz never opens one itself.
Corroboration may require `human_confirmation`, placing a named human inside
the verification path, but even then the underlying candidate must first
pass its deterministic gates: the human confirms a verified candidate; a
human assertion never substitutes for one.

**What deliberately has no override.** Two things are beyond every path,
manual or otherwise:

- **Evidence truth.** No authority, flag, or waiver can accept a tampered
  evidence record, promote a narrative claim to gate weight, or rewrite a
  sealed digest. The regulator scopes visibility of evidence; it must never
  alter evidence itself. Consensus never satisfies a gate and never deletes
  a contradiction.
- **Completion without verified win conditions.** No manual flag declares a
  game won. `verified_win` is computed from the sealed contract against
  measured state, and the fold refuses anything else. The override of a
  win condition, where a business genuinely needs one, is the predecessor-
  named amendment of the contract — a governed, sealed, auditable act —
  never an in-flight assertion.

This asymmetry is the design's centre of gravity: process can be escalated
and, within attributed limits, overridden; facts cannot. An organisation
that adopts Playbook inherits exactly that bargain.

## Cross-references

- [00 — Executive overview](00-executive-overview.md): the guarantee set
  this governance model underwrites.
- [01 — System architecture](01-system-architecture.md): how the crates
  behind each authority compose.
- [03 — Evidence, journal, and replay](03-evidence-journal-replay.md): the
  sealing, digest, and fold mechanics that make refusals replayable.
- [05 — Security and boundaries](05-security-and-boundaries.md): side-effect
  authorisation and the least-privilege surface personnel may touch.
- [07 — Glossary and event index](07-glossary-and-event-index.md): the
  vocabulary and the full event catalogue, including every rejection event.
- Contract documents: `docs/huddles.md` (admission economics, escalation
  ladder, budget envelopes), `docs/blitz.md` (eligibility, target freeze,
  race invariants), `docs/tactics.md` (tactical authority boundary and
  composition rules), `docs/closure-campaign.md` (the FC-1 win-condition
  closure and gate status).
- Normative code: `crates/playbook-state/src/authority.rs` (single writer),
  `crates/playbook-core/src/contract.rs` (GameContract), `crates/playbook-routing/src/lib.rs`
  (play selection), `crates/playbook-huddle/src/admission.rs`,
  `crates/playbook-blitz/src/trigger.rs` and `budget.rs`,
  `crates/playbook-tactics/src/admission.rs` and `composition.rs`,
  `crates/playbook-franchise/src/bench.rs`,
  `crates/playbook-gates/src/lib.rs`, `crates/playbook-security/src/lib.rs`.
