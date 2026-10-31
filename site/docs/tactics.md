# The tactical layer

The authority crates decide *which personnel act*. `playbook-tactics` decides
the organisational shape those personnel take — what information they
receive, what interaction constraints they operate under, and what tactical
objective governs the play. It sits between the franchise and the execution
authorities: **it recommends and structures; the existing authorities
execute and validate.**

The design rule: a football concept earns a place in Playbook only when it
creates a distinct, testable, economically meaningful orchestration
behaviour. Every tactic is a `TacticalDirective` from the frozen contract —
a class (formation / play / adjustment / doctrine), a deterministic trigger
over measured context, a personnel shape with explicit domain ownership, an
information topology, a concurrency and authority policy, a bounded budget,
an expected effect, a termination condition, and a declared counter.

## Authority boundary

A tactic may alter assignments, information distribution, the active
personnel shape, concurrency, budget allocation, and observation strategy.
It may never alter evidence truth, YTG, gate satisfaction, objective
definition, frozen roster eligibility, the salary cap, or an accepted tool
result. Only the existing authorities do that.

## The eight foundational tactics

| Module | Tactic | Orchestration behaviour |
|---|---|---|
| `formation` | Spread (+ the formation engine) | broad parallel ownership, minimal context overlap; the deterministic planner ladders critical risk → Heavy, a discovered blocker → Trips, divergence → Bunch, breadth+looseness → Spread |
| `protect` | Heavy / Max Protect | generative breadth collapses to one executor; every other seat is verification, specialist support, reserves, observation; unknown spend reserved, protection scored |
| `adjustment` | Motion / Shift / Audible / Check-With-Me | pre-dispatch adaptation: tiny diagnostic probes, responsibility moves on a constant roster, plan replacement before irreversible execution only, deterministic alternative selection |
| `sequencing` | Constraint / Play Sequencing | tactical history, overreaction detection (repeated calls with degrading returns), and the constraint table (spread→bunch, verticals→prevent, flood→make-them-drive, heavy→spread) |
| `diagnostic` | Mesh / Flood / Levels | crossed-domain traversal whose crossing point is the reconciliation target; one domain attacked at all abstraction depths; progressive granularity |
| `conservative` | Prevent / Make Them Drive | risk-state classification, maximum change envelopes, block large unverified changes, staged small validated changes |
| `surveillance` | Spy / Bracket | observe-only watchers with drift bands and an intervention envelope that stops at recommendation; two different controls on one critical domain |
| `exchange` | Banjo / Scrape Exchange | joint ownership pairs with deterministic switching; explicit journalled responsibility handoffs with stale-owner rejection |

## Admission, composition, conflict

`admission::admit` is the generic rule: the trigger must fire on measured
context AND net expected value (benefit − cost − risk) must clear the policy
threshold. Rejections carry machine reason codes and are deterministic.

`composition::compatibility` is the machine-readable conflict matrix, total
and symmetric over `KNOWN_TACTICS`. `Spread + Mesh` composes; `Spread +
Heavy` are exclusive (the plan keeps the first-listed and drops the later);
`Empty + Max Protect` is invalid outright; `Prevent + Verticals` conflict.
`compose` fails closed: unknown tactics are refused, one name cannot belong
to two classes in one plan, and invalid or conflicting pairs are errors.

## Journal and memory

Five event kinds — `tactic_admitted`, `tactic_rejected`,
`tactic_completed`, `tactic_failed`, `tactic_countered` — are journaled on
the drive ledger and replayed like everything else.
`DriveState::tactics()` is the multi-play tactical memory: recent windows,
consecutive-call runs (the overreaction detector's input), and per-outcome
totals. No learned opaque routing exists in this sprint; the selection rule
is deterministic arithmetic.

## The acceptance benchmark

`playbook-sim/tests/tactical_game.rs` runs the six-phase shape-shifting
campaign from the sprint directive: broad unknown problem → Spread;
discovered blocker → concentration; divergence → Bunch with a changed
information topology; high-risk work → protection doctrines; new runtime
information → a lawful pre-dispatch audible (and the structural refusal of a
post-dispatch one); fragile release state → Prevent and staged Make-Them-
Drive. The same roster and objective produce different arrangements, topologies,
and measured results in every phase — and every tactical decision replays.
