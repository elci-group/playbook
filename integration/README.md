# Integration

The integration suite is `cargo test -p playbook-sim`.

`tests/mvp.rs` runs the acceptance drive and compares the builtin policy with `playbook-policy-pass-heavy`.

`tests/failure_injection.rs` covers provider rate limits, authorization stops, timeouts, tool bugs, malformed evidence, compiler failure, unresolved repository mutation, environment failure, unsupported operations, play action, duplicate delivery, stale capability snapshots, budget exhaustion, artefact rejection, field goals with yards remaining, and abandon.

`tests/properties.rs` fuzzes narrative claims, empty waivers, and over-budget spends, then checks replay, witness tampering, and schema fields.
