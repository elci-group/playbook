# Franchise Runtime Closure — campaign map

This document is the Examine/Locate record for the Franchise Runtime Closure
directive. It maps every campaign phase and release gate onto the crates that
exist today, names the smallest architectural gap in each, and fixes the
sprint order. The directive's strategic rule governs: integration, runtime
enforcement, measurement, and proof outrank new conceptual subsystems.

**STATUS: ALL SIX RELEASE GATES CLOSED** as of the FC-1..FC-7 delivery. Gate
A: `GameContract`/`WinCondition` with fold-enforced completion gating
(`playbook-core::contract`), `ProgressVector` (`playbook-ytg`), and the
EVALUATING branch ladder (`playbook-state::evaluate`). Gate B: the Bench
decision service and the Manmark registry with the strategic→instinctive
graduation path (`playbook-franchise::{bench,manmark}`), plus the sealed
Thinker→Tinker directive. Gate C: huddles. Gate D: Lastdown plus CERS
(`playbook-cers`). Gate E: Blitz. Gate F: the canonical Franchise Game runs
end-to-end through the journal fold
(`playbook-sim/tests/canonical_game.rs`) with the §28 demonstration trace
and no human intervention. The sections below are the original gap analysis,
retained as the record of what was found and what was built.

## Release gate status

### Gate A — Deterministic foundation: LARGELY MET

| Requirement | Where it lives today | Gap |
| --- | --- | --- |
| Game state machine | `playbook-state` `DriveState`, `DriveStatus`, the `Down` ladder, `classify_measurement` | The EVALUATING branch (§6: ADVANCE / HUDDLE / SUBSTITUTE / LASTDOWN / BLITZ / CERS_RECOVERY) is implicit — spread across `classify`, `doctrine`, and per-subsystem triggers. No single deterministic decision record names the branch chosen and why. |
| Replayable journal | `playbook-journal` (`Journal`, `Recovery`), `fold(..., ApplyMode::Audit \| Resimulate)`, fixture equality tests in `playbook-huddle/tests/fixtures.rs` and `playbook-blitz/tests/fixtures.rs` | None. Replay is proven by checked-in fixtures that must fold identically under audit and resimulate. |
| Immutable GameContract | `TaskContract` (statement, workload class, constraints, integrity) + `Objective` in `playbook-franchise` | **The P0 gap.** Win conditions are not structured. There is no `WinCondition` type that is externally inspectable, individually addressable, and independently verifiable — so "verified win" rests on gate satisfaction alone, with no contract-level enumeration of what a win means. Loss conditions, an evidence policy, and an escalation policy are likewise unnamed. |
| Budget enforcement | `DriveBudget`, `effective_remaining_ppm`, `HuddleBudget`, `BlitzBudget` per resource mode, governor refusals at fold time | None. |
| Independent evidence validation | `Evidence::seal/verify` digest chain, `playbook-gates::apply_evidence`, narrative claims carry no gate weight | None. |

### Gate B — Team operation: PARTIAL

| Requirement | Where it lives today | Gap |
| --- | --- | --- |
| FranchiseState | `playbook-franchise` (`Franchise`, `FranchiseSnapshot`, `FranchiseRecord`, objectives, frozen rosters, management history) | Institutional memory covers roster/history/priors. Missing: `provider_reliability`, `cost_history`, and `recovered_patterns` (CERS knowledge) as first-class franchise state (§5). |
| Front Office roster | `playbook-front-office` (offline portfolio construction) + `playbook-franchise` (`stage_roster`, `activate_roster`, frozen per-drive rosters) | None of substance. The eligibility/selection boundary is enforced and tested. |
| Manmark ownership | Doctrine only; `playbook-blitz::escalation::ManmarkRequest` is the sole seam | **The P1 gap.** There is no runtime mark registry: no `domain / owner / severity / state / evidence / last inspection / next inspection trigger` records, no instinctive-vs-strategic marking runtime, no strategic→instinctive graduation path (§8). |
| Bench substitutions | `BenchDecision` journal event, franchise availability (`ready/benched/unavailable`), huddle `PersonnelAction` recommendations | Substitution is possible but not decided by a deterministic service. The five substitution classes (tactical / performance / availability / economic / emergency, §10) have no decision function and no measured input contract. |
| TT execution | Nothing | P2. Thinker→Tinker separation with control return on invalid premise (§16) is absent; do not build before P0/P1 close. |

### Gate C — Coherence: MET

Huddles are fully operational: condition-triggered admission economics
(`playbook-huddle::admission`), coherence-debt bands, scoped context packets
with asymmetric disclosure, contradiction sets, forced synthesis, escalation
ladder, fold-enforced journal events, fixtures, and tov mirrors. Blitz Wave 3
hands `HuddleRecommendation` to this subsystem; wave 1 independence is
fold-enforced.

### Gate D — Recovery: PARTIAL

| Requirement | Where it lives today | Gap |
| --- | --- | --- |
| Lastdown | `playbook-lastdown` doctrine + franchise continuation packages | None. |
| Provider/agent failure recovery | `ToolOutcomeRejected`, idempotency reconciliation, open-execution recovery, artefact rejection | None. |
| CERS | Nothing | **The P1 gap.** Failed work leaves failure *evidence* (sealed `Evidence`, `failure_refs`) but no structured recovery artefact: confirmed facts, failed assumptions, partial patches, reproduction procedures, rejected hypotheses (§13). CERS must consume blitz/huddle residue too (post-Blitz harvesting is a named Blitz requirement). |
| Incorrect-agent-output recovery | Validation failures classified, artefacts rejected at fold | Covered by the above once CERS lands. |

### Gate E — Defence: MET (this campaign)

Blitz shipped in `playbook-blitz`: instinctive/strategic eligibility with
deterministic thresholds, policy authorisation, content-addressed target
freeze with supersession, diversity-scored formation selection with effective
parallelism, the fold-enforced race state machine (single focus, independent
first wave, verification before victory, bounded expenditure, early
cancellation, fail closed, evidence preservation), per-mode envelopes, the
failure taxonomy, and economics (`BlitzPremium`,
`CostPerSecondRecovered`). BZ-T01..T12 plus nine fixture scenarios
demonstrate every required injection: fast wrong answers, provider outage,
budget/deadline exhaustion, correlated failure, cancellation races, verifier
failure, target mutation, power/network-critical formation bias.

### Gate F — Franchise proof: NOT MET

The canonical adversarial game harness (§19–§20) does not exist. The
`playbook-sim` crate proves the MVP sequence and huddle economics but injects
neither hallucinated completion, nor provider outage, nor misleading claims,
nor conflicting implementations, nor strict spend ceilings against a real
repository. Gate F is blocked on P0/P1 closure and is the last gate before
"autonomous franchise runtime" may be claimed.

## Sprint order (P0 first, per the directive)

1. **FC-1 — GameContract win conditions (P0).** Introduce `WinCondition`
   (individually addressable id, deterministic `kind` — gate satisfied,
   evidence present, artefact validated, threshold met, forbidden dependency
   absent — plus subject and mandatory flag), a `GameContract` carrying them
   with the objective, and a deterministic
   `verify(contract, gates, evidence, artefacts) -> VerificationReport`.
   Journal events gain the sealed contract through `serde(default)` fields so
   existing logs replay unchanged. Exit gate: a false completion claim
   against any unmet win condition is refused at fold time, with fixtures.
2. **FC-2 — ProgressVector (P0).** Multi-dimensional YTG (resolved /
   unresolved requirements, failing gates, uncertainty, evidence strength,
   regression count) derived from the drive state; the scalar stays derived
   from the vector, never the reverse. Exit gate: a regression in one
   dimension cannot hide behind average progress; fold replay proves the
   vector.
3. **FC-3 — EVALUATING decision record (P0 integration).** One deterministic
   function that, given the drive state, names the branch (ADVANCE / HUDDLE /
   SUBSTITUTE / LASTDOWN / BLITZ / CERS_RECOVERY) with reason codes, wired to
   the existing triggers. No new orchestration — a single point where the
   existing subsystems are chosen between. Exit gate: replayable decision
   records in the journal; adversarial tests where a huddle, a substitution,
   and a blitz each win the branch.
4. **FC-4 — Bench decision service (P1).** Deterministic substitution
   decision over the measured inputs, classifying into the five classes,
   journalled. Exit gate: each class demonstrated by an injected failure and
   a journalled substitution.
5. **FC-5 — Manmark runtime (P1).** Mark registry with ownership, severity,
   inspection triggers, instinctive-first enforcement, and the
   strategic→instinctive graduation path. Blitz's `ManmarkRequest` becomes
   its escalation producer. Exit gate: a deterministic defender resolves a
   marked domain without inference; a strategic remediation graduates.
6. **FC-6 — CERS (P1).** Structured recovery artefacts from failed plays,
   huddles, and blitzes; consumable without the failed context; franchise
   `recovered_patterns` storage; reuse measured. Exit gate: a failed play's
   residue shortens a later possession in simulation.
7. **FC-7 — Game harness and canonical benchmark (Gate F).** Adversarial
   harness injecting the §19 list; the canonical game with deterministic
   validation, heterogeneous personnel, injected failures, and strict spend
   limits; VGCR and the §21 metrics recorded. Exit gate: the recorded
   demonstration of §28, end to end, no human intervention.

Jose profiles and TT remain P2 and are intentionally deferred behind the
runtime-closure work. Divcon-fed scouting economics remain P3.

## Boundary conditions honoured

- Deterministic control plane: every item above folds through
  `playbook-state`; inference recommends, never decides.
- Evidence over assertion: `WinCondition` verification consumes sealed
  evidence classes only; narrative claims satisfy nothing.
- Non-goals (§24) respected: no new football vocabulary, no dashboards, no
  uncontrolled agent messaging, no self-modification.
