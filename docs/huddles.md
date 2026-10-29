# Huddles

`playbook-huddle` is the synchronization subsystem of the drive authority. A
huddle is a budgeted, policy-admitted synchronization barrier invoked when the
expected cost of team-state divergence exceeds the expected cost of
cross-agent reconciliation. It exists to keep communication rare, deliberate,
hierarchical, evidence-bound and economically justified. A huddle is not a
discussion forum: it is an admission decision, a budget envelope, a state
partition and a committed outcome, all recorded in the drive ledger.

A committed huddle outcome may change assignments, tactics, assumptions,
sequencing, personnel recommendations and tool strategy. It may never change
gate satisfaction, historical evidence or the measured field. A failed
validation is never reinterpreted as success: a huddle that cannot commit
records why, and the official drive record stays untouched.

## Scopes and the escalation ladder

| Scope | Rank | Participants | Purpose |
| --- | --- | --- | --- |
| `position` | 0 | specialists of one position | reconcile repeated failure or a local contradiction |
| `unit` | 1 | personnel sharing a dependency edge | reconcile a gate unlock and its dependents |
| `coordinator` | 2 | coordinator and affected units | fourth-down disposition, coordinator-band debt |
| `team` | 3 | drive-wide membership | mandatory reconciliation at high coherence debt |
| `franchise` | 4 | franchise-level authority | cross-drive coherence under manual authority |

Escalation moves only up this ladder, from rank 0 to rank 4, and only with
scope justification. Reaching `franchise` scope additionally requires manual
authority. An escalation is not a continuation: it closes the escalating
huddle and opens a new admission at the target scope, with its own trigger,
budget and decision.

## Trigger taxonomy

Trigger evaluation is deterministic and ordered most severe first: failure,
then success, then strategic, then coherence debt.

| Kind | Reason code | Condition |
| --- | --- | --- |
| Failure-conditioned | `repeated_failed_play` | `failed_attempts` reaches `failure_threshold` while the field has not changed |
| Success-conditioned | `gate_unlocked_dependents` | a mandatory gate completed with at least two dependent personnel active |
| Strategic | `fourth_down_disposition` | the drive is on fourth down |
| Coherence-debt | `coherence_debt` | coherence debt reached the eligible band |

Coherence debt is a deterministic integer in 0..=1000: a weighted sum of
seven clamped per-mille components — assumption divergence, dependency
staleness, contradiction load, personnel turnover, cross-unit change impact,
unresolved blockers and context age. The bands are monitor at 200,
huddle-eligible at 400, coordinator-recommended at 600 and mandatory
reconciliation at 800. The failure threshold, band thresholds and debt
weights live in `policy.huddle`, never in compiled business logic.

## Admission economics

Admission compares expected gain against expected cost on eight deterministic
0..=1000 components:

```text
EV = (rework_avoided + failure_reduction + coordination_gain + budget_preservation)
   - (inference + latency + context + opportunity)
```

A huddle is admitted when `EV > minimum_expected_value`. Three bypasses skip
the economics: mandatory coherence debt, manual authority, and a
failure-conditioned trigger at eligible debt or above. A refusal always
carries a machine reason code:

| Reason code | Meaning |
| --- | --- |
| `scope_busy` | an active huddle already holds this scope |
| `unchanged_field` | the scope just closed on the same field fingerprint; a huddle storm is refused |
| `no_budget` | the drive's remaining budget cannot cover the scope envelope |
| `scope_unjustified` | team or franchise scope without mandatory debt or manual authority |
| `expected_value` | expected value at or below the policy floor |

The same state and policy always produce the same decision. Every refusal is
an event, so the audit log shows why communication did not happen.

## Budget envelopes

| Scope | `max_tokens` | `max_cost_microunits` | `max_rounds` | `max_participants` | `max_duration_ms` |
| --- | --- | --- | --- | --- | --- |
| `position` | 2,000 | 200,000 | 1 | 3 | 5,000 |
| `unit` | 6,000 | 600,000 | 2 | 5 | 15,000 |
| `coordinator` | 8,000 | 800,000 | 2 | 6 | 20,000 |
| `team` | 16,000 | 1,600,000 | 3 | 12 | 60,000 |
| `franchise` | 32,000 | 3,200,000 | 3 | 24 | 120,000 |

Envelopes live in `policy.huddle.budgets`. A consumption delta that would
pass any ceiling is refused, and exhaustion forces synthesis from the reports
already collected. No huddle extends itself. Escalation is a new admission
event with a fresh envelope; it is never a budget extension.

## Barrier modes and partitions

Three barrier modes exist. `soft` is the position default: the huddle
observes and recommends without pausing other work. `scope` is the default
for unit and coordinator scope: work inside the locked partition waits while
the huddle collects reports. `hard` applies to mandatory-debt huddles and
franchise scope: the covered work stops until the huddle closes.

A huddle locks exactly one state partition: its participants plus the
evidence its trigger cites. Only one huddle may lock a partition, and
admission refuses an overlap with `state partition is already locked`.
Disjoint partitions may run concurrently.

## State machine and journal events

A huddle advances `Requested` → `Admitted` → `CollectReports`, then closes as
`Committed`, `Partial`, `ForcedSynthesis`, `Escalated`, `Rejected` or
`Aborted`. Every transition is a drive event and every rule is enforced at
fold time.

| Event tag | Records |
| --- | --- |
| `huddle_requested` | huddle id, scope, trigger, timestamp |
| `huddle_admitted` | budget envelope, barrier mode, participants |
| `huddle_rejected` | admission refusal reason code |
| `huddle_opened` | report collection begins |
| `huddle_participant_invited` | participant with its disclosure profile |
| `huddle_report_submitted` | one normalized participant report |
| `huddle_contradiction_detected` | one explicit contradiction object |
| `huddle_budget_consumed` | one consumption delta |
| `huddle_outcome_committed` | the reconciled outcome |
| `huddle_escalation_requested` | target scope and justification |
| `huddle_escalated` | new huddle id and target scope |
| `huddle_closed` | terminal reason code |

The fold reconstructs active huddles, budgets, reports, contradictions and
outcomes from these events alone; nothing is a side channel. Replay
reproduces the same state or refuses the log: a tampered or out-of-order
huddle event is an error, never a silent repair.

## Contradictions

A contradiction is an explicit object with an id, a subject, the conflicting
claims and a severity of `low`, `medium`, `high` or `critical`. A resolution
is one of `resolved_by_evidence`, `resolved_by_supersession`,
`resolved_by_authority`, `deferred` or `unresolved_blocking`. Resolution by
evidence requires an evidence reference the drive knows; resolution by
authority requires a named authority; an `unresolved_blocking` entry belongs
in the unresolved list, not the resolved one. An unresolved critical
contradiction can only commit with an escalation target attached, and the
huddle then closes as `Partial`. Consensus never satisfies a gate and never
deletes a contradiction.

## Context packets and asymmetric disclosure

Each participant receives a purpose-built context packet: the task fragment,
relevant gate snapshots, summarized evidence, its current assignment,
dependency changes, open contradictions, its per-participant budget and the
requested response schema. The packet is the minimum sufficient shared
state; the drive transcript is never disclosed.

The `DefaultRegulator` derives the disclosure profile from the measured
personnel kind:

| Personnel kind | Gates | Peer reports | Strategic state | Evidence refs |
| --- | --- | --- | --- | --- |
| `model` | no | yes | no | 4 |
| `specialist_agent` | yes | no | no | 8 |
| `deterministic_procedure`, `tool`, `toolchain` | yes | yes | on strategic triggers only | 16 |
| `human_escalation` | yes | yes | yes | 32 |

No authorization, no disclosure: a field the profile does not authorize stays
empty. The regulator scopes visibility; it must never alter evidence itself.

## Tactical consequences

Phase 6 gives committed outcomes two journaled tactical consequences.

Bench review. `Authority::bench_from_review` walks the ledger's committed
substitution recommendations and journals a `bench_decision` event for each
one not yet acted on; the verdict cites the huddle's reason. Benched
personnel leave the selection pool until a reversed verdict returns them, a
personnel with an open execution cannot be benched mid-flight, and replaying
the review never double-benches. The measured routing rule is untouched:
self-reported capability still never outranks the table — the Bench acts, it
does not lobby.

Fourth-down coaching. The most recent coordinator-scope reconciliation whose
outcome carried a strategy change marks the conversion plan contested. On
fourth down the doctrine then banks instead of gambling: the convert
utility drops to zero and the possession takes the field goal when one is
banked. This is §17's "can this personnel group realistically convert?"
answered by the huddle and enforced by Lastdown.

## HTTP surface (on the drive service)

The drive service exposes six huddle routes. They support debugging and
external orchestration; autonomous execution does not require them, because
the same machine rules run inside the drive fold.

| Method | Route | Purpose |
| --- | --- | --- |
| POST | `/v1/drives/{id}/huddles/request` | request a huddle; the machine admits or rejects |
| GET | `/v1/drives/{id}/huddles` | active and closed huddles for the drive |
| GET | `/v1/drives/{id}/huddles/{huddle_id}` | one huddle with phase, budget and reports |
| POST | `/v1/drives/{id}/huddles/{huddle_id}/reports` | submit a participant report |
| POST | `/v1/drives/{id}/huddles/{huddle_id}/commit` | commit a reconciled outcome |
| POST | `/v1/drives/{id}/huddles/{huddle_id}/escalate` | request escalation up the ladder |

Every route still journals through the fold: the HTTP adapter can do nothing
the event log refuses.

## Invariants

- No evidence, no gate completion from a huddle.
- No state change, no repeated huddle on the same field.
- No admission, no participant invocation.
- No budget, no huddle.
- No authorization, no disclosure.
- No committed outcome, no tactical mutation.
- No evidence, no by-evidence resolution.
- A blocking contradiction permits no unsafe progression.
- No scope justification, no escalation.
- No huddle may alter historical evidence or rewrite yards-to-go.

## Tov modules

Huddle scoring is split into replaceable Tov modules under `tov/huddle`. The
Rust crate remains the authority: modules are compile-checked, versioned as
source and failure-isolated, and a missing or failing module falls back to
the deterministic Rust scorer.

| Module | Responsibility |
| --- | --- |
| `coherence_debt` | score the seven divergence components |
| `trigger_score` | rank the trigger taxonomy |
| `admission_score` | estimate the expected-value components |
| `participant_rank` | score candidate relevance |
| `normalize_report` | schema-validate and bound participant output |
| `contradiction_score` | grade contradiction severity |
| `escalation_policy` | propose the escalation target |
| `disclosure_filter` | shape context packets per profile |

## Observability

Huddles report: `huddles_total`, `huddles_rejected_total`,
`huddle_tokens_total`, `huddle_cost_total`, `huddle_duration_ms`,
`huddle_participants`, `huddle_escalations`, `huddle_contradictions_found`,
`huddle_contradictions_resolved`, `coherence_debt_before` and
`coherence_debt_after`.

Comparative validation — no-huddle versus periodic versus flat-triggered
versus hierarchical-triggered synchronization — is implemented as a
deterministic programme in `playbook-sim::huddle_study` and asserted by
`cargo test -p playbook-sim --offline huddle_study`. Two scenarios run under
all four regimes with an identical policy, plane, roster and drive loop; the
regimes differ only in when a huddle may run and who attends.

Results (total synchronization-plus-drive tokens; completion):

| Scenario | A control | B periodic | C flat-triggered | D hierarchical-triggered |
| --- | --- | --- | --- | --- |
| `flaky_integration` | incomplete, 8,580 tokens, 19 repeated failed plays | completed, 2,260 | completed, 2,140 | completed, 1,840 |
| `clean_delivery` | completed, 720 (no huddles) | completed, 720 | completed, 1,020 | completed, 870 |

Reading: on divergence-prone work the ordering is D > C > B — triggered
huddles beat the periodic schedule because they react to the measured field
instead of the clock, and the hierarchical participation floor pays fewer
synchronization tokens than flat attendance for the same reconciliation. The
control regime never completes: it stalls between failing retries until the
budget or the step cap ends the drive. On trivially independent work the
control regime leads — the success-conditioned trigger is the only one that
fires, and its synchronization cost is pure overhead — which is the expected
`A` win the hypothesis reserves for drives that never diverge.

The result is a simulation finding under fixed deterministic scenarios, not a
live-drive benchmark; the Live Lucid integration phase re-runs the same
measures against real drives before any production claim.
