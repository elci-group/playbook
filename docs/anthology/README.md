# The Playbook Documentation Anthology

Supporting documentation for Playbook — a deterministic, evidence-governed
organisational control plane for heterogeneous AI and tool personnel. The
anthology is the enterprise documentation set: twelve chapters that explain
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
| 09 | [Module playbook](09-module-playbook.md) | All readers | How do the crate groups and features fit together? |

| 10 | [Feature walkthroughs](10-feature-walkthroughs.md) | New operators | How do I run, inspect, and recover each feature? |
| 11 | [Crate atlas](11-crate-atlas.md) | Students and contributors | What does each of the 44 workspace crates own? |

## Choose your first play

![Playbook](../../assets/segmented/01-primary-horizontal-lockup-transparent-padded.png)

Start with [the module playbook](09-module-playbook.md), follow [the feature walkthroughs](10-feature-walkthroughs.md), then use [the crate atlas](11-crate-atlas.md) to find code. Read chapters 02, 03 and 05 before operating live integrations. Chapters 04 and 06 cover recovery and quality gates.

Each chapter includes a play card or concrete workflow and a visual diagram. A drive is a bounded attempt at a goal; a gate is an acceptance condition; yardage is evidence-backed progress. Football knowledge is not required.

## Visual identity

The [asset catalog](../../assets/README.md) documents six regions and four variants per region, covering transparent margins and white-ink knockout. Padded transparent marks provide chapter identity; diagrams explain relationships and failure paths.

## Normative sources

The anthology summarises; the contracts govern. The normative documents are
`docs/franchise.md`, `docs/huddles.md`, `docs/blitz.md`, `docs/tactics.md`,
`docs/live-runtime.md`, `docs/closure-campaign.md`, and the crate-level
documentation in `crates/*`. Where this anthology and a contract disagree,
the contract wins and the anthology should be corrected.
