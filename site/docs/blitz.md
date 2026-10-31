# Blitz

`playbook-blitz` is the defence subsystem of the drive authority. A blitz is
defensive parallel single-focus inference: under material defensive pressure —
an outage, an imminent deadline, failing power, quota or network, a critical
unpatched CVE — Playbook dispatches multiple independently operating workers
against one frozen, content-addressed target and races their candidate
artefacts through deterministic verification. The governing principle is
economic: when the expected cost of delay exceeds the expected marginal cost
of redundant inference, Playbook may deliberately purchase time with parallel
inference. The first independently verified candidate wins, never the first
response; redundant inference is cancelled the moment the win condition is
satisfied; every ceiling the race operates under is a policy envelope, and
the whole event is recorded in the drive ledger.

A blitz is not generic parallelism, a swarm, a huddle, voting, an ensemble, a
retry or a model fallback. It is not generic parallelism because the work is
one problem, one winner, one deadline. It is not a swarm because nothing
emerges: admission, envelopes, waves and closure are all decided events. It
is not a huddle because nobody reconciles anything during the race — workers
never see each other. It is not voting because agreement confers nothing;
verification does. It is not a retry because every worker attacks the same
frozen target from the start, not the same line again. It exists to purchase
verified recovery latency, and to stop spending the moment the defensive
objective is met.

## Pressure and eligibility

The measured situation is `DefensivePressure`: severity, deadline,
deterioration, blast radius, solver uncertainty, containment confidence,
recovery confidence, resource decay and expected delay loss. The contract —
distinguishing difficult from urgent — may not change even as the
representation evolves. A difficult problem deserves better personnel; an
urgent one deserves concurrent personnel. Difficulty never admits a blitz by
itself.

| Class | Who decides | What admits it |
| --- | --- | --- |
| `instinctive` | deterministic policy | a predefined condition holds; inference is not in the loop |
| `strategic` | inference proposes, policy authorises | urgency and pressure scores clear their floors |

An instinctive blitz fires on a named condition and carries a machine reason
code. The codes, most severe first:

| Reason code | Condition |
| --- | --- |
| `critical_cve_unpatched` | a critical CVE is present in a loaded component and no patch is verified |
| `outage_rollback_failed` | an outage rollback was attempted and failed |
| `health_floor_rollback_failed` | component health sits below `production_health_floor_ppm` (300,000) and its rollback failed |
| `power_critical` | battery is below `battery_critical_ppm` (150,000) |
| `quota_exhaustion_imminent` | remaining provider quota is below `quota_critical_ppm` (100,000) |
| `connectivity_loss_imminent` | connectivity is lost or demonstrably about to be |

A strategic blitz starts as an urgency proposal from inference and must still
pass deterministic authorisation. The pressure score is a weighted sum of
seven clamped 0..=1000 components — severity 300, deadline pressure 300,
deterioration 200, blast radius 150, solver uncertainty 250, resource decay
200, delay loss 300 — divided by 1000 and clamped. Policy admits the
expenditure only above both floors: urgency at `min_urgency_score` (300) and
pressure at `min_pressure_score` (500). The same state and policy always
produce the same decision, and a refusal is a journaled event, so the audit
log shows why the defence did not spend.

## Target freeze

Every worker operates against a `TargetPackage`: problem statement, evidence
references, artefact references, constraints, forbidden actions, expected
output, win condition, verification gates and a deadline. `TargetPackage::seal`
refuses a package without a problem statement, a win condition or at least one
verification gate, orders the gates by rank, and seals the package
content-addressably: the SHA-256 over its canonical bytes becomes
`target_hash`. `verify` recomputes the digest and refuses any mismatch. The
hash is the proof that all workers were independently attempting the same
problem; workers receive semantically equivalent packages, and the hash says
so.

Single focus is a structural invariant, not a convention. The ledger holds at
most one active blitz — a second formation would silently dilute focus — and
every candidate carries the `target_hash` it was actually attempting. The
fold refuses a candidate attributable to any other problem. The trigger's
evidence bundle is frozen with the target; nothing is added mid-race.

If the defensive problem itself changes under the race, nothing is silently
retargeted. Target drift is handled by supersession: the running blitz closes
as `Superseded` and a new blitz opens against the new hash. The old race's
record, spend and scouting survive in the ledger; the new race gets its own
seal.

## Formation selection

Roster scoring is deterministic. Model quality alone never selects a
defender; expected success trades against speed, independence and
availability, and cost discounts everything:

| Component | Weight |
| --- | --- |
| success | 4 |
| speed | 3 |
| independence | 3 |
| availability | 2 |
| cost | 1 |

A candidate below the `min_worker_score` floor (2,000) is not sent regardless
of prestige. Independence is computed from shared upstream failure domains,
sorted and deduplicated — never from brand names.

Diversity floors apply to the formation as a whole: at least
`min_providers` (2) distinct providers, at least `min_model_families` (2)
distinct model families, and at most `max_same_family` (3) workers from any
one family. Eight correlated workers are not eight independent attempts:
effective parallelism counts independent attempts, not dispatched workers —
eight correlated workers are roughly 3.7 attempts. Admission refuses a
formation below `min_effective_parallelism_milli` (1,500).

What is scarce biases the formation, never the verification:

| Mode | Formation bias |
| --- | --- |
| `power_critical` | remote workers preferred — local compute is what is failing |
| `network_critical` | local workers preferred — the network is what is failing |
| any other mode | indifferent |

## Envelopes per resource mode

The resource mode selects one of five policy envelopes
(`policy.blitz.budgets`). A consumption delta that would pass any ceiling is
refused; every dimension is monotonically increasing, so the governor cannot
be argued with after the fact.

| Mode | `max_tokens` | `max_cost_microunits` | `max_duration_ms` | `max_workers` | `max_waves` |
| --- | --- | --- | --- | --- | --- |
| `normal` | 40,000 | 4,000,000 | 120,000 | 8 | 3 |
| `latency_critical` | 80,000 | 8,000,000 | 30,000 | 8 | 3 |
| `power_critical` | 20,000 | 2,000,000 | 60,000 | 4 | 2 |
| `network_critical` | 20,000 | 2,000,000 | 60,000 | 4 | 2 |
| `cost_critical` | 10,000 | 1,000,000 | 90,000 | 2 | 2 |

No blitz extends itself. Exhaustion is a closure, not a renegotiation. A
failed verification may be retried at most `max_verifier_retries` (1) times;
a candidate that fails twice has failed.

## The race

Candidates stream. A worker emits a `Candidate` the moment it has a plausible
artefact — arrival immediately makes it eligible for verification. A worker
that sees the shape of the fix at second four must not spend another thirty
seconds writing an essay first. Verification races inference: the verifier
starts as soon as a candidate arrives and runs concurrently with the workers
still racing.

The winner is atomic. Exactly one `SelectedWinner` exists per recovered
blitz; once the corroboration requirement is satisfied, the winner is
committed, later candidates are rejected — never double-committed — and all
redundant workers are cancelled immediately. Cancellation is mandated, not
best-effort: cancellation latency is benchmarked and recorded with the
tokens and cost the stop avoided, and a race that cannot stop its losers has
failed in its own named way.

Corroboration decides when a verified candidate may win. Policy selects one
mode; the builtin default is `first_pass`:

| Mode | Satisfied when |
| --- | --- |
| `first_pass` | one verified candidate exists |
| `two_independent_passes` | verified candidates exist from two distinct workers |
| `n_of_m` | at least `n` verified candidates exist |
| `heterogeneous_confirmation` | two verified candidates come from different providers and different model families |
| `deterministic_only` | one candidate passed only deterministic gates |
| `human_confirmation` | a verified candidate is additionally confirmed by a human |

The first response is never automatically the winner; corroboration is what
makes a first pass eligible to be one.

## Verification hierarchy

A candidate is untrusted until its required gate set passes. Ten gates
exist, ranked so static evidence outranks model adjudication and adjudication
outranks human review:

| Rank | Gate | Sealed evidence class |
| --- | --- | --- |
| 0 | `static_invariant` | static analysis |
| 1 | `compilation` | compiler result |
| 2 | `unit_test` | test report |
| 3 | `regression_test` | test report |
| 4 | `security_test` | static analysis |
| 5 | `integration_test` | test report |
| 6 | `benchmark` | benchmark |
| 7 | `runtime_health` | tool invocation |
| 8 | `independent_adjudication` | artefact validation |
| 9 | `human_review` | narrative claim |

The deterministic gate rule: adjudication and human review are generative.
A gate only they pass never constitutes sufficient evidence on its own, and
`deterministic_only` corroboration accepts only candidates whose every passed
gate is deterministic. Generative self-certification — a model declaring its
own work fixed — is worth nothing alone. An unavailable verifier leaves the
candidate untrusted forever: fail closed.

## Waves and the huddle seam

Waves preserve independence before paying for coherence.

Wave 1 attacks in isolation. Its workers share the frozen evidence and
deterministic tool outputs — never each other's reasoning, never each other's
candidates. No candidate sharing, no cross-agent reasoning: that is what
makes the first wave a measurement of independent solvability. A later wave
may only be dispatched after the previous wave closed, which is what keeps
wave 1 independent of wave 2.

Wave 2 is evidence-enriched. It receives what wave 1 learned the
deterministic way: failure evidence and verifier diagnostics. It still does
not receive wave 1's reasoning.

Wave 3 is the seam. When independence has failed — correlated failures, an
empty verified set — the blitz emits a `HuddleRecommendation`: scope key,
reason code, evidence references, the failure class and the correlated
failure count. The caller maps the recommendation onto an actual huddle
request; the blitz never opens one itself. Huddles never run during wave 1:
coherence is purchased only after independence has had its race.

## Failure taxonomy

Failure classes require different remediation, so they are never collapsed
into a generic failure:

| Variant | Meaning |
| --- | --- |
| `no_candidate` | every worker left the race without emitting a candidate |
| `invalid_candidates` | candidates arrived but none is attributable to the frozen target |
| `verification_failure` | candidates existed and every one failed its gates |
| `deadline_exhausted` | the target deadline passed before a verified winner |
| `budget_exhausted` | an envelope ceiling flipped exhausted mid-race |
| `provider_failure` | upstream provider failures ended the race |
| `tool_failure` | a deterministic tool in the verification path failed |
| `target_ambiguity` | the situation could not be sealed into one frozen target |
| `target_drift` | the defensive problem changed under the race |
| `correlated_failure` | a shared failure domain took out multiple workers at once |
| `cancellation_failure` | redundant workers could not be stopped after the win |
| `recovery_regression` | the verified fix regressed the recovered state |
| `policy_rejection` | policy refused the expenditure at admission |

## State machine and journal events

A blitz advances `Proposed` → `Authorised` → `FreezingTarget` → `Dispatching`
→ `Racing` → `Verifying` → `Recovered` → `Cancelling`, with `Exhausted`,
`Escalating`, `Failed` and `Aborted` as the defensive exits — twelve states
in all. It closes into exactly one `BlitzClosure`: `Rejected`, `Recovered`,
`Exhausted`, `Escalated`, `Superseded`, `Failed` or `Aborted`.

Every transition is a drive event. Persistence and the fold live in
`playbook-state`, which projects drive events onto the blitz event type and
folds them through `apply_blitz`. The fold reconstructs the live blitz, its
budgets, candidates, verifications, cancellations and closures from these
events alone; nothing is a side channel. Replay reproduces the same state or
refuses the log: an out-of-order or unknown blitz event is an error, never a
silent repair.

## Economics and scouting

Three clock measures are kept distinct because they flatter differently:

| Measure | Reads | Answers |
| --- | --- | --- |
| TTFC | `first_candidate_ms` | how fast did anyone propose anything |
| TTFV | `time_to_verified_ms` | how fast did anything pass verification |
| TTR | `time_to_recovery_ms` | how fast was recovery actually measured |

Fast unverified output improves TTFC while TTR burns; the defence exists to
shrink TTR, and only verification moves it.

Two cost measures are derived at closure. `BlitzPremium` is the deliberate
excess: what the race cost beyond what the winning line alone would have
cost — the price actually paid for verified-recovery latency, bounded by the
tokens and cost early cancellation avoided. `CostPerSecondRecovered` puts
that premium on the clock: race cost over recovery seconds. Both fold from
the ledger alone and feed scouting.

Scouting is the point of the ledger. Every dispatched worker produces a
`WorkerScouting` row — personnel, provider, model family, wave, whether it
produced a candidate, whether the candidate verified, tokens, cost, latency —
and cancelled and failed workers are included, because knowing who did not
produce is the point. Raw parallelism, effective parallelism and correlated
failure counts are recorded beside them. These rows feed the front office's
formation fitness alongside its divcon upset statistics and inform the Jose
disclosure seam huddles regulate with, so each blitz makes the next one
better staffed and cheaper.

## Invariants

- Single focus: every worker attempts the same frozen target; a candidate attributable to any other problem is refused.
- Independent first wave: workers share frozen evidence and deterministic tool outputs, never each other's reasoning or candidates.
- Verification before victory: the first response never wins; the first independently verified candidate does.
- Bounded expenditure: concurrency, tokens, money, deadline, waves and retries are policy ceilings; no worker may expand its envelope.
- Early cancellation: recovery verified, redundant inference stops.
- Fail closed: a failed controller, verifier or budget governor never commits an unverified artefact; an unavailable verifier leaves the candidate untrusted forever.
- Evidence preservation: every blitz reconstructs why it was invoked, who played, what each attempted, what won, what was cancelled, what it cost, and whether recovery actually occurred.

## Tov modules

Blitz scoring is split into replaceable Tov modules under `tov/blitz`. The
Rust crate remains the authority: modules are compile-checked, versioned as
source and failure-isolated, and a missing or failing module falls back to
the deterministic Rust scorer.

| Module | Responsibility |
| --- | --- |
| `eligibility` | rank the instinctive trigger reasons |
| `worker_score` | score a roster candidate deterministically |
