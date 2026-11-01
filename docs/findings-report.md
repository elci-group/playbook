# MAJOR FINDINGS REPORT

**Programme:** Playbook — Consolidated Evaluation
**Period:** This session (v0.1.58 → v0.1.66)
**Scope:** Project VALUE live draftboard (H5), the real-world workflow experiment (five governed runs, free-model builders), the Evidence-Gated CI campaign, and the Matchday/presentation surface
**Standing rule:** every number below is measured; every criticism is on the record.

---

## FINDING 1 — The platform's referee role is production-grade

**Point.** Across every campaign, Playbook's authority never accepted work that failed its gates and never rejected work that passed.

**Evidence.** Five live foreman runs over the real OpenRouter pool: runs 1–4 ended `completion claimed: false` — recorded as failures in the journal, with per-model failures named (provider errors, missing content, dropped crates) — and run 5 ended `completion claimed: true` only when all-model content, content completeness and deterministic assembly all measured green. The CI pipeline's first full run recorded two honest `fail` outcomes (Traci flag incompatibility, Amber workspace-root limitation) as non-blocking findings rather than hiding them. The MVP acceptance drive has never printed `✓ replay clean` without Audit and Resimulate folds comparing equal.

**Explanation.** The gate architecture is mechanically derivable: acceptance is a function of the evidence object, not of any participant's opinion — including the orchestrator's. This is the property the whole architecture exists to protect, and it held under the least favourable conditions (a subject trying to pass work the gates considered incomplete).

**Evaluation.** Any future failure of this property — a pass printed while a gate failed — is by definition a presentation-layer bug and is trivially detectable (the report suite asserts it).

---

## FINDING 2 — Live free models are usable but wildly heterogeneous

**Point.** The free provider pool is real, but completion on trivially verified tasks ranges from 100% to 0%, and rank order is not stable across runs.

**Evidence.** Two full draftboard runs over the same six models and six validated tasks: `nvidia/nemotron-3-nano` 6/6 both runs (median ~0.8–1.0 s); `cohere/north-mini-code` regressed 6/6 → 5/6 between runs; both `gemma-4` variants scored 0/6 at ~220 ms — error responses, not model stupidity; `liquid/lfm-2.5` 1/6 → 2/6.

**Explanation.** Free-tier pool composition changes, provider-side errors present as instant failures, and per-model reliability moves between runs. Any roster built on a snapshot ages immediately.

**Evaluation.** The draftboard must be re-run before each selection decision; a roster frozen today is a hypothesis, not a fact. The measured numbers are now on the site and in `draftboard-franchise.json`, so drift is visible by diffing two runs.

---

## FINDING 3 — The real-world workflow took five runs and seven operator interventions; four were harness repairs the platform should have self-performed

**Point.** Playbook refused everything imperfect (correctly) but could not diagnose or repair its own harness; every gap between "refused" and "admitted" was closed by a human.

**Evidence.** The intervention audit (published on the site, `site/interventions.json`):
1. The foreman harness itself had to be written — Playbook's tool plane is fixture-backed and cannot yet dispatch open-ended authoring.
2. Run 1 mass rejection: markdown fences + literal heading matching — a harness rigidity bug.
3. Run 2 mass rejection: `max_tokens: 200` starved reasoning models; `finish_reason: "length"` decapitated the HTML — an envelope bug.
4. Runs 3–4: 38-crate transcription truncation — fixed by chunking to eight cards (work decomposition, standard unmoved).
5. First-card drops — fixed by first/last anchors in the brief.
6. A pipeline collision: deliver's generator command silently overwrote the model-authored page — resolved by explicit output-path ownership.
7. Shell/server hygiene and one manual commit when the kaptaind scheduler stalled.

**Explanation.** The critical separation held: the gates never lowered, the fallbacks never faked a pass, and the final admission was earned by genuinely model-authored content. But the loop "validate → diagnose → repair harness → retry" ran entirely outside the platform. The value-lab integrity findings apply to CI tooling too: the judge's own defects were found by a human reading raw provider responses.

**Evaluation.** Re-run the foreman monthly. The target state is that interventions of type 2, 3 and 4 become platform-owned: validation failures fed back into automatic brief repair, token-envelope sizing from observed `finish_reason`, and work decomposition from observed truncation. Each is mechanically derivable from data the foreman already records.

---

## FINDING 4 — The CI federation admitted its first real changeset, and caught real tool incompatibilities on day one

**Point.** The Evidence-Gated CI pipeline ran end-to-end on an actual revision and produced a mechanical `ADMITTED` verdict with an eleven-gate evidence envelope.

**Evidence.** Revision `61b0ab4` (the governed website work): 8 blocking gates passed (fmt 612 ms, check 7.7 s, clippy 0.85 s, workspace tests 26.4 s, strict deliver 21.3 s, replay, MVP replay, value-lab), advisory/conditional findings recorded. The adversarial fixture campaign (10 tests) proves rejection of wrong-SHA evidence, missing mandatory gates, failed/inconclusive blocking gates, and expired waivers. The specialist adapters found two day-one realities: this Traci build rejects the directive's documented flags, and Amber cannot analyse a workspace root.

**Explanation.** The federation design kept every authority in its lane: adapters observe, the envelope records, the verdict derives, and admission verification independently re-checks revision identity, policy version and mandatory-gate presence.

**Evaluation.** The §35 demonstration (a deliberately defective PR rejected through the full chain) still requires branch protection (§18) and the CLAIMS/1 registry behind Verity — both pending. Until then the pipeline is proven in mechanics but not in enforcement.

---

## FINDING 5 — VALUE integrity and independent evaluation are now structural, not aspirational

**Point.** The subject cannot alter its own evaluation without generating integrity evidence.

**Evidence.** `value-lab` holds: hashed experimental snaps, decision snapshots frozen per material decision, OCAV/regret under pre-registered weights, Shadow Defence scoring with lab–subject agreement as a measured variable, threshold-frozen verdict classification, a seed-deterministic fault schedule, and stop conditions including `subject_controls_research_authority`. The new integrity gate blocks protocol mutations that are undeclared, escalates subject-authored changes without waivers, and downgrades a waiver exactly one level — never to clean.

**Explanation.** The separation of powers survived its first contact with real incentives: the live draftboard used the same measured-record priors that the roster uses, and nothing in the pipeline could rewrite the draftboard after seeing results.

**Evaluation.** The apparatus is complete; the 100–500-task live season remains an operations programme (Phase 2–7 of §37) that this machinery now makes possible rather than hypothetical.

---

## FINDING 6 — Presentation parity held everywhere it was tested

**Point.** Styled, machine and plain outputs carry the same facts on every surface.

**Evidence.** Every CLI surface now renders through the real 3form library (headline/verdict/stage-bar), with piped output at zero escape codes, `--basic` byte-pinned, and `--form3` grammar untouched. The Matchday surfaces (manager, broadcast, film room) project `PresentationSnapshot` pure functions with per-frame parity against the fold, and the website renders from the repository itself — version, crates, docs, recordings extracted, never hardcoded.

**Explanation.** One projection rule — the same state projects the same frames — enforced by `Eq` on snapshots and asserted per-frame in the film-room test.

**Evaluation.** The standing test is drift: any surface that shows a fact the snapshot cannot answer fails review.

---

## RECOMMENDATIONS (ranked by leverage)

1. **Close intervention classes 2–4:** automatic brief-repair, envelope sizing from `finish_reason`, decomposition from truncation data. The foreman already records every input these decisions need.
2. **A live `CapabilityPlane` for OpenRouter** so drives dispatch real model work natively — turning the foreman pattern from a harness into a platform feature.
3. **Branch protection + CLAIMS/1** to convert the CI federation from proven mechanics into enforced governance (§18, §12).
4. **Run the VALUE primary season** on the now-complete apparatus; the draftboard should re-run first — Finding 2 says the roster it freezes is perishable.
5. **Fix the kaptaind scheduler stall** (post-cluster-expiry hang, restart-resistant, observed twice) — it forced two manual commits this session.

---

## CLOSING STATEMENT

Across every axis tested this session, the architecture's central promise held: **no claim of correctness, quality, completion or readiness was accepted without independent, mechanically validated evidence** — including claims made by the platform's own operators. What the session exposed is not a weakness in that promise but the price of keeping it: every refusal is real, and today a human still pays for the diagnosis. The apparatus now exists to make the platform pay that price itself.
