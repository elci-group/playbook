# Playbook Security and Boundary Model

## Purpose of this document

Playbook is a deterministic, evidence-governed organisational control plane for
heterogeneous AI and tool personnel. A security reviewer asking "what can the
personnel touch, and what can they never touch?" needs more than a threat
model written in prose: they need to see where every boundary is enforced in
code, what is refused outright, and — just as importantly — what is *not*
enforced today. This document is that inventory.

The design stance can be stated in four rules, each of which is developed in a
section below:

1. **Authority is granted, never inferred.** A live invocation receives a
   complete `ExecutionEnvelope`; an envelope that fails validation is refused,
   never completed by guesswork (`playbook-live::envelope`).
2. **Information boundaries are checked on the serialized artefact, at the
   boundary it crosses.** Competitive metadata and secrets are refused by
   scan, not by convention.
3. **The control plane itself is conservative infrastructure.** Unsafe code is
   forbidden workspace-wide, dependencies are a pinned handful, and the test
   gate runs offline.
4. **The system assumes its workers may lie.** The `FaultPlane` scripts the
   attacks — false success claims, partial reports, duplicated results — and
   the constitution must hold *by construction*, not by worker good behaviour.

Where this anthology summarises and a contract document or the crate-level
documentation disagree, the contract wins. The normative source for the live
material here is `docs/live-runtime.md`.

## The least-authority execution model

### The side-effect class taxonomy

Every play execution carries exactly one `SideEffectClass`
(`playbook-core::domain`). The ladder is ordered by blast radius, and a
drive's policy names the classes it permits:

| Class | Meaning | Typical authority |
|---|---|---|
| `ReadOnly` | Observation only; no state changes | `discover`, `git rev-parse`, measurement |
| `LocalMutation` | Writes confined to the invocation's own working area | scratch builds, temporary artefacts |
| `RepositoryMutation` | Writes to the shared repository state | commits, branch updates |
| `RemoteMutation` | State changes on a remote system | API writes, issue updates |
| `Deployment` | Moves artefacts into serving positions | release, rollout |
| `Destructive` | Irreversible or removal effects | deletion, teardown |
| `Privileged` | Escalated authority (credentials, administrative control) | secret-touching operations |

Authorization lives in `playbook-security::authorize`, and the seam is
deliberately tiny: it reads the policy allowlist and the requested class, and
returns `Ok` only if the class is in the allowlist. It has **no budget
parameter** — the crate's own test is named
`low_budget_does_not_create_a_deployment_permission`, and the doc comment
states the principle: a third-down pass cannot spend its way past it. Wealth
never converts into authority.

### Envelope validation: what is checked, what is refused

The `ExecutionEnvelope` is the whole of one invocation's authority: execution
identity, the authority-cleared play execution, personnel, objective and
drive, `allowed_operations`, `allowed_side_effects`, the expected result
schema and evidence classes, a `budget_envelope`, a timeout, and — for
repository-bound work — a `working_directory`. The struct is
`#[serde(deny_unknown_fields)]`: an envelope carrying fields the schema does
not know is itself refused at deserialization.

`ExecutionEnvelope::validate(&policy_side_effects)` performs eight checks, and
every refusal names the missing or exceeding authority. Nothing is defaulted:

1. **Idempotency key present.** A live invocation without one is refused with
   `Inadmissible`: live work must be reconcilable before it is dispatchable.
2. **Side effect within the drive policy.** `authorize` is called against the
   policy's class list — the envelope's own say-so is not sufficient; the
   security seam independently holds the line.
3. **Side effect within the envelope's grant.** An envelope that does not
   list the execution's own class is refused ("envelope does not grant …").
4. **Operation within the envelope.** An operation outside
   `allowed_operations` is refused even though the play itself would allow it.
5. **Budget fits the play's ceiling.** `budget_envelope.fits_within`
   enforces the one-directional rule: an envelope can narrow authority, never
   widen it. Overreach returns `BudgetCeiling`.
6. **Evidence is named.** An empty `expected_evidence` list is refused — an
   invocation that cannot say what evidence it owes does not run.
7. **Timeout is nonzero.** Hope is not a deadline mechanism.
8. **Mutating work is place-bound.** A working directory is mandatory unless
   the side effect is `ReadOnly`; a mutating invocation cannot be
   location-free.

The executor never supplies a missing field itself. This is the anti-pattern
under review: in most agent frameworks, a missing permission is an
inconvenience the runner papers over. Here it is a refusal by name.

One adjacent accounting rule belongs to the security story because it removes
an incentive to falsify: `CostReport::reserved` books unknown spend
(tokens, monetary cost) **at the envelope ceiling**, never as zero, with the
companion flags `tokens_known` / `monetary_cost_known` / `provider_reported`
kept honest. A worker cannot make its cost disappear by not reporting it.

### The ToolPlane allowlist: fixed command lines, policy owns the arguments

`ToolPlane` (`playbook-live::toolplane`) is the first non-fixture capability
plane: real commands in a real working directory. Its authority model is an
allowlist of `ToolCommand` entries — `program`, `args`, `evidence_class`,
and an optional target `gate`. Two properties do the security work:

- **An operation not on the allowlist is refused, never improvised.** The
  refusal is a `ToolFailureKind::Unsupported` failure whose diagnostic names
  the operation and the allowlist. An empty allowlist is a plane that can
  only refuse.
- **Policy owns the command line.** The invocation may choose *which*
  allowlisted operation to run; it may not choose or extend the arguments.
  There is no string-interpolated shell command anywhere in the plane —
  arguments are the fixed vector from the policy, executed directly.

The plane enforces the invocation's deadline itself (poll-wait, then kill: a
command that outlives its deadline is reported as a `Timeout` failure whose
effect is unknown), captures the real exit code and output, and seals the
outcome as the evidence class the play demanded. `discover` observes the
allowlist without executing anything, so even inventory is a read-only act.

## Information boundaries

Playbook's sharpest non-obvious boundary is competitive: personnel differ in
provider and model, and the system refuses to let one model's identity,
self-image, or ranking metadata flow to another. The rule is "amnesia, not
dishonesty" — later personnel receive the measured residue of a drive, never
the narrative of who ran it before.

### The forbidden key list

`playbook_core::forbidden_competitive_keys()` is the exhaustive, test-exercised
list of what may never serialize across the boundary:

`previous_model_identity`, `previous_model_self_assessed_confidence`,
`previous_model_ytg_prediction`, `other_candidate_model_predictions`,
`competitive_ranking`, `previous_model`, `previous_ytg`, `nominal_ytg`,
`competitor_assessments`, `route_rank`, `selection_history`,
`coordinator_notes`, `model_reputation`, `conversation`, `own_ytg_milli`.

`leaks_competitive_metadata(bytes)` scans the serialized text for any of
these keys. Note what the list protects in both directions: the successor
must not learn who preceded it or how that predecessor rated itself, and the
successor must not carry its *own* self-assessed yardage (`own_ytg_milli`) —
progress is `measured_ytg_milli`, validated by evidence, or it does not
travel.

### Where the checks live

- **Continuation packages refuse to leak at sealing time.**
  `ContinuationPackage::seal_unchecked` (`playbook-core::domain`) serializes
  the package to canonical bytes, leak-checks them, and refuses with
  `PlaybookError::AmnesiaLeak` *before* computing the `integrity_sha256`
  digest. The integrity digest is therefore only ever computed over a clean
  package — a leaking package cannot exist in sealed form.
- **`playbook-lastdown` re-checks the sealed artefact.**
  `package_leaks` re-serializes the finished `ContinuationPackage` and rescans
  it. The tests `seal_package_preserves_measured_state_and_excludes_competitive_keys`
  (asserts `measured_ytg_milli` survives while every forbidden key is absent
  from the serialized bytes) and `top_level_competitive_metadata_is_detected_in_serialized_json`
  (injects `previous_model_identity` and `own_ytg_milli` into the serialized
  JSON and asserts detection) are the regression fence for the whole list.
- **The fold refuses a leaking punt.** In `playbook-state::apply`, the
  `PuntIssued` event handler leak-checks the incoming package and rejects the
  event with `AmnesiaLeak`; in `ApplyMode::Audit` it additionally verifies the
  package matches the field's sealed package, failing replay with
  `ReplayDivergence` on mismatch. The journal is thus a second, independent
  enforcement point — the boundary holds even if a caller bypassed the
  sealing path.
- **CERS packets are leak-checked before handoff.** `playbook-cers::packet`
  builds the scoped consumable packet (bounded lists in harvest order, plus
  failure class, reuse hint, and digests), serializes it, and refuses with
  `AmnesiaLeak` if it leaks. The module's own statement of scope: "Scoping
  never decides what is true; it only decides what leaves."
- **The simulation and CLI treat a leak as a failed report.** `playbook-sim`
  re-exports `package_leaks`, reports `package_leaks` in every run report,
  and fails the run on it; the CLI's report gate fails on
  `package_leaks` exactly as it fails on replay mismatch.

### Secrets out of logs, contracts, packages, scouting records, and CERS artefacts

The system carries *references* and *digests*, not payloads: evidence travels
as sealed bodies with digest chains, artefacts as `ArtefactRef`s with content
digests, packages as measured state plus refs. The structural consequence is
that none of the serialized surfaces — event logs, Front Office contracts,
Lastdown packages, scouting records, CERS artefacts — has a field in which a
credential could be conventionally placed, and every surface that crosses a
trust boundary is scanned on the way out. Provider credentials are never
accepted by any crate type; where they exist at all, they exist in the host.
(See *Residual risks* for the honest limit of this.)

## Memory safety and supply chain

- **Unsafe code is forbidden workspace-wide.** The root `Cargo.toml` carries
  `[workspace.lints.rust] unsafe_code = "forbid"`, every member crate opts in
  with `[lints] workspace = true`, and `playbook-security` additionally
  declares `#![forbid(unsafe_code)]` inline. There is no `unsafe` block in
  the workspace to audit, and the compiler — not a review guideline — holds
  that line.
- **No floats in control-plane math.** The domain vocabulary is integers:
  milli-yard units (`measured_ytg_milli`, `expected_ytg_reduction_milli`),
  parts-per-million probabilities (`p_preserve`, `remaining_fraction_ppm`),
  micro-dollar costs (`monetary_cost_micros`). A scan of the crates finds no
  floating-point control-plane arithmetic. This is a security property, not
  merely a style preference: determinism is what makes the journal fold,
  replay equality, and digest comparison meaningful, and IEEE-754 non-
  associativity is a classic source of replay divergence.
- **A pinned, minimal dependency surface.** All dependencies are declared in
  `[workspace.dependencies]` and consumed by `.workspace = true` reference;
  the entire third-party surface is four crates — `serde`, `serde_json`,
  `sha2`, `thiserror` — with everything else an in-workspace path
  dependency. `publish = false` keeps the workspace from being fragmented
  into independently versioned crates.
- **The offline test gate.** The suite is designed to prove its claims with
  the network unplugged: the fixture-replay gates fold checked-in journals
  identically under audit and resimulate; the v0.2.0 live gate
  (`playbook-live/tests/live_runtime.rs`) runs a real `git rev-parse HEAD`
  against the actual repository, seals the observation, and accepts it
  through the ordinary evidence path — no external service involved; and the
  Front Office enforces an `offline` capability flag with
  `require_offline_fallback`, refusing a roster that has no offline fallback.
  The deliverable gate additionally forbids unfinished-work markers
  (placeholder macros and their comment cousins) in shipped sources.
  An attacker who controls the network can therefore not flip a test
  outcome.

## Adversarial hardening

### The FaultPlane fourteen-fault table

`FaultPlane` (`playbook-live::faults`) scripts the directive's required fault
list over any plane, consumed in script order; once the script is exhausted
the inner plane runs untouched. Happy-path orchestration is insufficient:
these are the faults the live runtime must survive *by construction*. The
table below is the documented state transition, reproduced from `faults.rs`:

| Fault | Plane answer | Dispatch stage | Authority consequence |
|---|---|---|---|
| `HttpTimeout` | `Failed(Timeout)` | EffectUnknown | reconcile; no blind retry |
| `ProviderRateLimit429` | `Failed(RateLimit)` | DispatchAttempted → outcome rejected | back off; retry only per policy |
| `ProviderServerError5xx` | `Failed(Environment)` | DispatchAttempted → outcome rejected | substitute or retry per policy |
| `ConnectionLoss` | `Failed(Transient)` | EffectUnknown | reconcile the effect occurrence |
| `ToolProcessDeath` | `Failed(ToolBug)` | EffectUnknown | partial effects possible: reconcile |
| `MalformedResponse` | `Failed(InvalidInput)` | DispatchAttempted → rejected | no effect booked |
| `PartialOutput` | `Done` with truncated result | OutcomeAccepted-shaped | evidence scoring must reject the incomplete report |
| `CostUnavailable` | `Done` with unreported spend | OutcomeAccepted | spend reserved at the ceiling, never zero |
| `DuplicateResult` | `Done` replayed | second commit | idempotency refuses the double effect |
| `LateResultAfterCancellation` | `Done` after cancellation | OutcomeAccepted | the completed-before-cancel rule wins |
| `StaleAgentState` | `Failed(StaleCapability)` | DispatchAttempted → rejected | refresh capability generation first |
| `IncorrectSuccessClaim` | `Done` with a narrative claim | OutcomeAccepted-shaped | narrative satisfies no gate; completion refused |
| `RepositoryConflict` | `Failed(Environment)` with unresolved side effect | DispatchAttempted → rejected | reconciliation required before further mutation |
| `ReconciliationAmbiguity` | `Failed` with conflicting effect notes | DispatchAttempted → rejected | manual reconciliation; ambiguity is never auto-resolved |

Read the table as an attacker would. Five faults arrive success-shaped
(`PartialOutput`, `CostUnavailable`, `DuplicateResult`,
`LateResultAfterCancellation`, `IncorrectSuccessClaim`) precisely so that the
authority's scorer, accounting, idempotency, and cancellation paths — not the
worker's honesty — are the mechanisms that must catch them. Four faults
(`HttpTimeout`, `ConnectionLoss`, `ToolProcessDeath`,
`ReconciliationAmbiguity`) leave the effect occurrence unknown, and the
`leaves_effect_unknown` predicate keeps that set exact. `RepositoryConflict`
carries an unresolved `SideEffectNote` with `occurred: Some(true)` that blocks
further mutation; `ReconciliationAmbiguity` carries `occurred: None`, and the
authority must reconcile by asking, never by guessing.

Every injected failure names itself in its diagnostic ("injected fault: …") —
an injected fault that hides would be worse than none.

### The conservative cancellation rule: unknown means reconcile

Adapters report one of six cancellation states:
`cancelled_before_execution`, `cancelled_during_execution`,
`cancellation_requested`, `non_cancellable`, `completed_before_cancel`, or
`unknown`. The conservative rule binds them: `cancelled_during_execution`,
`cancellation_requested`, and `unknown` keep the invocation in
`EffectUnknown` — the spend stays reserved and the invocation stays
reconcilable, because partial effects may exist and a late result may still
arrive. Avoided spend is booked only for clean states. Booking avoided cost
on fiction is how budgets learn to lie, and a budget that lies is a control
surface an attacking worker can lean on.

### False success claims: the constitution survives an attacking worker

The `IncorrectSuccessClaim` fault is the sharpest entry in the table, and its
handling is worth stating precisely. The worker returns `Done` with status
`Succeeded` and one piece of evidence: a `NarrativeClaim` whose body says "the
work is complete and correct". The defence is layered and structural:

- `EvidenceClass::can_satisfy_gate()` is false for `NarrativeClaim` (as well
  as `GateTransition` and `CapabilitySnapshot`): a claim is not the class of
  thing that can satisfy a gate, so the answer does not depend on what the
  claim *says*.
- `playbook_core::narrative_has_no_weight` encodes the constitution as a
  predicate: narrative carries no gate weight, by construction. The faults
  test asserts both on the sealed claim.
- Completion is refused: a gate that needs a `TestReport` needs the scored
  shape — the scorer in the fault tests requires keys like `exit_code` and
  `passed`, and `PartialOutput`'s truncated body fails that scorer the same
  way the authority's ingest scorer would.

The consequence for review: a compromised or hallucinating worker can waste
budget and create reconciliation work, but it cannot promote itself. The
gate is satisfied by independently validated evidence or it is not satisfied;
"the worker said so" is not an input the authority possesses a pathway to
accept.

## Residual risks and the honest boundary

A security document that claims full closure is itself a false success
claim. The following are known, deliberate, and unenforced today:

- **The live provider transport is host-supplied.** Provider-specific
  behaviour lives behind `ModelTransport`, never inside the authority,
  reducers, evidence logic, routing, or the Front Office; a host wires its
  provider client by implementing one trait, and `UnimplementedTransport`
  refuses with directions rather than pretending. But the transport itself —
  the code that talks to the provider, and whatever it logs — is outside
  this workspace and outside these guarantees. Playbook packages the
  transport's responses as claims with digests; it cannot vouch for the
  transport.
- **ToolPlane isolation is an allowlist of commands, not an OS sandbox.** The
  working directory is bound at the envelope (mutating invocations must be
  bound to one), commands are fixed by policy, and deadlines are enforced by
  the plane. There is no namespace, seccomp, chroot, or capability-drop
  wrapper in the plane: an allowlisted program runs with the executor's OS
  privileges and can read what that user can read. Hosts deploying against
  hostile inputs should wrap the executor in an OS-level sandbox; the
  allowlist constrains *which* programs run, not what a program can reach
  once running.
- **Secret storage is the host's responsibility.** No Playbook crate accepts
  or stores credential material. The serialized surfaces are scanned for
  competitive metadata, and the CERS packet boundary additionally scans for
  credential-shaped material (`api_key=`, `bearer `, `password=`,
  `private_key=`, `authorization:`, and relatives): a harvest fact that
  embeds a credential value refuses the packet with `AmnesiaLeak` before it
  can cross the handoff. The scanners are substring scans over fixed
  needle lists, not a semantic classifier — they reliably catch the listed
  needles (the tests prove it, including injected keys and credential-shaped
  facts), and they are not a general-purpose DLP engine: a novel key shape,
  or a secret in a form the needles do not match, is not something the scans
  promise to catch. Hosts must still keep credentials out of the inputs they
  hand to envelopes, package drafts, and harvest payloads in the first
  place.
- **Two scanner-corollary notes for reviewers.** First, the forbidden-key
  list is versioned source: extending it changes what sealing accepts, so it
  should change only with a test that exercises the new key. Second, the
  envelope's `working_directory` is a string handed to the plane; path
  canonicalization and containment within an intended root remain deployment
  concerns, not enforced ones.

None of these gaps weakens the internal constitution — evidence gates,
idempotency, leak refusal, and the allowlist hold regardless. They mark
where Playbook's guarantees end and the hosting environment's begin, and
they should be re-read whenever the deployment story changes.

## Cross-references

- `docs/live-runtime.md` — normative source for the envelope, cost
  reservation, durable dispatch, cancellation states, the ToolPlane, the
  provider seam, and fault injection.
- `docs/franchise.md`, `docs/huddles.md`, `docs/blitz.md` — the contract
  documents for roster activation, scoped context packets, and escalation;
  the anthology summarises, the contracts govern.
- `docs/closure-campaign.md` — the gate map recording where each guarantee
  (replayable journal, evidence validation, Lastdown, CERS) was built and
  tested.
- `docs/anthology/02-authority-and-governance.md` — who may decide what;
  this document describes what a decided execution may touch.
- `docs/anthology/03-evidence-journal-replay.md` — the digest chains and
  fold mechanics the leak checks and evidence gates depend on.
- `docs/anthology/04-operations-runbook.md` — the operational view of the
  dispatch ledger and reconciliation paths named in the fault table.
- `docs/anthology/07-glossary-and-event-index.md` — definitions for
  `EffectUnknown`, `SideEffectClass`, evidence classes, and the events
  referenced here.
- `crates/playbook-security/src/lib.rs` — the `authorize` seam and its
  no-budget-parameter test.
- `crates/playbook-live/src/{envelope,toolplane,faults,dispatch,cancel}.rs` —
  envelope validation, the command allowlist, the fault table, and the
  cancellation vocabulary.
- `crates/playbook-core/src/{domain,error}.rs` — the side-effect taxonomy,
  `forbidden_competitive_keys`, `leaks_competitive_metadata`, package
  sealing, and `PlaybookError::AmnesiaLeak`.
- `crates/playbook-lastdown/src/lib.rs`, `crates/playbook-cers/src/packet.rs`,
  `crates/playbook-state/src/apply.rs` — the leak checks at package sealing,
  CERS handoff, and journal fold respectively.
