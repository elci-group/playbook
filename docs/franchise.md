# Franchise ownership

`playbook-franchise` is the persistent management layer above Playbook's drive
authority. A franchise owns objectives, personnel, allocations, management
decisions and measured history. A player owns a bounded execution assignment.
Replacing a player does not replace its objective or the official drive ledger.

The franchise registers execution resources in its inventory. Front Office
controls eligibility through an offline roster contract; the coaching plane
changes availability, with a required reason in the management history. Each member has a personnel
id, a position label, optional provider/model metadata, and an availability of
`ready`, `benched` or `unavailable`. Personnel kinds remain the capability classes
used by tactical routing. Position labels let the host organise its specialists;
they do not override capability admissibility. Activate a benched substitute
before dispatching it. Only frozen roster members can play; registering a new
resource during a contest does not expand that contest's eligibility. Provider and model metadata stay in franchise storage and
are not added to Lastdown's continuation package.

The coordinator delegates play selection to existing Playbook routing. By
default it considers ready roster members; a `player_id` constrains the next
bounded play to a particular player. Benched, unavailable and unreconciled busy
players are excluded. Changing provider means acquiring a new personnel identity
and selecting it, preserving separate empirical records. Acquiring a member
registers an execution resource; it does not purchase capacity from a provider.

The drive authority is the official. It accepts and checks evidence, computes
yards-to-go, enforces admissibility and determines completion. A completion
request with open mandatory gates records a rejected claim and returns
`completed: false`. Objectives require pristine gate declarations in dependency
order and at least one mandatory gate. Pre-satisfied or waived declarations are
rejected, so creating an objective cannot manufacture progress.

## Objectives and continuity

An objective has an immutable task contract and acceptance gates. It references
one durable drive ledger. Bounded plays and renewed possessions occur inside
that ledger; the franchise does not throw it away when a player fails. Many
independent objectives can coexist in one franchise.

Each drive captures its own `frozen_roster`, including the issued contract and
personnel descriptors. Contracts can be staged while work runs, but activation
requires all franchise contests to be terminal and reconciled. A punt does not
open a transaction window. Readiness can change live within frozen eligibility.
Legacy manual rosters freeze the inventory at kickoff. Existing snapshots that
predate freezing retain their original digest and event logs; recovery freezes
their inventory once and persists that migration. Historical kickoff membership
cannot be reconstructed from those older snapshots.

The ledger retains evidence, validated artifacts, failure observations,
idempotency keys, tactical state and the sealed continuation. The new management
layer uses existing Lastdown disposition, punt and possession rules. `receive`
reserves its new grant against the franchise cap before committing the renewed
possession. Acceptance gates, task identity and observed historical costs survive
that transition. The possession's local budget can reset; franchise allocation
and spending never reset with it.

Every drive event goes through a franchise write-ahead sink. In particular,
`ToolInvoked` reaches durable storage before the capability plane executes it.
If a player or host disappears after dispatch, reopening reconstructs the open
execution. Another play is refused until the host reconciles whether its
idempotency key occurred. Benching or replacing that player does not clear the
uncertainty. Other objectives cannot assign that player while it remains busy.
Reconciliation requires an explicit boolean; omitted information is not treated
as proof that an effect did not occur.

## Salary cap and scouting

The cap uses all eight existing `DriveBudget` dimensions: tokens, money in
micros, wall-clock milliseconds, tool calls, inference calls, retries, context
capacity and provider quota. Each initial or renewed possession reserves a
grant. The sum of lifetime grants cannot exceed the franchise cap in any
dimension, including zero-valued dimensions. Failed reservations leave the
franchise unchanged. Grants remain reserved after completion or abandonment;
this first implementation deliberately has no automatic refund or cap renewal.
`cap_unallocated` therefore reports uncommitted capacity, not unused player
budget. This conservative policy bounds dispatch across simultaneous objectives.

`observed_spend` separately sums returned success, failure and rejected-outcome
costs plus explicit pressure events across all possessions. An unknown result
does not become a measured zero-cost success; `unresolved_invocations` exposes
the unresolved work and its original grant stays reserved. If reconciliation confirms an
execution occurred but its cost was lost, the franchise atomically charges the
entire remaining possession envelope before clearing the invocation. That
conservative charge is reported separately as `unmeasured_spend_reserved`, so
unknown expenditure cannot be silently reused or presented as observed spend.
The next possession needs a fresh grant from unallocated franchise capacity.
A reported overrun is retained even when rejected by the official. Further dispatch is refused if
observed franchise spending exceeds its cap. A budget is a control-plane
contract; an external executor must enforce its execution envelope.

Scouting is derived from evidence-backed historical priors. New drives import
the franchise's measured record, then learn their own observations. Aggregation
subtracts each imported seed so recovery and later drives never count those
samples again. Self-reported distance does not become measured performance.
Franchise records expose active, punted, completed, abandoned and field-goal
counts, measured outcomes, observed spending and unallocated cap.

## HTTP contract

The existing server hosts franchise files under
`<journal-dir>/franchises/<franchise-id>.json`. All request bodies are JSON.
Unknown fields are rejected. Franchise-managed drives cannot be accessed or
mutated through legacy `/v1/drives` routes.

| Method | Route | Body / result |
| --- | --- | --- |
| GET | `/v1/franchises` | Franchise ids |
| POST | `/v1/franchises` | `franchise_id`, `salary_cap` |
| GET | `/v1/franchises/{f}` | Durable franchise, derived record and scouting |
| POST | `/v1/franchises/{f}/roster` | `member` (`RosterMember`), `reason` |
| POST | `/v1/franchises/{f}/roster/{p}/availability` | `availability`, `reason` |
| POST | `/v1/franchises/{f}/roster-contracts/stage` | `contract`, `reason`; no live effect |
| POST | `/v1/franchises/{f}/roster-contracts/activate` | `reason`; between contests only |
| POST | `/v1/franchises/{f}/objectives` | `Objective`, with `drive_id: null` |
| POST | `/v1/franchises/{f}/drives` | `objective_id`, `drive_id`, `budget`, optional `at_ms` |
| GET | `/v1/franchises/{f}/drives/{d}` | Objective id and official tactical snapshot |
| GET | `/v1/franchises/{f}/drives/{d}/events` | Replayable drive events |
| POST | `/v1/franchises/{f}/drives/{d}/preflight` | `{}` |
| POST | `/v1/franchises/{f}/drives/{d}/steps` | Optional `player_id`, `at_ms` |
| POST | `/v1/franchises/{f}/drives/{d}/pressure` | `cost`, optional `at_ms` |
| POST | `/v1/franchises/{f}/drives/{d}/punt` | `{}` |
| POST | `/v1/franchises/{f}/drives/{d}/receive` | `player_id`, `budget`, optional `at_ms` |
| POST | `/v1/franchises/{f}/drives/{d}/reconcile` | `key`, `occurred` |
| POST | `/v1/franchises/{f}/drives/{d}/complete` | Optional `at_ms`; result includes `completed` |
| POST | `/v1/franchises/{f}/drives/{d}/abandon` | `reason` |

`DriveBudget`, `Personnel`, `Gate` and `TaskContract` retain the existing core
wire representations. `RosterMember` and `Objective` are public serde types in
`playbook-franchise`; the integration tests construct complete HTTP requests.
The Rust API accepts a host-provided `CapabilityPlane`, so the host can supply
real execution without making players owners of state.

## Durability and operating scope

Persistence writes a checksummed, schema-versioned snapshot to a pending file,
fsyncs it, atomically renames it and fsyncs the parent directory before returning.
Each snapshot includes all drive event logs. Recovery verifies the digest,
schema, ownership links, budget grants and audited folds. A corrupt franchise
blocks startup instead of silently discarding objectives. Pending files left
before a rename do not supersede the committed snapshot. If directory sync
fails after rename, the writer refuses further mutations until reopened.

Run one service owner per journal directory. The service serializes franchise
operations; this is a single-host store, not a distributed locking protocol.
Full snapshots favour straightforward recovery over large-history throughput.
The HTTP adapter still uses `FixturePlane`; its scripts reset on host restart,
while the durable execution ledger and capability generation survive. Live
Lucid, provider, Bench, TT and CERS adapters remain integration seams, not
implemented network clients. Existing Playbook continuation packages remain
`PLAYBOOK/1`; no external Lastdown repository or wire format is changed.

Validation covers player replacement after accepted progress, disappearance at
dispatch, reconciliation and busy-player exclusion, multi-objective cap
enforcement, possession renewal, false completion, forged gate declarations,
scouting without double counting, checksum rejection and HTTP recovery.
