# Playbook Personnel and Organisation

## Purpose of this document

Playbook is a deterministic, evidence-governed organisational control plane.
Its raw material is personnel: models, tools, deterministic procedures,
specialist agents, and human escalation. This document explains how the
organisation staffs, identifies, owns, and substitutes that personnel. It is
written for engineering leads and operators who must decide who plays, who
sits, who owns a persistent problem, and who is replaced — and who must be
able to defend those decisions from the record afterwards.

Two commitments run through every section. First, selection is deterministic:
the same measured inputs always produce the same organisational decision, and
every decision carries machine-readable reasons. Second, capability is
measured, not asserted: a player's kind, tier, or price never overrides what
its history shows it can do. The normative sources for this document are
`docs/franchise.md` and the crate-level documentation in
`crates/playbook-franchise`, `crates/playbook-front-office`,
`crates/playbook-routing`, `crates/playbook-live`, and `crates/playbook-core`.
Where this summary and the contracts disagree, the contracts win.

## Personnel kinds and the capability hierarchy

`PersonnelKind` in `crates/playbook-core` defines six capability classes,
grouped into four tiers. The tier is an asking order, not a prestige ranking:
lower tiers are asked first.

| Tier | Kinds | Character |
| --- | --- | --- |
| 0 | `DeterministicProcedure`, `Tool`, `Toolchain` | Deterministic; no inference consumed |
| 1 | `Model` | Single-model inference |
| 2 | `SpecialistAgent` | Bounded specialist execution |
| 3 | `HumanEscalation` | Human decision on the record |

The ordering rule is stated in the source and enforced by routing:
deterministic capability outranks model prestige. Routing
(`crates/playbook-routing`) ranks personnel by tier ascending, then score
descending, then personnel id ascending, so a compiler, a test runner, or a
fixed procedure is always considered before any model, however celebrated the
model. Among admissible plays, only the best achievable tier of positive-yard
plays survives selection. On a third-down rush the scorer additionally applies
a deterministic bonus, biasing the organisation toward tier-0 execution when
the situation is urgent.

Every personnel record carries the same measured fields, expressed on parts
per million scales where one million is the full scale: `capability_fit_ppm`
(measured coverage of a play's requirement), `reliability_ppm` (measured
success rate), `quota_remaining_ppm` (remaining provider quota), and
`evidence_quality_ppm` (the quality of the evidence this player has produced),
alongside `cost_tokens`, `latency_ms`, and an optional self-reported
`own_ytg_milli`. The scoring function blends these under policy weights, with
one deliberate asymmetry: a learned historical prior is measured ground truth
and outranks the player's own estimate. The self-report only fills the gap
until enough measured outcomes exist. Self-reported distance never becomes
measured performance. Price alone does not win; intelligence alone does not
win.

## The franchise as persistent institution

The franchise (`crates/playbook-franchise`) is the persistent management layer
above Playbook's drive authority. It survives games: it owns objectives,
roster, allocations, management decisions, and measured history, while a
player owns only a bounded execution assignment. Replacing a player does not
replace its objective or the official drive ledger; many independent
objectives can coexist in one franchise, each with an immutable task contract,
acceptance gates, and one durable drive ledger.

Membership is governed by three availability states — `ready`, `benched`, and
`unavailable` — plus the frozen eligibility contract. Each drive captures its
own `frozen_roster`, including the issued contract and personnel descriptors.
Only frozen roster members can play: registering a new resource during a
contest does not expand that contest's eligibility. Contracts can be staged
while work runs, but activation requires all franchise contests to be terminal
and reconciled, and a punt does not open a transaction window. Readiness can
change live, but only within frozen eligibility.

Every management action is a `ManagementRecord` — a sequence number, an
action, a subject, and a required reason. The coaching plane changes
availability only with a reason in the management history, so the answer to
"why was this player benched" is always a query, never an investigation. The
snapshot also carries the salary cap in all eight `DriveBudget` dimensions:
lifetime grants reserve cap before any dispatch and never reset, failed
reservations leave the franchise unchanged, and there is deliberately no
automatic refund. Observed spend, unresolved invocations, and conservatively
reserved unmeasured spend are reported separately, so unknown expenditure can
neither be silently reused nor presented as measured cost. Scouting is derived
from evidence-backed historical priors; each imported seed is subtracted during
aggregation so recovery and later drives never count those samples twice.

Two boundaries matter to operators. Position labels let the host organise its
specialists, but they do not override capability admissibility. And provider
and model metadata stay in franchise storage — they are not added to Lastdown's
continuation package.

## Front Office: the offline selector

The Front Office (`crates/playbook-front-office`) constructs the roster the
way a real front office does: in the off-season, from closed-season film. It
performs offline, deterministic portfolio construction with no execution-plane
dependency. Its module documentation states the hard boundary plainly: no
repository, live task, drive state, tool handle, or execution callback is
accepted as input.

Its inputs are deliberately lossy: workload classes (repository analysis, code
repair, technical research, long-context synthesis, reconnaissance) described
as `OpponentProfile`s with ppm weights that must sum to exactly one million,
and `Candidate` records carrying closed-season `HistoricalPerformance` per task
class, cost, quota, latency, reliability, tool coverage, and explicit failure
domains — shared upstream infrastructure and rate-limit pools, not just brand
names. The constraints an operator sets are structural: maximum members,
minimum depth per formation, minimum distinct providers, minimum reliability,
minimum historical samples, maximum latency, and whether an offline fallback
is required.

The temporal rule is structural, not advisory: the effective season must be
strictly later than the season the evidence runs through. The Front Office
never selects on the same game's results, because it never sees a live game at
all. Candidates it rejects carry machine reasons — reliability, latency, quota,
cost, no historical coverage, or marginal roster value. The output is a
`RosterPlan` reporting coverage, redundancy, and diversification in ppm, sealed
as a `RosterContract` with the magic `ROSTER/1` and a SHA-256 digest over the
canonical plan bytes. `verify()` re-checks the digest and the structural
contracts — provider diversification, per-formation depth, formation coverage —
before the plan is trusted. The franchise stages such contracts and activates
them only between contests; that is the only path by which eligibility changes.

## Live identity and occupancy

Live identity (`crates/playbook-live`) answers a question most orchestration
layers skip: when the organisation measures a worker, what exactly is the
worker? A `LiveIdentity` is the tuple of provider, model, major system-profile
version, and tool envelope. All four together define one stable empirical
player. A change to any of them produces a distinct identity, because folding
`provider-A/model-X` and `provider-B/model-X` into one player would compare
incomparable histories — the same model weights behind a different provider
are a different quota pool, a different serving stack, and therefore a
different measured performer.

The identity maps deterministically to a personnel id — a slugified
`live-provider-model-profile-envelope` — so the same identity always
accumulates its empirical record on the same player, and `comparable()` reports
whether two identities may legitimately be compared at all.

Occupancy is governed with equal discipline. A worker with an unresolved
invocation is busy: the effect of the outstanding invocation is still unknown,
and reassignment is prohibited by default. Only two explicit guards together —
the capability declares safe concurrent occupancy, and policy permits
reassignment for this play — can lift the prohibition. Reconciliation comes
first, always, and it requires an explicit boolean: omitted information is not
treated as proof that an effect did not occur. Benching or replacing a busy
player does not clear the uncertainty; other objectives cannot assign that
player while it remains busy. Busy players are never silently reassigned —
reconciliation first, personnel decisions second.

## Manmark: domain ownership

Manmark (in `crates/playbook-franchise/src/manmark.rs`) gives every persistent
problem domain exactly one defender. A `Mark` records the domain, the owning
personnel, a severity, a state, a remediation class, a deterministic
inspection trigger, the sealed evidence ids attributed to it, a monotonic
last-inspection timestamp, and a consecutive-successes counter.

The mark's state is a three-valued coverage report: `Contained` (the defender
holds coverage), `Exposed` (the trigger tripped; the defender is beaten), and
`Escalating` (a breach was escalated and a blitz owns recovery). Inspection is
deterministic and runs on evidence: a mark trips exactly when the observed
evidence class equals the trigger's class and the canonical JSON summary of the
sealed body contains the trigger's subject fragment (an empty fragment matches
every body of the right class). Inspection timestamps are monotonic — a later
observation never rewinds the record. A breach seals the evidence into the mark
and resets the success streak; a clean inspection of an exposed or escalating
mark recontains it.

Enforcement is instinctive-first. An instinctive mark that breaches is handled
immediately by the host running a fixed deterministic procedure, and no
inference is consumed — the cheapest defence plays by default. Only a strategic
mark, breaching at High or Critical severity, proposes a `ManmarkRequest`
through the Blitz escalation seam — and it is a proposal only: the registry
never authorises expenditure. The host flips a breached mark into `Escalating`
once policy authorises the blitz.

The graduation path closes the loop. After enough consecutive successful
strategic remediations, `graduate` demotes the mark's remediation to an
extracted deterministic procedure (recorded as `graduated: <domain>`) and
restarts the streak. What began as an inference-covered escalation becomes, once
proven, a zero-inference instinct. The registry keeps its marks sorted by
domain, refuses duplicates, and persists order-independently.

## The Bench: substitution with reasons

The Bench (in `crates/playbook-franchise/src/bench.rs`) is the deterministic
substitution service: substitution is a service, not a debate. Inference never
sits on this ladder — it reads measured fields and picks. The same measured
inputs always decide the same way.

The ladder is an ordered evaluation of five substitution classes, most urgent
first, each rung firing only when its situational condition holds and an
eligible replacement exists:

1. **Emergency** — an emergency is declared and the starter is unavailable or
   its provider health sits below `MIN_PROVIDER_HEALTH_PPM` (100,000 ppm).
2. **Availability** — the starter is unavailable or below the provider-health
   floor. Exactly at the floor the starter is still playable.
3. **Performance** — the starter has accumulated at least three recent gate
   failures (`PERFORMANCE_FAILURE_FLOOR`) and a replacement exists with
   strictly fewer failures and fit at least the starter's.
4. **Economic** — at most half the drive's monetary budget remains, the
   starter costs more than zero per play, and a replacement exists costing at
   most half as much while meeting the play's required fit.
5. **Tactical** — the starter's measured fit is below the play's requirement
   and a replacement meets the requirement. Fit decides; prestige does not —
   the ladder never reads `PersonnelKind`.

Eligibility is uniform: a replacement must be available, must not be busy, and
must not be the starter itself. Busy exclusion applies on both sides — a busy
starter is not substituted at all until its invocation reconciles, and busy
players are never selected as replacements. Candidates are ranked by one total
order — fit descending, provider health descending, gate failures ascending,
per-play cost ascending, personnel id ascending — so bench order never matters
and ties always resolve to the lower personnel id. A ladder that reaches its
floor with no eligible replacement keeps the starter playing; the decision
record keeps `replacement` optional precisely to stay honest about that default.

Every decision is journalled with its class, the outgoing player, the
replacement, and machine reason codes in ladder order (for example
`budget_half_consumed`, `replacement_costs_at_most_half`). Measured stagnation
context (`stall_cycles`) is journaled alongside the decision but never consulted
by the ladder itself. A benched substitute must be activated before it is
dispatched; the bench decision and the activation are separate, recorded steps.

## Cross-references

- **01 — System architecture**: how `playbook-franchise`, `playbook-front-office`,
  `playbook-routing`, `playbook-live`, and the official compose into one control
  plane, and where personnel state lives in each.
- **02 — Authority and governance**: the official as the independent authority
  over evidence, progress, and completion; the franchise as management, not
  referee; the Manmark registry's proposal-only escalation boundary.
- **04 — Operations runbook**: operating procedures for roster freezing,
  contract staging and activation between contests, bench activations, busy
  reconciliation, and recovery of a franchise whose player disappeared at
  dispatch.
- **05 — Security and boundaries**: what personnel may touch; why the Front
  Office accepts no execution-plane input; why provider and model metadata
  never leaves franchise storage into continuation packages.
- **Contracts**: `docs/franchise.md` (franchise ownership, roster freezing,
  salary cap, scouting, HTTP contract), `docs/tactics.md` (routing and
  admissibility), `docs/live-runtime.md` (live identity and occupancy),
  `docs/blitz.md` (escalation and severity), and `docs/huddles.md`. Where this
  document and a contract disagree, the contract governs.
