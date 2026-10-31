# The command surface, argued in PEEVE

Each claim in this document is argued in five steps: **Point** (the claim),
**Evidence** (what the machine actually did), **Explanation** (the
mechanism underneath), **Visualisation** (a recording made from the same
binary the authority runs), and **Evaluation** (how you would catch it
failing). The recordings are rendered by
[vhs](https://github.com/charmbracelet/vhs) from the tapes in `docs/tapes/`;
regenerate any of them with `vhs docs/tapes/<name>.tape`. They are not
mock-ups — every frame is the installed binary folding real state.

---

## The acceptance drive reads like a broadcast

**Point.** `playbook mvp` is the whole product in one command, and its
output should read like a match report — verdict first, statistics after —
not like a log.

**Evidence.** The drive ends with a green **✓ replay clean** verdict
("the replayed state equals the live state"), the final disposition
(`punt`), and the integrity counters (false completion claims, duplicate
failed plays, control duplicates). Above them, the stage bar counts the
six downs and the headline names the segment.

![playbook mvp — the acceptance drive with verdict](assets/cli-mvp.gif)

**Explanation.** The verdict is not decoration: `Audit` and `Resimulate`
modes fold the same journal independently, and the ✓ is printed only when
the two states compare equal (I-17). The counters beneath it are the
authority's own admissions against itself — a false completion claim is a
completion the drive refused, printed even though it incriminates the
refuser.

**Evaluation.** Pipe it: `playbook mvp | cat` must carry zero escape
codes and the identical facts. Force `--basic`: the stage names and
`replay_match true` remain. If the ✓ ever prints while
`replay_match false`, the presentation layer has diverged from the
authority — that is the bug, and the sim command exists to catch its
siblings.

---

## A policy change is shown, not asserted

**Point.** `playbook sim` answers "what would this policy have moved?"
with three replay checks and an economics table — never with prose
opinions.

**Evidence.** The comparison opens with three green verdicts (yards-to-goal,
budget and completion replays all match), then the moved quantities:
yards gained, gates satisfied, efficiency ratio, tool failures, spend,
and the yards-to-goal trajectory frame by frame.

![playbook sim — policy comparison with replay verdicts](assets/cli-sim.gif)

**Explanation.** Sim is a counterfactual run under a folded policy
against a recorded trace. The three verdicts are the load-bearing claim:
they prove the counterfactual is a fair replay of the same game, not a
strawman. Only then do the economics numbers mean anything.

**Evaluation.** `down_labels_differ yes` between the builtin and
pass-heavy policies is expected — different doctrine, different labels.
If the replay verdicts go red, the comparison is invalid regardless of how
favourable the economics look; the tool refuses to let a policy look good
by replaying a different game.

---

## The journal loop closes on camera

**Point.** A drive that journaled can be folded back from disk, and what
survives is exactly what provably happened — the loop is short enough to
film.

**Evidence.** One tape, three beats: `mvp --journal /tmp/peeve-drive.jsonl`
plays and writes 61 events; `recover` on that journal folds them back and
reports `status Completed, events 61, unresolved_effects 0`; `recover` on
a missing journal refuses with `io: No such file or directory` instead of
inventing a drive.

![the journal loop — play, recover, refuse](assets/cli-recover.gif)

**Explanation.** Recovery truncates at the first torn, tampered or
out-of-sequence record rather than skipping it, so a folded drive is
bit-honest to its journal. The refusal on the missing file is the same
discipline pointed at the filesystem: absence is not silently forgiven.

**Evaluation.** Flip one byte inside a journal event and recover: the fold
must stop at that record, not past it. `playbook recover` on the truncated
prefix must report the shorter event count — recovery is a prefix
guarantee, and the tape's third beat is the trivial prefix of zero events
refusing outright.

---

## The help is a tour, not a tax

**Point.** `playbook --help` should be the fastest way to understand the
franchise — verbose, plain-spoken, and pleasant enough that people read it.

**Evidence.** Seventy-three lines, three sections: every command gets a
full-sentence description of what it does and when to reach for it; the
output-style contract is spelled out; three worked examples end the tour.

![playbook --help — the animated command surface](assets/cli-help.gif)

**Explanation.** The reveal is animated line-by-line on a terminal (9 ms
per line — a curtain, not a circus) and printed instantly and plainly when
piped. Help goes to stdout, so it greps; a closed pipe ends the reveal
quietly instead of panicking on SIGPIPE.

**Evaluation.** `playbook --help | head -3` must exit 0 with no panic;
`playbook --help | grep -i punt` must find the fourth-down story. The
`--basic` help carries no paint at all. The test
`help_is_verbose_and_pipes_plain` pins all of this.
