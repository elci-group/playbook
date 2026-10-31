# Matchday: the presentation contract

Playbook Matchday is the operational command environment: **Football Manager
× broadcast telemetry, rendered through a terminal** — not a dashboard with
its own interpretation of events. The design rule is absolute:

> The TUI is a projection of authoritative Playbook state. Football
> semantics are presentation metadata over machine state; nothing in the
> presentation layer interprets, it maps.

## Layering

```text
authoritative state (DriveState, folded from the journal)
     ↓
playbook-presentation   deterministic 3form semantic projection
     ↓
playbook-tui            first renderer (frames, CLI, film room)
```

`playbook-presentation` carries no terminal assumptions and no colours —
the same projections can drive the terminal, the web dashboards, the play
cards and a replay recording without duplicating interpretation logic. It
extends the `playbook-cards` invariant: visualisation is generated from the
same data the authority executes, so doctrine and diagrams cannot drift.

## The 3form protocol

Every important Playbook noun or event has three forms of the same truth:

* **glance** — one compact line (`FIRST DOWN • +10 • boundary ✓`);
* **tactical** — the spatial block (the field rail, the route, the
  topology);
* **analytical** — forensic `(label, value)` rows for depth disclosure.

`present_event(&DriveEvent) -> ThreeForm` is the per-event projection;
`PresentationSnapshot::from_state(&DriveState)` is the whole-drive
projection. Both are pure, `Eq`, and serialisable — the same state always
projects the same snapshot, so `Audit` and `Resimulate` replays project
identically (I-17).

## The snapshot

The `PresentationSnapshot` carries, and nothing more than, what the
authoritative state can answer:

* **ScoreBug** — drive id, down label, YTG in milli-yards, Δ yards this
  cycle, completion, spend, the broadcast clock (consumed wall time),
  status and disposition.
* **FieldProjection** — the semantic field from the gate graph: declared
  length (Opposition Kicker), banked vs unvalidated yards, every validation
  boundary with its state, Red Zone, field-goal range, kick outcome,
  touchdown.
* **Roster / PlayerRow** — the active player, assessed personnel and the
  bench, each with measured reliability and quota, never self-report.
* **Plays** — the admitted tactic history from the TacticsLedger.
* **GateRows** — the game-plan board: mandatory flags, gate states,
  satisfaction and confidence.
* **ShadowCompare** (optional, built by the caller from
  `playbook_field::shadow`) — the best credible line-up Playbook did not
  field, with the signed opportunity-cost delta and whether the starters
  sit on the Pareto frontier.
* **BlitzOverlay** — the live blitz: parallel runners, candidates, the
  verified winner, waves.
* **HuddleStrip** — active coordination topology.
* **SubstitutionRow** — the medical/board record.

## Film Room: the TUI is replayable

Because Playbook's journal is deterministic, the UI replays it:

```text
playbook-tui --replay journal.jsonl                 # step through, film review
playbook-tui --replay journal.jsonl --frame 42      # one frame
playbook-tui --replay journal.jsonl --form broadcast
playbook-tui --live   journal.jsonl                 # follow a live journal
```

`frames(&events, &policy)` folds the journal stepwise into one frame per
event, each frame carrying the event's three forms plus the full snapshot.
Frame *i* is asserted equal to the snapshot of the prefix fold — the film
cannot drift from what the authority actually did.

Film-room keys: `ENTER`/`n` next frame, `p` back, `f` cycles the depth
switch (glance → tactical → forensic), `k` the kick decision moment,
`d` the Lastdown decision moment, `r` roster depth, `h` the heat map,
`g` jump to the final whistle, `q` quit.

## The three principal experiences

```text
MANAGER MODE  → control    (the dugout: formation, roster, play call)
BROADCAST     → observe    (scorebug, field rail, evidence, overlays)
FILM ROOM     → understand (frame-by-frame replay of any journal)
```

Design constraint: no dense hacker-terminal aesthetics. Large empty
regions, strong typographic hierarchy, a small number of huge statistics,
persistent scorebug, restrained semantics.

## Heat map

The possession heat map places recent work on four action phases —
defensive, possession, progressive, clinical — by vocabulary kind. It shows
**style of work**, not token consumption.

## Tests

The projection campaign lives in
`crates/playbook-presentation/tests/projection.rs` (snapshot purity, field
geometry, film parity, three-form coverage) and the Matchday campaign in
`crates/playbook-tui/tests/frames.rs` (golden scorebug, dugout, broadcast,
overlays, Shadow Defence on the dugout, and a full film-room replay of a
real MVP journal with per-frame parity).
