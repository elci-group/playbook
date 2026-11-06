# Kaptaind reliability: observed stalls, failure contract, reproduction plan

**Status:** investigation record (§33) + the §34 failure contract as implemented in `playbook-resolution`.

## Observed defects (2026-10-31, /home/sal/playbook)

Two distinct stall classes, both reproducible-in-kind, both silent:

**S1 — binary artefact in the cluster analysis set.** A working tree whose
touched paths included a `.gif` caused the scheduler to log
`cluster window expired by timeout` and then hang indefinitely — no test
hook, no analysis artifact, no commit. Removing the binary from the
touched set restored normal clustering within one cycle (verified twice:
the `site/assets` incident and the earlier `docs/assets` incident).

**S2 — post-expiry stall, restart-resistant.** With a text-only working
set, clusters expired (`cluster window expired by timeout`, ids observed
at 22:12, 22:39, 22:56, 23:02) and the scheduler returned to `Idle`
without running the test hook. One daemon restart restored function once;
a later occurrence survived a restart. The daemon's own log shows no
error between expiry and silence — the pipeline dies between cluster
expiry and test-hook spawn. Evidence contrast: successful commits the
same day (v0.1.56–59) show `cluster window expired` followed by test-hook
and commit activity within seconds.

## Ranked hypotheses (from observed behaviour; source-level confirmation pending)

1. **(S1, most likely)** The post-expiry analysis path assumes UTF-8 or
   parseable content in touched paths and blocks or aborts without
   logging on binary input — consistent with the stall tracking the
   gif's presence exactly.
2. **(S2)** A state-machine transition after cluster expiry waits on a
   resource that is never signalled — an internal channel consumed by a
   worker that exited, or a lease/lock taken before the test hook and
   orphaned when an earlier failed run bypassed its release path. The
   restart-resistance being intermittent suggests a race rather than a
   deterministic deadlock.
3. **(S2 variant)** `min_commit_interval`/backoff state persisted across
   cycles after a hook failure, suppressing the hook silently — a
   backoff without a log line.

## Reproduction plan (deterministic)

1. Replay a dirty working tree containing one binary touched path and
   one text path; assert the test hook spawns (it did not).
2. Force a first hook failure (failing fmt), then repair; assert the
   next cluster proceeds (it did not, post-restart).
3. Property: every `cluster window expired` log line is eventually
   followed by either a test-hook start or an explicit skip reason,
   within a bounded interval — no silent expiry.

## The §34 failure contract (implemented)

`playbook-resolution` treats version-management failure as first-class:
`RefusalCause::VersionControlFailure` (governance family) classifies as
`Certain` from explicit machine state and prescribes `Escalate` only —
never blind retry, never an assertion that a commit occurred. The
reconciliation semantics mirror live dispatch: a commit whose outcome is
uncertain is `Uncertain` until reconciled against observable facts; the
seven-intervention corpus test pins the wedge case as an escalation
(`phase_b` asserts the escalation count).

**A failed kaptaind action must never: pretend a commit occurred; advance
state against an uncommitted tree; lose evidence lineage; retry blindly
where commit outcome is uncertain.**

