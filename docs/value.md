# Project VALUE: the research contract

**Point.** Playbook shall earn its value empirically. Internal
architectural correctness is not evidence of external effectiveness. The
`value-lab` crate is the independent evaluator that exists so Playbook can
be proven wrong — Playbook is the subject; the lab decides what happened
(§36).

**Evidence.** The lab owns, as code:

* **Frozen conditions** — `ExperimentalSnap` hashes task, commit, tools,
  the *same* model pool for every regime (§10), quota and budget (§9);
  every run references the snap hash. `DecisionSnapshot` freezes what the
  subject knew immediately before each material decision (§14); hindsight
  cannot enter the counterfactual because the counterfactual reads only
  this.
* **Regimes** — P0 through C6 (§4), with C6 (oracle/hindsight) marked
  undeployable and excluded from "best control" comparisons. The question
  is not `Playbook > baseline` but whether Playbook approaches
  hindsight-efficient orchestration more closely than deployable controls.
* **Value math** — `OCAV = U(actual) − U(best frozen alternative)` (§3),
  decision regret and cumulative regret (§16), under utility weights frozen
  at pre-registration. One catastrophe swamps a completed drive by
  construction, and the test suite pins that.
* **Shadow Defence scoring** — the subject's predictions are scored for
  calibration, ranking accuracy and regret error by `score_shadow`, and
  lab–subject agreement is itself a measured variable (§3, §15). Shadow
  Defence does not validate itself.
* **Verdict classification** — `VALUE_PROVEN / CONDITIONAL / NEUTRAL /
  NEGATIVE` (§1) against `FrozenThresholds` (≥10% completion improvement or
  ≥15% cost-per-completion reduction, plus no quality regression, positive
  median OCAV, resilience under fault, and superiority in more than one
  stratum). All thresholds frozen before results (§27, §28).
* **The fault schedule** — deterministic from a seed: the same lab
  reproduces who was faulted and how; ≥20% of the corpus must be
  adversarial or the primary season refuses to validate (§17).
* **Stop conditions** — journal integrity, information parity, pricing
  reliability, evaluator cleanliness, gate bias, snapshot
  reconstructability, and subject-controls-authority each pause the study
  fail-closed (§35).
* **Concrete experiments** — the Huddle comparison runs Playbook's own
  Phase 8 programme as the subject with the lab owning the reading (§19);
  the Lastdown audit independently recomputes the doctrine decision and
  fails a fabricated choice (§21).

**Explanation.** The separation of powers is the whole design: Playbook's
sim engine *executes* (it is the only implementation of the semantics), the
lab *measures and judges* (it recomputes optima from frozen snapshots,
never from the subject's reports). A feature existing in the architecture
establishes nothing — ablations (`ablation_table`) rank each subsystem's
marginal contribution, and a mechanism whose removal changes nothing has
not earned its complexity (§30 P3).

**Visualisation.**

```text
playbook value-lab protocol     # the frozen protocol document
```

```text
# VALUE_PROTOCOL — frozen before the primary season
Hypotheses: H1 completion, H2 resource efficiency, H3 progressive efficiency,
H4 reliability, H5 personnel selection. Central metric: OCAV (§3).
Thresholds (frozen): completion ≥ 100000 ppm relative; cost ≥ 150000 ppm …
```

The scoreboard (§24) renders every axis of record — completion rate,
median cost and wall time, recovery rate under fault, human interventions,
cumulative regret, median OCAV — with the best **deployable** control as
the comparison column and the verdict stamped beneath. The miniature-season
integration test (`a_miniature_season_produces_a_defensible_scoreboard`)
runs the whole §37 pipeline: frozen corpus → fault schedule → two regimes
→ verdict → rendered table → bootstrap interval over completion cost.

**Evaluation.** Until §38's statement contains measured numbers from the
frozen corpus, Playbook makes no claim of demonstrated superiority — the
crate's own tests assert the *mechanics* of honest classification
(inferior performance yields `VALUE_NEGATIVE`; a single winning stratum
yields `VALUE_CONDITIONAL`, never `VALUE_PROVEN`). A hostile reviewer
checks three things: the thresholds predate the results (protocol hash),
the counterfactuals read only decision snapshots, and every stop condition
actually stops. All three are in the test suite.
