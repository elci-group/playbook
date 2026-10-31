# The enterprise integration layer

Six external systems surround the Playbook control plane, and every one of
them is an advisory or delegated surface. Playbook owns the game. The
dependency direction is one-way and non-negotiable:

```text
Playbook authority → state transition → journal
                                    └→ integration representation
```

never the reverse. Each subsystem in `playbook-integration` is a contract
plus a deterministic decision procedure with an explicit
unavailable-fallback — and no advisory failure can corrupt authoritative
state.

## The six surfaces

| System | Football role | Computational role | Hard boundary |
|---|---|---|---|
| **Mesut** | The offensive line | Execution substrate routing beneath `playbook-live`: Tokio, Rayon, or blocking pools; queue placement; scrambles under measured pressure | May move *where and how* work runs; may never change capability, evidence, authority, side-effect permission, objective, salary cap, or tactical directive |
| **REECE** | Reading the defence before the snap | Recursive ecosystem concurrency consideration: a bounded, read-only query over adjacent-project facts | Informs the read; never calls the play. Recursion is budget-bounded |
| **Dreamseq** | The film session | Post-drive review built from the journal: personnel, tactics, economics, recovery, infrastructure displacement KPIs | Reviews closed drives; never reopens or rewrites them. Recommendations enter planning as evidence, not commands |
| **Hybreed** | The personnel decider between competing candidates | Semantic reconciliation of divergent implementations: which traits survive, which conflict | Never modifies Prime; plans only |
| **Emulsify** | The scheme-fitter | Structural compatibility classification (Mix / Emulsify / Suspend / Separate) and merge-unit planning | Plans materialisation; never authoritatively merges |
| **Theosis** | The equipment manager | Capability freshness: installed-vs-remote classification, preflight before a campaign, maintenance-window updates through Baby | Never downgrades; never updates mid-play; a failed build preserves the old version |

Kaptaind remains the transaction authority for consolidation: freeze
candidate SHAs, verify provenance, apply the plan, run acceptance gates,
compare the UNI score, roll back on regression, record immutable
provenance. No Hybreed/Emulsify output bypasses it.

## The Mesut seam

An authorised invocation converts to a typed `MesutExecutionRequest` — work
kind, priority, hard resource constraints (immutable), soft preferences,
cancellation contract, deadline, and evidence expectations. Routing is
deterministic: blocking tools run on the blocking pool, CPU-bound analysis
on Rayon, I/O on Tokio; a preferred substrate is honoured only when safe.
Queue position is priority-ordered with arrival tiebreaks.

**Scramble** is dynamic local re-routing under measured pressure — queue
congestion, blocked workers, CPU saturation, I/O stalls, service-time
shifts, imminent deadlines. A scramble is lawful only when the preserved
contract is intact (same capability, evidence, authority, objective) and
the work is cancellable or has not started. It journals as execution
metadata, never as a new tactical play.

## REECE admission and authority

REECE runs only when the action warrants it: API mutations, dependency
replacements, architectural changes, consolidations, provider substitutions
with ecosystem effect. A local typo fix needs no ecosystem recursion. The
consideration record carries projects, capability edges, conflicts,
projected impacts, and ownership constraints with a per-mille confidence —
and the QB receives a compressed field read (threats, open receivers,
coverage, risk), never the full recursive graph.

## Dreamseq review and displacement

The review package is built from the journal after authoritative closure —
opening and closing progress, personnel, tactics with YTG deltas,
substitutions, spend vectors, failures, and recovered CERS knowledge. The
§18 questions are evaluated into structured findings, and the §20
displacement KPI is explicit: expensive reasoning → cheap rehearsal →
stable pattern → Rust/Tov infrastructure. Packages are redacted before
leaving: credential needles, bearer tokens, and absolute home paths never
travel.

## Theosis preflight

Before a Franchise campaign, a bounded preflight classifies every tool:
Current, UpdateAvailable, UpdateRequired, Unavailable, or UpdateFailed.
Policy per class: Current continues; UpdateAvailable continues (the update
waits); UpdateRequired outside a maintenance window refuses the affected
capability. Automatic downgrade is forbidden — an older remote never
triggers action — and a failed build preserves the old version.

## Journal and 3form

All fifteen integration events (`mesut_execution_routed` through
`reece_qb_consideration_returned`) are journalled and replayed like every
other event; they land in `DriveState::integrations()` as an advisory
ledger that cannot alter game truth. In the 3form structural account,
integration spans carry `advisory=true` so external nodes are visually and
structurally distinguishable from authoritative Playbook nodes.

## Failure policy

Every integration fails independently: Mesut unavailable → ordinary
playbook-live execution; REECE unavailable → the QB continues with local
evidence; Dreamseq unavailable → the drive closes with the review omitted;
Hybreed unavailable → candidates stay unconsolidated; Emulsify unavailable
→ no materialisation; Theosis unavailable → current verified versions
remain in use. No optional failure corrupts authoritative state.
