# Live Franchise Runtime — the v0.2.x foundations

`v0.1.x` proved Playbook knows how an organisation should behave. This
document records the v0.2.x foundations that ship in `playbook-live`: the
crate where Playbook starts operating one. The constitutional rules are
unchanged — the Front Office stays offline, the franchise owns the work, the
authority stays deterministic, and evidence stays sovereign. A model response
is a claim until independent evidence validates it; nothing in this crate
weakens that for convenience.

## The execution envelope

Every live invocation receives a complete [`ExecutionEnvelope`]
(`playbook-live::envelope`): execution id, the authority-cleared play
execution, personnel, objective and drive, the allowed operations and side
effects, the expected result schema and evidence classes, a budget envelope
that can narrow but never widen the play's ceiling, a timeout, and — for
repository-bound work — a working directory. `validate` refuses any missing
authority by name; the executor never supplies a missing field itself. An
envelope without an idempotency key is refused outright: live work must be
reconcilable before it is dispatchable.

## Cost: unknown is never zero

A live invocation reports a `CostReport`. Where the provider reports spend,
it is booked. Where it does not, `reserved` takes the envelope ceiling for
the unknown dimensions — booking zero would be fiction, and fiction
compounds. Wall clock and tool calls are always measured by the executor
itself.

## Live identity

`LiveIdentity` derives the deterministic personnel id
`live-{provider}-{model}-{profile}-{envelope}`. A change of provider, model,
major profile, or tool envelope is a different empirical player; folding
`provider-A/model-X` and `provider-B/model-X` into one record would compare
incomparable histories. `Occupancy` encodes the default busy rule: an
unresolved invocation means the player is busy — no reassignment unless the
capability explicitly supports safe concurrent occupancy and policy permits
it. The Bench enforces this from both sides: a busy starter is not
substituted until its effect reconciles, and busy members are never selected
as replacements.

## Durable dispatch (at-least-once, never blind)

Playbook does not pretend distributed execution gives literal exactly-once.
`DispatchLedger` carries the explicit vocabulary:

```text
invocation persisted → dispatch attempted → outcome accepted/rejected
                                       ↘ effect unknown → reconciled
```

The one hard rule: no retry occurs merely because the caller did not receive
a response. A lost response is `EffectUnknown`; it leaves that stage only
through reconciliation, which asks what actually happened. An effect proven
not to have occurred may be re-persisted under the same idempotency key —
at-least-once, with attempts counted. An effect that occurred is never
re-persisted: idempotency blocks a second one.

## Cancellation is a six-state answer, and unknown is conservative

Adapters report `cancelled_before_execution`,
`cancelled_during_execution`, `cancellation_requested`, `non_cancellable`,
`completed_before_cancel`, or `unknown`. The conservative rule binds them:
`cancelled_during_execution`, `cancellation_requested`, and `unknown` keep
the invocation in `EffectUnknown` — the spend stays reserved and the
invocation stays reconcilable, because partial effects may exist and a late
result may still arrive. Avoided spend is booked only for clean states;
booking avoided cost on fiction is how budgets learn to lie. This matters
most for Blitz, where cancellation is the point.

## The live tool plane

`ToolPlane` is the first non-fixture `CapabilityPlane`: an allowlist of real
commands bound to a real working directory. An operation outside the
allowlist is refused, never improvised. The plane enforces the invocation's
deadline itself (poll-wait, then kill: a timed-out command is a `Timeout`
failure whose effect is unknown), captures the real exit code and output,
and seals the outcome as the evidence class the play demanded — a full
scored test report, a compiler result, an environment snapshot. `discover`
observes the allowlist without executing anything. This is what closed the
v0.2.0 gate: `playbook-live/tests/live_runtime.rs` runs a real `git
rev-parse HEAD` against the actual repository, seals the observation, and
the drive authority accepts it through the ordinary evidence path.

## The provider seam

Provider-specific behaviour lives behind `ModelTransport`, never inside the
authority, reducers, evidence logic, routing, or the Front Office.
`ProviderPlane` wraps any transport with the constitutional packaging: the
response is packaged as a claim (its digest travels as an artefact
reference), the invocation itself is sealed as `ToolInvocation` evidence,
and reported usage is real spend while unreported usage stays flagged and
reserved. `UnimplementedTransport` is the honest placeholder: it refuses
with directions rather than pretending. A host wires its provider client by
implementing one trait.

## Adversarial fault injection

`FaultPlane` scripts the directive's required fault list over any plane,
each fault with a documented state transition (see the table in
`faults.rs`): transport faults surface as their failure kinds and route to
the correct dispatch stage; a repository conflict carries an unresolved side
effect that blocks further mutation; reconciliation ambiguity carries
`occurred: None` and is never auto-resolved; the partial report arrives
success-shaped so the evidence scorer must be the thing that rejects it; the
incorrect success claim arrives as narrative, which carries no gate weight
by construction; duplicate, late, and cost-unavailable results arrive as
successes so the authority's idempotency, cancellation, and reservation
paths are the ones that must hold them.

## What v0.2.1 demonstrated

`provider_failure_persists_and_the_bench_substitutes_without_losing_the_objective`:
player A's provider answers 5xx — the failure is journaled against A's
identity, the dispatch ledger closes the invocation as rejected with no
effect, the Bench substitutes player B (a different empirical identity per
the identity rules), and B takes the same assignment on the same drive and
produces the real evidence A could not. Nothing is left unreconciled.
