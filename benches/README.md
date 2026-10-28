# Benchmarks

Policy comparison is a resimulation, not a wall-clock microbenchmark.

```bash
cargo run -p playbook-cli -- sim --trace mvp --policy playbook-policy-pass-heavy
```

The report keeps yards-to-go, budget, and completion aligned, and shows where the down labels diverge. Add a historical trace by folding its event log through `playbook_sim::compare_policies`.
