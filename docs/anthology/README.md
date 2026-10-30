# The Playbook Documentation Anthology

Supporting documentation for Playbook — a deterministic, evidence-governed
organisational control plane for heterogeneous AI and tool personnel. The
anthology is the enterprise documentation set: eight documents that explain
the system to the audiences who must trust, operate, secure, and fund it.
Each document is grounded in the crates it describes and cross-references
the contract documents that carry the normative rules.

| # | Document | Audience | Question it answers |
|---|---|---|---|
| 00 | [Executive overview](00-executive-overview.md) | Executive stakeholders | What is Playbook, what does it guarantee, and why does it matter? |
| 01 | [System architecture](01-system-architecture.md) | Engineering leads | How is the system structured, and how do the crates compose? |
| 02 | [Authority and governance](02-authority-and-governance.md) | Architecture reviewers | Who may decide what, and where are the boundaries? |
| 03 | [Evidence, journal, and replay](03-evidence-journal-replay.md) | Reliability engineers | How does the system know what was true, and can it prove it again? |
| 04 | [Operations runbook](04-operations-runbook.md) | Platform operators | How is a game run, and what happens when things fail? |
| 05 | [Security and boundaries](05-security-and-boundaries.md) | Security reviewers | What can personnel touch, and what can they never touch? |
| 06 | [Metrics and quality gates](06-metrics-and-quality-gates.md) | Engineering management | How is the organisation measured, and what blocks a release? |
| 07 | [Glossary and event index](07-glossary-and-event-index.md) | All readers | What do the words mean, and what events exist? |
| 08 | [Personnel and organisation](08-personnel-and-organisation.md) | Engineering leads, operators | How is the organisation staffed, and who owns which problem? |

## Normative sources

The anthology summarises; the contracts govern. The normative documents are
`docs/franchise.md`, `docs/huddles.md`, `docs/blitz.md`, `docs/tactics.md`,
`docs/live-runtime.md`, `docs/closure-campaign.md`, and the crate-level
documentation in `crates/*`. Where this anthology and a contract disagree,
the contract wins and the anthology should be corrected.
