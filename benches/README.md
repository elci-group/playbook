# Benchmarks

This command compares classifications while replaying one fixed event log.
It is not a wall-clock microbenchmark and does not generate counterfactual
actions or measure what a policy would achieve if it had controlled the drive.

```bash
cargo run -p playbook-cli -- sim --trace mvp --policy playbook-policy-pass-heavy
cargo run -p playbook-cli -- sim --trace fixtures/sprint_transcript.json --policy builtin
```

`--trace` accepts `mvp` or a JSON file containing a serialized `Vec<DriveEvent>`.
`--policy` accepts `builtin` / `playbook-policy-1` or `pass-heavy` /
`playbook-policy-pass-heavy`; it defaults to the pass-heavy contrast. The
The selected policy is used to fold the same events in resimulation mode,
while the builtin policy supplies the audit baseline. Most reported values
describe the observed trace under that builtin audit baseline: YTG trajectory,
completion status, budget remaining, fourth-down position, gates, time, cost,
and event-log counters. `tool_failures` counts both failed tool executions
and rejected tool outcomes. The selected policy affects the match and down-label
comparison fields; these results do not describe actions it would have chosen.

`budget_spent_ppm` is the difference between the first and last normalized
remaining-budget observations. A possession restart can allocate a fresh
budget, so this value and its derived efficiency ratio are net-change
indicators, not cumulative resource consumption across a full trace. A
historical trace can also be compared through
`playbook_sim::compare_policies_with`.
