# Integration

The integration suite is `cargo test -p playbook-sim`.

`tests/mvp.rs` runs the acceptance drive and compares the builtin policy with `playbook-policy-pass-heavy` over the same recorded events. This is a fixed-event replay classification comparison, not a counterfactual benchmark of actions each policy would choose.

The reported trajectory and event metrics describe the observed trace under
the builtin audit baseline; the candidate policy changes only the replay
classification comparison. `budget_spent_ppm` compares the first and last
normalized remaining-budget values, so a possession restart can make it differ
from cumulative resource consumption.

The CLI `sim` command accepts `--trace mvp` or a JSON event-log path and a
known builtin policy name. Its argument validation, policy resolution, and
JSON trace loading are covered by the CLI unit tests.

`tests/failure_injection.rs` covers provider rate limits, authorization stops, timeouts, tool bugs, malformed evidence, compiler failure, unresolved repository mutation, environment failure, unsupported operations, play action, duplicate delivery, stale capability snapshots, budget exhaustion, artefact rejection, field goals with yards remaining, and abandon.

`tests/properties.rs` fuzzes narrative claims, empty waivers, and over-budget spends, then checks replay, witness tampering, and schema fields.
