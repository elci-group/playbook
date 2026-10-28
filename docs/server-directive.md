# Technical Directive: playbook-server — Continuous Drive Service

Status: adopted. Target version: 0.1.x. Owner: playbook maintainers.
Supersedes: the one-shot HTTP/1.0 status surface (`serve_once`, `demo_snapshot`).

## 1. Purpose

`playbook-server` becomes the reference host for the phase-7 contract seam: a
long-lived process that owns drives, dispatches plays against a capability
plane, and demonstrates the durability contract the rest of this repository
enforces — an invocation is fsynced before dispatch, an outcome is retained
even when it is rejected, and every boot is treated as a crash recovery.

The server is the "host" the state-machine documentation addresses directly:
"A host must persist an invocation before dispatching a real tool, preserve
its returned outcome, and reconcile uncertain side effects after a crash."

## 2. Goals

1. One process hosts many drives. Each drive is created over HTTP, advances
   one play per request, and is queryable at any point.
2. Every committed drive event is durable before the requester sees the
   response. The journal is the drive; memory is a cache.
3. Boot equals recovery: journals on disk are folded back into live drives at
   startup. A drive killed mid-invocation reappears with its open execution
   and demands reconciliation before it advances.
4. The HTTP surface is a thin, testable translation layer over `Authority`.
   No drive logic lives in the server.
5. The capability plane is `FixturePlane`. The seam is real; the plane is not
   the production ELCI daemon.

## 3. Non-goals

- No TLS and no authentication. The default bind is loopback; the deployment
  boundary is the host. A fronting proxy owns transport security.
- No multi-node coordination, no drive migration between processes.
- No assessments, narrative evidence, or continuation intake over HTTP; the
  continuation package is produced by the drive machine, not accepted by it.
- No live ELCI. A production plane is an integration task outside this
  repository.

## 4. Architecture

Single process. `std::net::TcpListener`, one thread per connection, HTTP/1.1
with `Connection: close` semantics per request. A `Service` maps
`(method, path, body)` to a JSON response; the socket layer only parses and
serializes. Drive state lives in a `BTreeMap<String, DriveHandle>` behind one
`Mutex`; a `DriveHandle` pairs an `Authority` with its `Journal` and a
monotonic logical clock. All drive mutation goes through `Authority`, which
remains the single writer.

Durability: each drive owns a `playbook_journal::Journal` under the journal
directory, attached before the first event. Because `Authority::push` fsyncs
before commit, every response that reports new events implies those events
are on disk. Graceful shutdown therefore reduces to bounded request draining;
process kill is safe by construction and uses the same recovery path as boot.

Capability plane: each drive gets a fresh `FixturePlane::mvp()`. Play
outcomes, including induced failures, are drive events, not transport errors.

## 5. HTTP contract

All bodies are JSON. Errors use one envelope:

```json
{"error": "machine_code", "detail": "human readable"}
```

| Method | Path                          | Purpose                              |
|--------|-------------------------------|--------------------------------------|
| GET    | `/healthz`                    | liveness; `{"ok": true}`             |
| GET    | `/readyz`                     | readiness; `{"ok": true, "drives": n}` |
| GET    | `/v1/drives`                  | list of drive ids                    |
| POST   | `/v1/drives`                  | create a drive                       |
| GET    | `/v1/drives/{id}`             | tactical snapshot                    |
| GET    | `/v1/drives/{id}/events`      | full event log                       |
| GET    | `/v1/drives/{id}/trace`       | OTLP-shaped span list                |
| POST   | `/v1/drives/{id}/preflight`   | capability discovery + preflight     |
| POST   | `/v1/drives/{id}/steps`       | route and execute one play           |
| POST   | `/v1/drives/{id}/pressure`    | consume external budget              |
| POST   | `/v1/drives/{id}/punt`        | seal and issue the continuation      |
| POST   | `/v1/drives/{id}/reconcile`   | reconcile an idempotency key         |
| POST   | `/v1/drives/{id}/complete`    | claim completion                     |

`POST /v1/drives` body:

```json
{"drive_id": "drive-1", "statement": "...", "workload_class": "engineering",
 "tokens": 1000, "quota": 1000, "policy": "builtin"}
```

`policy` is `builtin` or `pass-heavy`; omitted means `builtin`. The server
installs the standard gate set and the standard roster. `drive_id` must pass
`DriveId::new` and must not collide with a live or journaled drive.

`steps`, `pressure`, `punt`, `preflight`, `reconcile`, and `complete` accept
an optional `"at_ms"`. When omitted the server advances the drive's logical
clock by 100 ms from the last event.

Responses: creation and mutations return
`{"snapshot": <TacticalSnapshot>, "decision": <DecisionRecord>?}`. Reconcile
returns the snapshot. `GET events` returns the serialized event log.

Status codes: 200 on success; 201 on creation; 400 malformed JSON, unknown
policy, or invalid drive id; 404 unknown drive; 409 when the machine refuses
the transition (open execution, unresolved effects, blind retry, punt not
ready, incomplete completion) with `error` set to the machine code and the
idempotency key in `detail` when present; 500 journal IO failure; 503 when
the drive cap (256) is reached.

## 6. Recovery

On boot the server scans the journal directory. Each `*.jsonl` file is opened
with `Journal::open` (digest and sequence checked, torn tail repaired by
truncation) and its events folded in audit mode. A journal that fails to fold
is quarantined: it is renamed with a `.rejected` suffix and surfaced on
`/readyz` as `"rejected_journals": n`. It is never deleted.

An open execution after recovery appears in the snapshot as
`active_capability` and blocks advancement exactly as in-process: the next
`steps` call returns 409 until `reconcile` closes the key.

## 7. Observability

One log line per request on stdout: `method path status micros`. `GET
/v1/drives/{id}/trace` returns the `playbook_observability` span list. When
`--otlp <endpoint>` is configured, a drive that reaches a terminal status
exports its trace through `playbook_otlp::export` once; the export result is
logged, never fatal.

## 8. Resource limits

Request headers: 8 KiB. Request bodies: 1 MiB. Read timeout: 10 s per
request. Live drives: 256; further creations return 503. Journal directory:
one file per drive, created at drive creation.

## 9. Configuration

```
playbook-server [--bind ADDR] [--port PORT] [--journal-dir DIR] [--otlp URL]
```

Defaults: bind `127.0.0.1`, port `8088`, journal dir `./playbook-journals`,
OTLP off. The listening address is printed to stdout as
`listening http://ADDR:PORT`. Exit code 0 on clean stop; nonzero on bind
failure or an unreadable journal directory.

## 10. Acceptance criteria

Each criterion is enforced by an automated test on a real socket.

- AC-1 Lifecycle: create, preflight, step to fourth down, pressure, punt; at
  every point the reported snapshot matches the fold of `GET events`; the
  journal on disk replays to the same state.
- AC-2 Crash recovery: a journal is seeded with a drive whose last event is
  `ToolInvoked` (outcome lost); after boot the snapshot shows the open
  execution, `steps` returns 409, `reconcile` closes it, and the drive
  advances afterwards.
- AC-3 Error contract: unknown drive is 404; malformed JSON is 400 with the
  error envelope; a drive with an unresolved effect refuses `steps` with 409.
- AC-4 Journal authority: for every drive at every point, folding the journaled
  events reproduces the live state; a response never precedes its fsync.
- AC-5 Recovery on boot: restart the process against the same journal
  directory; every drive reappears with an identical snapshot.
- AC-6 Limits: a 2 MiB body is rejected; the error envelope is stable.

## 11. Docs

The README delivery status must stop claiming the server serves one fixed
snapshot. This directive is the server's contract of record.
