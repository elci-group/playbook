# Playbook Evidence, Journal, and Replay

## Purpose of this document

This document is written for reliability engineers. It answers two questions
that every other guarantee in Playbook depends on: how the system knows what
was true, and whether it can prove it again. It covers the evidence model
(what counts as knowledge and what does not), the journal (where knowledge is
written durably), replay (how a journal is turned back into state, and how
the two modes of doing that must agree), win declarations under replay (how a
claim of completion is checked against measured state rather than believed),
and failure and recovery semantics (what is preserved when things go wrong).

The anthology summarises; the contracts govern. The normative sources for
this document are `crates/playbook-core` (evidence, canonicalisation, and the
game contract), `crates/playbook-journal`, `crates/playbook-state` (the fold),
`crates/playbook-live` (dispatch and reconciliation), `crates/playbook-cers`,
and `docs/live-runtime.md`. Where this document and the code disagree, the
code wins.

## Evidence sovereignty

Evidence in Playbook is sovereign: it is not a report about reality, it is a
cryptographically addressed fragment of reality that anyone can re-verify
without trusting the writer.

**Sealed identity.** Every piece of evidence is sealed with `Evidence::seal`,
which takes a class, a JSON object body, and an observation timestamp, and
produces a digest with the SHA-256 function applied over the *canonical*
serialisation of the content. Canonicalisation (`playbook-core::canonical`)
recursively sorts object keys before serialising, so the digest is a property
of the content, not of the JSON formatting that happened to be in play. The
unit test in `canonical.rs` states the property directly: object key order
does not change the digest. Two observers who saw the same fact agree on the
same digest; an edited body no longer matches its digest and is detected, not
trusted.

**The claim/observation/evidence hierarchy.** A model statement is a claim
until independently confirmed. When a provider responds, `ProviderPlane`
packages the response as a claim — its digest travels as an artefact
reference — and separately seals the invocation itself as `ToolInvocation`
evidence. The claim describes what the model says happened; the sealed
invocation records what actually entered and left the tool plane. The
constitutional rule from `docs/live-runtime.md` is blunt: a model response is
a claim until independent evidence validates it, and nothing in the runtime
weakens that for convenience.

**The evidence class taxonomy.** `EvidenceClass` enumerates the kinds of
knowledge the system recognises: `CompilerResult`, `TestReport`,
`ArtefactValidation`, `ToolInvocation`, `Benchmark`, `StaticAnalysis`,
`RepositoryMutation`, `GateTransition`, `CapabilitySnapshot`,
`EnvironmentSnapshot`, and `NarrativeClaim`. Three of these —
`NarrativeClaim`, `GateTransition`, and `CapabilitySnapshot` — are bookkeeping
or testimony. `EvidenceClass::can_satisfy_gate` returns false for exactly
those three, and nothing else.

**Which classes can satisfy gates.** Gates move only on sealed evidence or on
an explicit waiver (a `WaiverGranted` journal event naming its authority and
reason). `playbook-gates::apply_evidence` gives narrative claims no gate
weight, and the contract verifier enforces the same rule a second time,
independently of policy: `EvidencePolicy::accepts` short-circuits on
`can_satisfy_gate`, so even a contract whose policy naively lists
`NarrativeClaim` cannot satisfy a win condition with it. The test
`evidence_policy_and_narrative_rejection_bind` in `contract.rs` pins both
halves: a policy mismatch rejects valid evidence of the wrong class, and a
policy that names narrative still rejects the claim.

## The journal

**One write path.** All state change in a drive flows through `DriveEvent`s
pushed by the drive authority. There is no second mutation path: the
authority validates, pushes, and the fold does the rest. `playbook-journal`
is the durable write-ahead sink under that path.

**Write-ahead sink semantics.** The journal writes one newline-terminated
JSON record per event — `{"seq":N,"digest":"<sha256>","event":{...}}` — where
the digest covers the canonical serialisation of the event and `seq` starts
at 1 and increases by exactly one per record. A record that is short,
unparseable, out of sequence, or digest-mismatched ends the valid history at
that point. Recovery keeps the longest valid prefix and truncates everything
after the first bad record, reporting `repaired_tail` so the operator knows a
truncation happened. A torn tail degrades the journal to a shorter honest
history; it never fabricates one.

**Subsystem ledgers are projections.** Huddle transcripts and blitz race
events are journalled on the same drive event log and folded by the same
`fold` function as everything else; they are views of one history, not
parallel histories.

**The fixture doctrine.** The transcripts under
`crates/playbook-huddle/tests/fixtures/` and `crates/playbook-blitz/tests/fixtures/`
are full drive logs built through the real authority and the real race
engine, so every event in them is one the fold accepted. They are regenerated
by an explicitly invoked test (`regenerate_huddle_fixtures`,
`regenerate_blitz_fixtures`), and the checked-in test folds each fixture and
asserts its scenario marker — which is what keeps the fixtures honest. The
consequence for reliability work is strict: if a reducer, a generator, or an
event schema drifts such that a checked-in fixture no longer folds to its
recorded outcome, the test suite fails. Generator drift is a test failure,
by construction, not a silent rewrite of history. The closure campaign
records the stronger property these tests protect: replay is proven by
checked-in fixtures that must fold identically under audit and resimulate.

## Replay

**Two modes, one answer.** `fold(events, policy, ApplyMode::Audit |
Resimulate)` rebuilds a `DriveState` from an event list. `Audit` re-derives
state for inspection; `Resimulate` re-runs history as if it were happening
now. The regression fixtures assert that both modes agree on the same log —
a state that differs between audit and resimulate is a defect, not a mode
semantics. Every event application validates: replaying a ledger reproduces
the same state or refuses.

**Recovery from journal.** On restart, a host opens the journal, recovers the
longest valid prefix, folds it, and continues from the derived state. Because
records are digest-chained to canonical bytes, recovery requires no trust in
the writer — a corrupted record is excluded, and the fold of the surviving
prefix is the truth.

**Idempotency and reconciliation.** Live work is at-least-once, never blind.
Every execution envelope carries an idempotency key — an envelope without one
is refused outright — and the dispatch ledger carries the v0.2 vocabulary:

```text
invocation persisted → dispatch attempted → outcome accepted/rejected
                                             ↘ effect unknown → reconciled
```

The one hard rule: no retry occurs merely because the caller did not receive
a response. A lost response is `EffectUnknown`; it leaves that stage only
through reconciliation, which asks the effect what actually happened. An
effect proven not to have occurred may be re-persisted under the same
idempotency key (at-least-once, with attempts counted on the dispatch
record); an effect that occurred is never re-persisted, because idempotency
blocks a second one.

Ambiguity has a first-class representation: `SideEffectNote` carries
`occurred: Option<bool>`, and `None` is the ambiguity marker. An unresolved
note keeps the execution open — the drive authority refuses completion and
abandon while an execution is open, and a recovering host must reconcile the
note before further execution. Ambiguous effects are never auto-resolved; the
fault-injection suite asserts that a `occurred: None` note reaches the
authority as ambiguity and stays there until something asks the question and
answers it. The fold-side counterpart is `SideEffectReconciled { key,
occurred }`: reconciliation with `occurred: true` commits the idempotency key
so the effect can never be repeated, and with `occurred: false` releases the
key so the work may legitimately be re-persisted.

Cancellation follows the same conservatism. Adapters report one of six
states; `cancelled_during_execution`, `cancellation_requested`, and `unknown`
keep the invocation in `EffectUnknown` with spend reserved, because partial
effects may exist and a late result may still arrive. Avoided spend is booked
only for clean states.

## Win declarations under replay

Completion is a verified verdict, not a report.

**The sealed `GameContract`.** A game's meaning of "won" is a sealed
contract: the objective, individually addressable win conditions, loss
conditions, the budget envelope, an evidence policy, an optional deadline,
and a `predecessor_hash`. Sealing requires at least one win condition, at
least one mandatory win condition (a contract with none could never be won
and could never lose honestly), and unique condition ids; it then computes
`contract_hash` as the SHA-256 digest over the contract's canonical bytes
with the hash field cleared. `verify` re-derives the digest and fails with
`DigestMismatch` on any tampering — the content-addressed hash makes the
contract self-verifying on every replay.

**Amendments name their predecessor.** Contracts are never mutated. An
amendment produces a new sealed contract whose `predecessor_hash` names the
contract it supersedes, so the amendment chain is itself a verifiable
history.

**DriveCompleted is refused unless it verifies.** The authority's completion
path checks the mandatory gate ladder and zero yards-to-go; if they do not
hold, it journals a `CompletionRejected` event with a reason and the drive
continues — a false completion claim is a journalled refusal, and the folded
state counts such claims explicitly. The fold enforces the same invariant
from the other side: a `DriveCompleted` event cannot be applied while an
execution is open, or when gates or progress do not warrant it, or — when a
sealed contract governs the drive — when `verify_win_conditions` does not
return a verified win. Each condition kind is checked deterministically
against measured state: `GateSatisfied` requires the named gate terminal and
satisfied; `EvidencePresent` requires the class to pass both the contract's
policy and the narrative exclusion, with a minimum count of uninvalidated
pieces; `ArtefactValidated` counts validated artefacts; `EnvironmentFact`
requires the environment snapshot to record the exact key and value. The
same deterministic check on replay yields the same verdict: a win recorded
in the journal is a win the journal can prove again.

## Failure and recovery semantics

**Rejections are preserved, not discarded.** A refused outcome is a fact.
`ToolOutcomeRejected` failures are journaled against the personnel identity
that produced them and survive replay, so the audit trail shows both the
attempt and the refusal. The v0.2.1 demonstration in `docs/live-runtime.md`
is the pattern: a provider answers 5xx, the failure is journaled against the
player's identity, the dispatch ledger closes the invocation as rejected with
no effect, and the Bench substitutes a different empirical identity that
produces the real evidence on the same drive. Nothing is left unreconciled.

**Open executions block forward motion.** While an invocation sits in
`EffectUnknown`, the drive holds an open execution: no completion, no
abandon, no further mutation through the affected seam. This is the price of
honesty about distributed effects, and it is enforced at fold time, so a
replayed history cannot quietly skip over an unreconciled ambiguity.

**CERS: failed cognition is harvested, not mourned.** Inference bought with
budget that fails burns the budget twice unless the failure is harvested —
once when the attempt fails, again when the next drive repeats it blind.
`playbook-cers` makes the second burn optional. Every materially failed play,
every closed blitz, and every provider outage yields a structured
`RecoveryArtefact` — measured facts, disproved assumptions, surviving partial
work, a reproduction path, ruled-out hypotheses — and never a claim of
success. Harvesting is deterministic: the same failure record produces the
same artefact byte for byte, id included. Artefacts are content-addressed
(the id is the digest of the artefact's own canonical bytes, with a context
digest over the harvested content), so tampering is detected, never trusted.
Consumption is scoped: a recovery packet hands later personnel the residue
without the failed context, and a packet carrying competitive metadata is
refused outright (`AmnesiaLeak`) rather than redacted. The store keeps
recovered patterns sorted, matched by hunt hint, and measures how often the
institution actually reuses what it recovered — because a recovery artefact
that is never replayed into later work is shelf decoration, not memory.

## Cross-references

- `docs/closure-campaign.md` — the gate map naming where replay, evidence,
  and recovery guarantees live, and the sprint order that closed them.
- `docs/live-runtime.md` — the dispatch ledger, cancellation states, and the
  v0.2.1 substitution demonstration.
- `crates/playbook-core/src/contract.rs` — `GameContract`, win and loss
  conditions, and the deterministic verdict.
- `crates/playbook-core/src/canonical.rs` — canonical JSON and the SHA-256
  identity function everything else is built on.
- `crates/playbook-journal/src/lib.rs` — the write-ahead record format and
  prefix recovery.
- `crates/playbook-state/src/apply.rs` — `fold`, `ApplyMode`, and the fold
  time refusals that hold under replay.
- `crates/playbook-live/src/dispatch.rs` — the dispatch stage vocabulary and
  reconciliation rules.
- `crates/playbook-cers/src/lib.rs` — the harvesting doctrine and the
  content-addressed recovery store.
- Anthology companions: [00 Executive overview](00-executive-overview.md),
  [02 Authority and governance](02-authority-and-governance.md),
  [04 Operations runbook](04-operations-runbook.md), and
  [07 Glossary and event index](07-glossary-and-event-index.md).
