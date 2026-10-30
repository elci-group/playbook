# Playbook Executive Overview

## Purpose of this document

This document is the entry point to the Playbook documentation anthology. It is
written for executive stakeholders — buyers, sponsors, and operators — who must
decide whether to trust and fund Playbook without reading Rust source code. It
answers three questions: what Playbook is, what it guarantees, and how those
guarantees are proven. It also states the maturity boundary and the honest risk
posture.

This document summarises; the contracts govern. Every claim here is grounded in
the repository's normative documents (`docs/franchise.md`,
`docs/closure-campaign.md`, `docs/blitz.md`, `docs/tactics.md`,
`docs/live-runtime.md`, `docs/huddles.md`) and the crates they describe. Where
this summary and a contract disagree, the contract wins. The companion
documents 01–07 carry the depth this overview deliberately omits.

## What Playbook is

Playbook is a deterministic, evidence-governed organisational control plane for
heterogeneous AI and tool personnel, written in Rust. It runs work as
*games* — bounded objectives executed under sealed contracts — staffed by
*personnel*: models, tools, and deterministic procedures. It is not an agent
framework and it does not call a model itself; hosts wire execution through a
capability-plane seam, and Playbook owns the decisions, budgets, evidence, and
record-keeping around that execution.

The organisation metaphor has exact computational meaning. A **franchise** is
the persistent institution: it owns objectives, the personnel roster, resource
allocations, and measured history across games. An **objective** carries an
immutable task contract and acceptance gates bound to one durable drive ledger.
A **game** (drive) is a bounded possession of work inside that ledger. The
football vocabulary — yards-to-go, downs, fourth down, punts — is presentation
metadata over an explicit machine state that folds from journaled events.
Personnel are registered resources with capability classes; a Front Office
service builds rosters offline under an eight-dimensional salary cap, and
membership freezes at kickoff. The words are the interface; the guarantees are
the product.

Two rules define the operating model:

1. **Models may propose progress. Only evidence establishes it.** Yards-to-go
   is a weighted, evidence-backed distance across deliver gates, tracked as a
   multi-dimensional progress vector (resolved and unresolved requirements,
   failing gates, uncertainty, evidence strength, regressions). A model
   response is a claim until independent evidence validates it; narrative
   claims carry no gate weight by construction.

2. **Victory is declared by verification, not assertion.** Each game carries a
   sealed `GameContract` enumerating individually addressable win conditions —
   gate satisfied, evidence present, artefact validated, threshold met,
   forbidden dependency absent — with mandatory flags. A deterministic
   verifier checks the contract against gates, sealed evidence, and artefacts;
   a completion claim against any unmet mandatory win condition is refused at
   fold time and recorded as a rejected claim.

Around this core, specialised subsystems give the organisation its full
repertoire: **huddles** (budgeted, policy-admitted synchronisation between
concurrent workers), **Blitz** (defensive parallel single-focus inference under
outage or deadline pressure, where the first independently verified candidate
wins, never the first response), **tactics** (deterministically admitted
organisational shapes — formations, adjustments, doctrines — each with a
declared counter and a conflict matrix that fails closed), **Lastdown**
(fourth-down possession doctrine: convert, field goal, punt, reset, or
abandon), **CERS** (structured recovery artefacts harvested from failed work),
and the **Bench** and **Manmark** services (deterministic substitution and
defender ownership registries).

## What Playbook guarantees

Playbook's promise to an operator is a set of constitutional guarantees that
hold regardless of which models or providers are staffed into a game.

| Guarantee | Meaning | Where it is enforced |
| --- | --- | --- |
| Deterministic control plane | The same state and policy always produce the same decision. Inference recommends; it never decides. Every decision — routing, admission, substitution, branch selection — folds through the state machine as a journaled event, including refusals. | `playbook-state` fold; every subsystem admits through deterministic policy |
| Evidence over assertion | A false claim cannot declare victory. Evidence is sealed in a digest chain; completion requests with open mandatory gates are rejected and journaled; a tampered witness is a replay divergence. | `playbook-gates`, `playbook-evidence`, `GameContract` verification |
| Possession survives failure | Worker or provider death does not end the objective. Every invocation is journaled before dispatch, carries an idempotency key, and is reconciled explicitly after a crash. A failed player is substituted without losing the objective, the ledger, or the accepted progress. | `playbook-journal` write-ahead log, dispatch ledger, Bench |
| Bounded expenditure | Budgets are enforceable, not advisory. Eight dimensions (tokens, money, wall clock, tool calls, inference calls, retries, context capacity, provider quota) are governed at fold time; the franchise salary cap bounds lifetime grants across all objectives; remaining budget never grows inside a possession. | `playbook-budget`, governors, franchise cap |
| Replayable history | Every committed event is fsynced to a checksummed journal before the authority commits it. Recovery verifies digests and sequence numbers, truncates a torn or tampered tail, and reproduces the state exactly. The replayed state equals the live state. | `playbook-journal`, audit/resimulate fold modes |
| Frozen roster eligibility | Contest membership freezes at kickoff and survives punts and recovery. Registering a new resource mid-game cannot expand that game's eligibility; readiness can change only within frozen eligibility. | `playbook-franchise`, `playbook-front-office` |

These are not aspirations; each is a testable invariant. The repository's test
suite holds, among others: evidence-free claims leave yards-to-go unchanged; a
mandatory failed gate cannot complete the drive; a tool failure cannot be
stored as a tool success; an identical failed play is rejected until the field
changes; budget pressure does not authorise deployment; and a journaled drive
recovers its event log exactly.

## How the guarantee set is proven

The guarantees are backed by a release-gate record, deterministic simulation
suites, and a per-delivery verification gate.

### Release gates A–F

The Franchise Runtime Closure campaign (see
[docs/closure-campaign.md](../closure-campaign.md)) decomposed the path to an
autonomous franchise runtime into six release gates. **All six gates are
closed** as of the FC-1..FC-7 delivery:

| Gate | Scope | Status |
| --- | --- | --- |
| A — Deterministic foundation | Sealed `GameContract`/`WinCondition` with fold-enforced completion gating, the multi-dimensional progress vector, and the single deterministic EVALUATING branch ladder | Closed |
| B — Team operation | The Bench decision service and the Manmark registry with the strategic-to-instinctive graduation path, plus the sealed Thinker-to-Tinker directive | Closed |
| C — Coherence | Huddles: admission economics, coherence-debt bands, scoped context packets, contradiction sets, escalation ladder | Closed |
| D — Recovery | Lastdown possession doctrine plus CERS structured recovery artefacts | Closed |
| E — Defence | Blitz: eligibility, envelopes, content-addressed target freeze, race state machine, cancellation economics | Closed |
| F — Franchise proof | The canonical Franchise Game runs end-to-end through the journal fold with the §28 demonstration trace and no human intervention | Closed |

The campaign document retains the original per-gate gap analysis as the record
of what was found and what was built.

### Deterministic simulation suites

Proof is by replayable demonstration, not narrative:

- **Acceptance drive** (`playbook mvp`): gate decomposition, capability
  discovery, evidence-backed yardage, an induced tool failure with a refused
  blind retry, budget pressure, a fourth-down punt, a fresh possession, and an
  audit replay in which the replayed state equals the live state.
- **Canonical adversarial game** (`playbook-sim/tests/canonical_game.rs`):
  heterogeneous personnel, injected failures, strict spend limits, hallucinated
  and misleading claims, and the §21 metric set — recorded end-to-end with no
  human intervention, replayed identically under audit and resimulate modes.
- **Tactical campaign** (`playbook-sim/tests/tactical_game.rs`): the same
  roster and objective produce different lawful organisational shapes across
  six phases, with every tactical decision journaled and replayable.
- **Blitz injection suite**: twelve tests (BZ-T01..T12) plus nine fixture
  scenarios covering fast wrong answers, provider outage, budget and deadline
  exhaustion, correlated failure, cancellation races, verifier failure, target
  mutation, and formation bias.
- **Live runtime tests** (`playbook-live/tests/live_runtime.rs`): a real
  command (`git rev-parse HEAD`) executed against the actual repository,
  sealed as evidence, and accepted through the ordinary evidence path; plus
  the scripted provider-failure-and-substitution demonstration in which the
  Bench replaces a failed player without losing the objective.

### The deliver verification gate

Every delivery passes a strict, mechanical verification gate
(`deliver --spec deliver.toml --strict`): code formatting, Clippy with warnings
denied (`-D warnings`), the full Rust test suite, and Tov policy checks and
tests over the isolated adapter modules. The deliver specification also
forbids placeholder marker strings (task-list, fix-me, and never-implemented
macro markers) in core sources. A graded analysis snapshot (`uni --fail-under 80 .`)
supplements the gate. Work that does not pass the gate does not ship.

## Maturity and the road to live operation

The repository version is **0.1.31**, closing out the v0.1.x series and opening
v0.2.x.

**v0.1.x — deterministic prototype, complete.** Phases 0 through 9 are in the
tree: draft-07 schemas, the deterministic core, the capability-plane envelope,
play admissibility, Lastdown dispositions, isolated Tov adapters, trace replay
with failure injection, the contract seam for an external orchestrator, and the
historical prior table that learns routing from measured outcomes across drives
(never from self-reports, which stop counting once a personnel's measured
record reaches the policy threshold). All six release gates above closed in
this series: Blitz, tactics, contracts, CERS, Bench, Manmark, the Thinker-to-
Tinker discipline, and the canonical game are delivered and tested.

**v0.2.x — live heterogeneous execution, begun.** `playbook-live` starts real
execution through a non-fixture capability plane while keeping the
constitutional rules unchanged. What exists today:

- `ToolPlane`, the first non-fixture plane: an allowlist of real commands bound
  to a real working directory, with deadline enforcement by the plane itself,
  real exit codes and output, and outcomes sealed as the evidence class the
  play demanded.
- `ExecutionEnvelope`: every live invocation carries its authority, allowed
  operations, evidence classes, a budget envelope that can narrow but never
  widen ceilings, a timeout, and a mandatory idempotency key.
- Durable at-least-once dispatch with explicit reconciliation: a lost response
  is `EffectUnknown` and leaves that state only through reconciliation — no
  retry occurs merely because the caller did not receive an answer.
- A six-state conservative cancellation vocabulary in which unknown keeps spend
  reserved; cost where unknown is never booked as zero.
- Deterministic live identity (`live-{provider}-{model}-{profile}-{envelope}`),
  so different providers are different empirical players with incomparable —
  and therefore never conflated — histories.
- `FaultPlane`, which scripts the directive's required fault list (transport
  faults, repository conflicts, reconciliation ambiguity, partial reports,
  incorrect success claims, duplicates, late results, unavailable costs)
  against any plane.

**What a host must supply.** Provider-specific behaviour lives behind the
`ModelTransport` trait, never inside the authority, reducers, evidence logic,
routing, or the Front Office. To staff model personnel against a real
provider, a host implements that one trait and wires its provider client and
credentials; `ProviderPlane` then packages every response as a claim with its
digest travelling as an artefact reference, and the ordinary evidence path
takes over. The repository ships an honest placeholder transport that refuses
with directions rather than pretending to work.

## Investment and risk posture

**What the buyer or operator gains.**

- **Measurable organisational performance.** The canonical game harness
  records the §21 metric set — including VGCR (verified game completion rate),
  time-to-recovery-style measures, and cost-per-win economics — alongside the
  simulation report's completion status, yards-to-go trajectory, remaining
  budget, tool failures, model calls, personnel changes, elapsed budgeted
  time, monetary cost, satisfied gates, and integer drive efficiency. Blitz
  adds its own economics: the blitz premium, cost-per-second-recovered, and
  effective parallelism. Metrics are defined by the contracts and produced by
  replayable runs; none of them is a self-reported model opinion.
- **Routing that learns from measured outcomes.** The prior table attributes
  evidence-backed progress to the personnel that produced it and, past a
  policy sample threshold, routes on measured residuals instead of
  self-assessments. Personnel history is empirical and portable across drives.
- **An audit surface that survives disputes.** The checksummed journal, sealed
  evidence, and deterministic replay mean any game can be re-derived exactly
  from its event log — including why the system declined to spend.

**The honest risks.**

- **Simulation-first evidence.** The acceptance drive and failure-injection
  suites exercise fixture and controlled-live capabilities. They do not
  demonstrate a live Lucid/ELCI integration at production scale, and the
  policy-comparison feature reclassifies recorded events under another policy;
  it does not generate counterfactual actions or demonstrate improved live
  routing outcomes. Live results at scale are still to be earned.
- **Live-provider integration is early.** The provider seam is one trait, but
  hosts must implement it and supply credentials; the space of real provider
  behaviours beyond the scripted fault list is not yet exhausted.
- **Single-host persistence.** The durable journal and franchise storage are
  local to the host. Serialization alone is not durable execution recovery:
  the host must persist invocations before dispatch and reconcile uncertain
  side effects after a crash — Playbook enforces the discipline and records
  the debts, but it does not replicate storage across machines.
- **Conservative economics by design.** Franchise grants are never
  automatically refunded or renewed, and unknown costs are booked at ceiling.
  This bounds risk deliberately, but operators should expect reservations, not
  optimisations, until refund policy evolves.

## Cross-references

Companion anthology documents:

- [01 — System architecture](01-system-architecture.md): how the crates
  compose, from `playbook-core` nouns to the journal fold.
- [02 — Authority and governance](02-authority-and-governance.md): who may
  decide what, and where the boundaries sit.
- [03 — Evidence, journal, and replay](03-evidence-journal-replay.md): how the
  system knows what was true and can prove it again.
- [04 — Operations runbook](04-operations-runbook.md): running a game, and
  what happens when things fail.
- [05 — Security and boundaries](05-security-and-boundaries.md): what
  personnel may touch and what they can never touch.
- [06 — Metrics and quality gates](06-metrics-and-quality-gates.md): the
  measurement surface and the release gates in depth.
- [07 — Glossary and event index](07-glossary-and-event-index.md): the
  vocabulary and the journal event catalogue.

Normative contract documents (these govern over any anthology summary):

- [Franchise ownership](../franchise.md) — objectives, rosters, salary cap,
  continuity, and the write-ahead franchise sink.
- [The huddle contract](../huddles.md) — budgeted, policy-admitted
  synchronisation.
- [The blitz contract](../blitz.md) — defensive parallel single-focus
  inference, eligibility, envelopes, and economics.
- [The tactics contract](../tactics.md) — organisational shape, admission,
  composition, and the conflict matrix.
- [The live runtime contract](../live-runtime.md) — the v0.2.x execution
  envelope, dispatch ledger, provider seam, and fault plane.
- [The closure campaign](../closure-campaign.md) — the release-gate record
  and the retained gap analysis.
- Crate-level documentation under `crates/*` and the schemas in `schemas/`.
