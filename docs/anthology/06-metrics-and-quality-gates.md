# Playbook Metrics and Quality Gates

## Purpose of this document

This document is written for engineering management. It answers two questions:
how the organisation is measured, and what blocks a release. Playbook is a
deterministic, evidence-governed control plane, and that character extends to
its measurement: every number the system reports is derived by integer
arithmetic from journalled, fold-replayable state, never from a narrative
claim about how work is going. The first half of the document is the metric
catalogue — what each subsystem measures, on what scale, and what question it
answers. The second half covers the machinery that turns measurement into
quality control: the verification gate that runs on every commit, the
four-layer testing doctrine, and the release gates.

The anthology summarises; the contracts govern. The normative sources for
this document are `docs/closure-campaign.md`, `docs/blitz.md`,
`docs/huddles.md`, `docs/tactics.md`, the crate documentation under
`crates/*`, and the gate specification itself in `deliver.toml`.

## The measurement philosophy

**Measured state over claims.** Playbook's founding rule is evidence over
assertion: a sealed evidence class moves a gate's satisfaction; a narrative
claim satisfies nothing. The measurement layer enforces the same rule against
optimism. Evidence-free satisfaction carries zero evidence strength in the
progress vector. Evidence-free claims leave yards-to-go unchanged. A false
completion claim against an unmet win condition is refused at fold time and
counted (`false_completion_claims` on the drive state), so the ledger records
not only progress but also every attempt to claim progress that was not
earned.

**Every subsystem emits deterministic metrics.** Yards-to-go, the progress
vector, drive efficiency, huddle debt, blitz economics, tactical success
rates and recovery reuse are all pure functions of recorded state. The same
inputs always produce the same outputs: `ytg_milli` recomputes from the gate
table, `BlitzReport` derives from the closed blitz record plus the sequential
estimate, and the tactical memory always recommends the same next call for
the same history. Metrics fold from the journal alone; nothing is a side
channel. Replay reproduces every number in this document's catalogue or
refuses the log.

**Scales.** Two integer scales carry almost every measurement:

- **Parts per million (`ppm`)** — `PPM_SCALE = 1_000_000` in
  `crates/playbook-core/src/units.rs`. Gate weights, satisfaction, budget
  fractions, success rates, completion and cancellation efficiency are all
  expressed in ppm, computed with saturating integer arithmetic. A rate is
  never a float; a half-successful tactic scores exactly 500_000 ppm.
- **Per-mille (`milli`, 0..=1000)** — coherence debt, pressure components,
  admission scores and effective parallelism use the milli scale, again as
  clamped integers.

**The YTG reference.** Yards-to-go is normalised to Lastdown's 10.000-yard
reference: `YTG_REFERENCE_MILLI = 10_000`. A fully open objective reads
10_000 milli; a completed one reads zero. Because the reference is a constant
rather than a per-drive judgement, distance is comparable across drives,
franchises and time.

## The metric catalogue

### Progress and completion

| Metric | Where it lives | Definition and question it answers |
| --- | --- | --- |
| YTG (`ytg_milli`) | `crates/playbook-ytg/src/lib.rs` | `YTG = Σ wᵢ(1 − sᵢ)` over declared gates, scaled to the 10_000-milli reference. Satisfaction `sᵢ` moves only when a gate has sealed evidence or an explicit waiver. Answers: how far is the objective from done? |
| Per-gate distance (`gate_unresolved_milli`) | `playbook-ytg` | The weighted unresolved share one gate contributes to total YTG. Terminal gates contribute zero. Answers: which single gate is holding the yards? |
| `ProgressVector` | `playbook-ytg::progress_vector` | Six dimensions: `resolved_requirements_ppm` (weight share of satisfied and waived gates), `unresolved_requirements_ppm` (weight share still open — resolved, unresolved and failing shares always reconstruct the whole), `failing_gates` (count of gates in the `Failed` state), `uncertainty_ppm` (weight share never assessed — how much of the field has not been scouted), `evidence_strength_ppm` (evidence-bearing satisfaction over total satisfaction — how much claimed progress rests on sealed evidence or waivers rather than nothing), and `regression_count`. Answers: is progress real, broad, evidenced, and not hiding a regression? |
| Vector primacy | `playbook-ytg` module docs | The vector is authoritative; the scalar YTG is derived from it for routing convenience, never the reverse. A regression in one dimension cannot hide behind average progress. |
| Completion estimate (`estimate_completion_ppm`) | `crates/playbook-state/src/classify.rs` | The drive state carries a completion estimate in ppm derived deterministically from the measurement and policy. Answers: what fraction of the drive does the state machine consider finished? |
| False completion claims | `playbook-state` drive state | Counter of completion claims refused at fold because a win condition was unmet. Answers: how often has the organisation tried to declare victory without the evidence? |

### Economics

| Metric | Where it lives | Definition and question it answers |
| --- | --- | --- |
| `DriveBudget` | `crates/playbook-core` | Eight enforced dimensions: tokens, monetary cost (microunits), wall clock (ms), tool calls, inference calls, retry count, context capacity and provider quota. `effective_remaining_ppm` and governor refusals at fold time turn the budget into a measurement, not an aspiration. Answers: what did the organisation buy, and what is left? |
| Drive efficiency (`Efficiency`) | `crates/playbook-ytg::drive_efficiency` | Yards gained in ppm of the opening distance over budget spent in ppm; `ratio_milli` is the ratio — yards gained per unit of budget spent. Constituents are kept beside the ratio so a good ratio can never launder a bad trade. Answers: what is a yard worth on this drive? |
| Blitz clocks (TTFC / TTFV / TTR) | `crates/playbook-blitz/src/economics.rs` | Three race-clock measures kept distinct because they flatter differently: time to first candidate (any quality), time to first verified candidate, and time to measured recovery. Fast unverified output improves TTFC while TTR burns; only verification moves TTR. Answers: how fast did recovery actually happen? |
| `BlitzPremium` (`blitz_premium_microunits`) | `BlitzReport` | Race cost minus the `SequentialEstimate` counterfactual — what one sequential worker was estimated to need for the same objective. The deliberate excess paid for verified-recovery latency; negative means the blitz was the cheaper strategy outright. Answers: what did parallelism cost over the alternative? |
| `CostPerSecondRecovered` | `BlitzReport` | Premium per full second of latency gained. A sub-second gain prices no whole second; a non-positive premium prices at zero. Answers: what does a recovered second actually cost? |
| Raw vs effective parallelism | `BlitzReport` (`raw_parallelism`, `effective_parallelism_milli`) | Raw parallelism counts dispatched workers; effective parallelism counts independent attempts. Eight correlated workers are not eight attempts — admission refuses a formation below the `min_effective_parallelism_milli` floor (1,500). Answers: how much redundancy did we really buy? |
| Cancellation efficiency (`cancellation_efficiency_ppm`) | `BlitzReport` | Avoided tokens over everything cancellable (`avoided / (avoided + wasted)`), where `tokens_wasted_after_recovery` is everything the race kept spending after the objective was met. An unrecovered blitz keeps all of it. Answers: how quickly did the defence stop spending once it had won? |
| Verification funnels | `BlitzReport` | Worker, provider and model success rates in ppm (verified over fielded, per dimension), plus `candidate_quality_ppm` (verified candidates over emitted candidates). Answers: who is worth sending next time? |
| Policy floors on measured pressure | `docs/blitz.md`, `policy.blitz` | Admission thresholds are policy data over measured quantities: urgency floor 300 and pressure floor 500 on the 0..=1000 weighted seven-component pressure score; formation weights (success 4, speed 3, independence 3, availability 2, cost 1) with the `min_worker_score` floor (2,000). Answers: when is expenditure admitted at all? |

### Coordination

| Metric | Where it lives | Definition and question it answers |
| --- | --- | --- |
| Coherence debt | `docs/huddles.md`, `tov/huddle/coherence_debt.tov` | A deterministic integer in 0..=1000: a weighted sum of seven clamped per-mille components — assumption divergence, dependency staleness, contradiction load, personnel turnover, cross-unit change impact, unresolved blockers and context age. Policy bands: monitor at 200, huddle-eligible at 400, coordinator-recommended at 600, mandatory reconciliation at 800. All thresholds live in `policy.huddle`, never in compiled logic. Answers: how incoherent is the shared picture, and what is owed? |
| Huddle correction | `huddle_outcome_committed` event | Journals `coherence_debt_before`, `coherence_debt_after`, contradictions resolved and a rework-avoided estimate; the outcome records the measured post-reconciliation debt. Answers: did the huddle actually reduce debt, and what rework did it avoid? |
| Tactical efficacy (ΔYTG per tactic) | `crates/playbook-tactics/src/sequencing.rs` | Every graded play records `ytg_before_milli` and `ytg_after_milli` with the evidence-graded success flag, in insertion order, ordered by monotonic timestamp. Answers: which tactic actually moved the ball? |
| Success rate (`success_rate_ppm`) | `TacticalHistory` | Integer success rate of a named tactic over its last `window` occurrences, 0..=1_000_000 ppm by pure integer math; `None` when the tactic has no occurrence in the window. Answers: is the tactic still working lately? |
| Overreaction detection | `detect_overreaction` | Fires when, over the trailing window: the most recent `threshold_repeats` records are one dominant tactic, and that tactic's in-window success rate has dropped below 500_000 ppm — strictly less than half. Repetition without decay detects nothing; exactly half succeeding is not an overreaction. Produces `Overreaction { tactic, repeats, recent_success_rate_ppm }`. Answers: are we overcommitted to a strategy the defence has answered? |
| Constraint counter (`recommend_next`) | `playbook-tactics` | The frozen constraint table maps each dominant tactic to its complementary counter (`spread` to `bunch`, `verticals` to `prevent`, `flood` and `quick_game` to `make_them_drive`, `heavy` to `spread`; anything else defaults to sequencing itself). The fold replaces the planned call only when the plan is the overcommitted tactic. Answers: what should be called instead? |

### Recovery

| Metric | Where it lives | Definition and question it answers |
| --- | --- | --- |
| Harvest coverage | `crates/playbook-cers/src/harvest.rs` | Every materially failed play, every closed blitz and every provider outage yields exactly one structured recovery artefact — measured facts, disproved assumptions, surviving partial work, a reproduction path, ruled-out hypotheses. Harvesting is total, deterministic and repeatable: the same failure always produces the same artefact, byte for byte, content address included. A win is harvested too. Answers: did this failure leave the institution anything? |
| Reuse rate (`reuse_rate_ppm`) | `crates/playbook-cers/src/store.rs` | Share of recorded recovery artefacts consumed at least once, in ppm; an empty store reuses nothing. The point is to measure whether the institution actually reuses what it recovered, not merely that it stored it. Answers: is recovered knowledge paying rent? |
| Lastdown continuation | `crates/playbook-lastdown`, `playbook-franchise` | Fourth down produces a scored disposition and a sealed continuation package that carries measured state forward to the next possession, omitting the keys the doctrine forbids. Answers: can the next drive start from what this one learned? |
| Blitz recovery latency | `BlitzReport`, `docs/blitz.md` | TTR is the recovery measure the defence exists to shrink; `latency_gain_ms` is the sequential estimate minus actual recovery time, and cancellation latency is benchmarked and recorded with the tokens the stop avoided. A race that cannot stop its losers has failed in its own named way (`cancellation_failure` in the failure taxonomy). Answers: how fast, and how cleanly, did defence recover? |

### Organisation

| Metric | Where it lives | Definition and question it answers |
| --- | --- | --- |
| VGCR (verified games won / attempted) | `docs/closure-campaign.md` (FC-7, §21 metrics) | The organisation-level rate: verified games won over games attempted, recorded by the canonical benchmark harness alongside the §21 metrics. A game counts as won only through the full win-condition verification path — the same standard a drive faces at fold time. The canonical game itself runs end to end through the journal fold in `crates/playbook-sim/tests/canonical_game.rs` with the §28 demonstration trace and no human intervention. Answers: when the franchise plays, how often does it verifiably win? |

## The verification gate

The deliverable gate is specified declaratively in `deliver.toml` and executed
by the `deliver` tool. It is the single definition of "the tree is sound" and
runs on every commit (below).

**File contracts.** Each `[[file]]` entry names a path and its requirements:

- `required = true` — the file must exist.
- `forbid_regex` — source files forbid the standard unfinished-work markers:
  the deferred-task and fix-later comment tags and the placeholder macro. A source file that carries a promise to finish later fails the gate;
  the repository has nowhere to hide a placeholder.
- `min_size_bytes` — documentation contracts must carry substance (most
  contract documents demand at least 2,000 bytes, the README at least 400,
  fixtures at least 100), so an emptied or stubbed file fails even though it
  exists.

The contracts cover every crate root of consequence, every contract document,
every checked-in fixture (including the full blitz failure-injection set and
the huddle scenario transcripts), the canonical and tactical game harnesses,
the live-runtime integration tests, and the win-condition and evaluation
modules that carry the completion gating.

**The command suite.** Each `[[command]]` entry declares an expectation
(`expect_exit`) and a timeout:

| Command | Assertion |
| --- | --- |
| `cargo fmt --all --check` | Formatting is canonical; exit 0. |
| `cargo clippy --offline --all-targets -- -D warnings` | No lint warning survives; any warning is an error. |
| `cargo test --offline` | The full suite passes and stdout reports `test result: ok`. |
| `tov check --json …` (per module) | Every Tov module checks clean: the adapter, route helper, test-report transform, capability probe, diagnostics, the eight huddle modules, the two blitz modules and the tactics adjustment module. |
| `tov test tov` | The Tov module suite passes. |

**Strict mode.** The gate is run as `deliver --spec deliver.toml --strict`.
Strict mode fails the run on any contract deviation rather than reporting
degradations; there is no advisory tier.

**Runs per commit.** The `kaptaind` watcher is configured in `kaptaind.toml`
with the test command `deliver --spec deliver.toml --strict --progress never`
marked `required = true`: a change set does not become a commit until the
entire gate passes. Watch is recursive over the tree, clusters events, and
rate-limits commits (`min_commit_interval`), so the gate's cost is paid per
change cluster, not per keystroke. The release definition is therefore
continuous: the mainline is always in a state that passed fmt, clippy, the
full test suite, and policy checks.

## The testing doctrine

**Four layers.** Playbook requires evidence at four altitudes, and a feature
without adversarial tests does not graduate:

1. **Unit.** In-crate tests assert the exact arithmetic: the canonical
   weighted gate table reads 3,500 milli; a half-successful tactic rates
   exactly 500_000 ppm; claimed-but-unevidenced satisfaction produces zero
   evidence strength; the resolved, unresolved and uncertainty shares
   reconstruct the whole. Every module in `crates/*` carries its own.
2. **Deterministic simulation.** `crates/playbook-sim` runs whole drives
   through the real fold: the MVP sequence, the six-phase tactical acceptance
   benchmark (`tests/tactical_game.rs` — same roster, different game state,
   different formation, different information topology, different measured
   result), and the canonical franchise game (`tests/canonical_game.rs`).
3. **Failure injection.** Adversarial scenarios are checked in as fixtures
   and demanded by the gate: fast wrong answers, provider outage, budget and
   deadline exhaustion, correlated failure, cancellation races, verifier
   fail-closed, target drift, and power- and network-critical formation bias
   (`fixtures/blitz/`), plus the live-runtime fault taxonomy in
   `crates/playbook-live/src/faults.rs`. The campaign exit gates were written
   the same way: a regression in one progress dimension must fail to hide, a
   huddle, a substitution and a blitz must each win their branch in an
   injected scenario.
4. **Real integration.** The live-runtime suite
   (`crates/playbook-live/tests/live_runtime.rs`) and the server's
   request-constructing integration tests exercise the real surfaces —
   journal recovery truncates at the first torn, tampered or out-of-sequence
   record; the tool plane reconciles idempotency keys — not simulator
   stand-ins.

**The fixture doctrine.** Checked-in fixtures are generated outputs, and the
generator/fold equality is the regression test. Scenario transcripts under
`fixtures/` are built through the authority, so every event in them is one
the fold accepted; they are regenerated by ignored tests (for example
`cargo test -p playbook-huddle --offline -- --ignored
regenerate_huddle_fixtures`). The checked-in companion test folds each
fixture and asserts its scenario marker, and replay must reproduce the same
state under audit and resimulate modes — fixture equality tests in
`playbook-huddle` and `playbook-blitz` fail if a generator change and the
checked-in output ever diverge. A silent semantic change to any subsystem
therefore breaks a fixture fold, not a future incident.

**Invariants the tests hold.** Evidence-free claims leave yards-to-go
unchanged; budget remaining does not grow inside a possession; a mandatory
failed gate cannot complete the drive; a tool failure cannot be stored as a
tool success; a tampered witness is a replay divergence; an identical failed
play is rejected until the field changes; a waiver needs an authority and a
reason; priors learn only from measured outcomes, and self-report stops
moving a routing score once the measured record reaches the policy threshold.

## Release gates

The Franchise Runtime Closure campaign defined six release gates. All six are
closed as of the FC-1..FC-7 delivery; the record of what each required lives
in `docs/closure-campaign.md`.

| Gate | Scope | Status |
| --- | --- | --- |
| A — Deterministic foundation | `GameContract`/`WinCondition` with fold-enforced completion gating, `ProgressVector`, the EVALUATING branch ladder, replayable journal with fixture equality, budget enforcement | Closed |
| B — Team operation | Bench decision service over measured inputs, Manmark registry with the strategic-to-instinctive graduation path, sealed Thinker-to-Tinker directive | Closed |
| C — Coherence | Huddles: condition-triggered admission economics, coherence-debt bands, contradiction sets, forced synthesis, escalation ladder | Closed |
| D — Recovery | Lastdown continuation plus CERS: structured recovery artefacts, consumable without the failed context, reuse measured | Closed |
| E — Defence | Blitz: eligibility, target freeze, diversity-scored formations, the race state machine, failure taxonomy, economics; the full injection suite demonstrated | Closed |
| F — Franchise proof | The canonical adversarial game end to end through the journal fold, with injected failures, heterogeneous personnel and strict spend limits; VGCR and the §21 metrics recorded; no human intervention | Closed |

Gate closure is not a one-time event: the deliver gate runs per commit, the
fixture folds re-verify the journal on every test run, and the canonical game
remains in the suite. A regression against any gate's substance fails the
tree on the next commit, which is the point.

## Cross-references

- `docs/anthology/00-executive-overview.md` — the guarantees these metrics evidence.
- `docs/anthology/01-system-architecture.md` — how the measuring crates compose.
- `docs/anthology/03-evidence-journal-replay.md` — the journal and fold machinery beneath every metric.
- `docs/anthology/04-operations-runbook.md` — operating the runtime the metrics describe.
- `docs/closure-campaign.md` — the gate definitions, exit criteria and campaign record.
- `docs/blitz.md` — pressure, eligibility, envelopes, race and economics contracts.
- `docs/huddles.md` — coherence debt, admission economics and huddle journal events.
- `docs/tactics.md` — the tactical contract, admission and the conflict matrix.
- `deliver.toml` — the verification gate specification itself.
- `crates/playbook-ytg/src/lib.rs`, `crates/playbook-blitz/src/economics.rs`,
  `crates/playbook-tactics/src/sequencing.rs`, `crates/playbook-cers/src/` —
  the metric implementations cited above.
