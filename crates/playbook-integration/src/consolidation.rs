//! The Hybreed/Emulsify/Kaptaind consolidation seam: reconciling divergent
//! valid implementations of the same objective into one materialised
//! result, transactionally.
//!
//! Football: late in the week, two position coaches have independently
//! drawn up valid but divergent answers to the same opponent look, and the
//! baseline script (Prime) already sits in the binder. Hybreed is the
//! coordinator's semantic reconciliation: it reads both game plans against
//! the baseline, retains the traits worth keeping from each side,
//! transforms where the two languages disagree, and writes the
//! contradictions down instead of hiding them — and it never rewrites the
//! baseline script it reconciled against. Emulsify is the structural
//! compatibility plan: given reconciled semantics, it classifies whether
//! the parts can actually line up together — mix outright, blend through
//! adapters, suspend components until later, or keep them separate — and
//! names the merge units, but it never performs an authoritative merge.
//! Kaptaind is the captain on the field and the only transaction
//! authority: it re-checks the package and the plan, verifies the
//! personnel provenance, freezes the candidate SHAs and the plan digest,
//! applies the plan, and rolls the whole transaction back whenever the
//! consolidated UNI score regresses against the pre-merge baseline. No
//! consolidation output reaches the tree except through the Kaptaind gate.
//!
//! The whole seam is deterministic and integer-only: confidence is
//! expressed in parts-per-million against [`playbook_core::PPM_SCALE`],
//! plan digests are sha256 over the canonical serialisation, and the same
//! inputs always produce the same digest and the same verdict.

use serde::{Deserialize, Serialize};

use playbook_core::{sha256_hex, to_canonical_bytes, PlaybookError, PPM_SCALE};

/// Hard limits a consolidation package imposes on the seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsolidationConstraints {
    /// The most merge units Emulsify may plan for one transaction.
    pub max_merge_units: u16,
    /// When set, the package must carry non-empty acceptance gates and
    /// evidence refs or it is refused.
    pub require_evidence_refs: bool,
}

/// The §9 consolidation package: one objective, the Prime artefact it
/// reconciles against, the divergent candidate artefacts, and the gates,
/// evidence, and personnel the transaction must answer for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsolidationCandidate {
    pub objective_id: String,
    pub drive_id: String,
    /// The Prime artefact digest — read-only for Hybreed.
    pub baseline_ref: String,
    /// Divergent artefact digests. At least two, or there is nothing to
    /// consolidate.
    pub candidate_refs: Vec<String>,
    /// Gate ids the consolidated result must satisfy.
    pub acceptance_gates: Vec<String>,
    pub evidence_refs: Vec<String>,
    /// Personnel contributions behind the candidates.
    pub provenance: Vec<String>,
    pub constraints: ConsolidationConstraints,
}

impl ConsolidationCandidate {
    /// §8: consolidation requires ACTUAL divergence — at least two
    /// candidate refs and a baseline to reconcile against. When
    /// `constraints.require_evidence_refs` is set, empty acceptance gates
    /// and empty evidence refs are refused as well. The refusal reason is
    /// carried as the machine-readable code of
    /// [`PlaybookError::Inadmissible`].
    pub fn validate(&self) -> Result<(), PlaybookError> {
        if self.candidate_refs.len() < 2 {
            return Err(PlaybookError::Inadmissible(
                "consolidation".into(),
                "insufficient_divergence".into(),
            ));
        }
        if self.baseline_ref.is_empty() {
            return Err(PlaybookError::Inadmissible(
                "consolidation".into(),
                "missing_baseline".into(),
            ));
        }
        if self.constraints.require_evidence_refs && self.acceptance_gates.is_empty() {
            return Err(PlaybookError::Inadmissible(
                "consolidation".into(),
                "acceptance_gates_required".into(),
            ));
        }
        if self.constraints.require_evidence_refs && self.evidence_refs.is_empty() {
            return Err(PlaybookError::Inadmissible(
                "consolidation".into(),
                "evidence_refs_required".into(),
            ));
        }
        Ok(())
    }
}

/// A property that survives reconciliation from one candidate: the trait
/// name plus the candidate ref it traces back to. Provenance is part of
/// the trait, not an afterthought.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trait {
    pub name: String,
    pub source_ref: String,
}

/// A trait that no candidate supplies as-is: `from_trait` names a retained
/// trait, and `transform` is the machine-readable reconciliation step that
/// turns it into `to_trait`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transformation {
    pub from_trait: String,
    pub to_trait: String,
    pub transform: String,
}

/// The §10 semantic reconciliation Hybreed proposes for one candidate
/// package. Hybreed never modifies Prime: everything here is a proposal
/// that still has to pass [`EmulsifyPlan::validate`] and the Kaptaind
/// gate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HybreedPlan {
    pub retained_traits: Vec<Trait>,
    pub rejected_traits: Vec<Trait>,
    pub transformed_traits: Vec<Transformation>,
    /// Contradictions between candidates that reconciliation could not
    /// dissolve. Recorded, never silently dropped.
    pub contradictions: Vec<String>,
    /// Reconciliation confidence in parts per million, 0..=[PPM_SCALE].
    pub confidence_ppm: u32,
    /// The source refs the reconciliation traced.
    pub provenance: Vec<String>,
}

impl HybreedPlan {
    /// Every retained trait must trace to a candidate_ref in the candidate
    /// package, and every transformation must start from a retained trait
    /// — provenance is not optional, and the plan is refused otherwise.
    /// Confidence outside 0..=[PPM_SCALE] is refused too. Checks run in a
    /// fixed order: confidence bound, retained-trait traceability,
    /// transformation chains.
    pub fn validate_against(
        &self,
        candidate: &ConsolidationCandidate,
    ) -> Result<(), PlaybookError> {
        if self.confidence_ppm > PPM_SCALE {
            return Err(PlaybookError::Probability(self.confidence_ppm));
        }
        for retained in &self.retained_traits {
            if !candidate
                .candidate_refs
                .iter()
                .any(|r| r == &retained.source_ref)
            {
                return Err(PlaybookError::Inadmissible(
                    "hybreed".into(),
                    format!("untraceable_trait_{}", retained.name),
                ));
            }
        }
        for transformed in &self.transformed_traits {
            let sourced = self
                .retained_traits
                .iter()
                .any(|retained| retained.name == transformed.from_trait);
            if !sourced {
                return Err(PlaybookError::Inadmissible(
                    "hybreed".into(),
                    format!(
                        "transformation_without_retained_source_{}",
                        transformed.from_trait
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// §11 structural compatibility of the reconciled parts. `Mix` lines up
/// outright; `Emulsify` needs adapters to blend; `Suspend` holds
/// components out until later; `Separate` refuses to combine at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityClass {
    Mix,
    Emulsify,
    Suspend,
    Separate,
}

/// One materialisation batch: the named unit and the candidate refs that
/// feed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeUnit {
    pub unit: String,
    pub sources: Vec<String>,
}

/// The §11 materialisation plan. Emulsify classifies and plans — it never
/// merges authoritatively; applying this plan is Kaptaind's transaction
/// alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmulsifyPlan {
    pub compatibility_class: CompatibilityClass,
    pub merge_units: Vec<MergeUnit>,
    pub adapters: Vec<String>,
    pub suspended_components: Vec<String>,
    pub rejected_components: Vec<String>,
    pub expected_tree_digest: String,
}

impl EmulsifyPlan {
    /// Suspend+Separate classes carry no merge units; Mix/Emulsify require
    /// at least one. Refuses the mismatch with the reason codes
    /// `suspend_separate_carry_no_merge_units` and
    /// `mix_emulsify_require_merge_units`.
    pub fn validate(&self) -> Result<(), PlaybookError> {
        if matches!(
            self.compatibility_class,
            CompatibilityClass::Suspend | CompatibilityClass::Separate
        ) && !self.merge_units.is_empty()
        {
            return Err(PlaybookError::Inadmissible(
                "emulsify".into(),
                "suspend_separate_carry_no_merge_units".into(),
            ));
        }
        if matches!(
            self.compatibility_class,
            CompatibilityClass::Mix | CompatibilityClass::Emulsify
        ) && self.merge_units.is_empty()
        {
            return Err(PlaybookError::Inadmissible(
                "emulsify".into(),
                "mix_emulsify_require_merge_units".into(),
            ));
        }
        Ok(())
    }
}

/// The frozen transaction record Kaptaind writes when a consolidation is
/// applied: the candidate SHAs it accepted, the digest of the plan it
/// applied, the UNI score on both sides of the transaction, and whether
/// the personnel provenance was verified.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KaptaindGate {
    pub candidate_shas: Vec<String>,
    /// sha256 of the canonical emulsify plan bytes.
    pub plan_digest: String,
    pub uni_before: u32,
    pub uni_after: u32,
    pub provenance_verified: bool,
}

/// The Kaptaind verdict. `Rejected` always carries a machine-readable
/// reason code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateVerdict {
    Accepted,
    Rejected { reason_code: String },
}

/// The §12 transactional gate. Deterministic, and the only path a
/// consolidation may take into the tree. Checks run in a fixed order:
/// candidate package validity, emulsify plan validity, provenance,
/// candidate SHAs, UNI regression, acceptance-gate re-run. ACCEPT only
/// when all hold: the candidate and plan validate; provenance is
/// verified; every candidate sha is present and non-empty;
/// `uni_after >= uni_before` (a regression rolls the transaction back with
/// reason `uni_regression_{before}_{after}`); and the candidate's
/// acceptance gates were all re-run (`gates_rerun`).
pub fn kaptaind_gate(
    candidate: &ConsolidationCandidate,
    plan: &EmulsifyPlan,
    uni_before: u32,
    uni_after: u32,
    provenance_verified: bool,
    gates_rerun: bool,
) -> GateVerdict {
    let reject = |reason_code: String| GateVerdict::Rejected { reason_code };
    if let Err(PlaybookError::Inadmissible(_, code)) = candidate.validate() {
        return reject(code);
    }
    if let Err(PlaybookError::Inadmissible(_, code)) = plan.validate() {
        return reject(code);
    }
    if !provenance_verified {
        return reject("provenance_unverified".into());
    }
    if candidate.candidate_refs.iter().any(|sha| sha.is_empty()) {
        return reject("candidate_sha_missing".into());
    }
    if uni_after < uni_before {
        return reject(format!("uni_regression_{uni_before}_{uni_after}"));
    }
    if !gates_rerun {
        return reject("acceptance_gates_not_rerun".into());
    }
    GateVerdict::Accepted
}

/// The plan digest Kaptaind freezes: sha256 over the canonical
/// serialisation of the emulsify plan
/// ([`playbook_core::to_canonical_bytes`] + [`playbook_core::sha256_hex`]).
/// Deterministic: the same plan always digests the same way, regardless of
/// field order.
pub fn plan_digest(plan: &EmulsifyPlan) -> Result<String, PlaybookError> {
    Ok(sha256_hex(&to_canonical_bytes(plan)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> ConsolidationCandidate {
        ConsolidationCandidate {
            objective_id: "objective-wide-zone".into(),
            drive_id: "drive-2026-w09".into(),
            baseline_ref: "sha-prime-base".into(),
            candidate_refs: vec!["sha-candidate-a".into(), "sha-candidate-b".into()],
            acceptance_gates: vec!["gate-two-minute".into()],
            evidence_refs: vec!["evidence-ref-a".into(), "evidence-ref-b".into()],
            provenance: vec!["personnel-coach-a".into(), "personnel-coach-b".into()],
            constraints: ConsolidationConstraints {
                max_merge_units: 4,
                require_evidence_refs: true,
            },
        }
    }

    fn hybreed_plan() -> HybreedPlan {
        HybreedPlan {
            retained_traits: vec![
                Trait {
                    name: "gap-integrity".into(),
                    source_ref: "sha-candidate-a".into(),
                },
                Trait {
                    name: "perimeter-fold".into(),
                    source_ref: "sha-candidate-b".into(),
                },
            ],
            rejected_traits: vec![Trait {
                name: "legacy-man-coverage".into(),
                source_ref: "sha-candidate-b".into(),
            }],
            transformed_traits: vec![Transformation {
                from_trait: "gap-integrity".into(),
                to_trait: "wide-zone-fits".into(),
                transform: "re-key run fits to wide zones".into(),
            }],
            contradictions: vec!["gap-integrity vs perimeter-fold in compressed splits".into()],
            confidence_ppm: 900_000,
            provenance: vec!["sha-candidate-a".into(), "sha-candidate-b".into()],
        }
    }

    fn mix_plan() -> EmulsifyPlan {
        EmulsifyPlan {
            compatibility_class: CompatibilityClass::Mix,
            merge_units: vec![MergeUnit {
                unit: "run-game".into(),
                sources: vec!["sha-candidate-a".into(), "sha-candidate-b".into()],
            }],
            adapters: vec!["adapter-gap-scheme".into()],
            suspended_components: vec![],
            rejected_components: vec!["legacy-man-coverage".into()],
            expected_tree_digest: "sha-tree-expected".into(),
        }
    }

    fn refusal_code(verdict: &GateVerdict) -> &str {
        match verdict {
            GateVerdict::Accepted => panic!("expected a rejection, got Accepted"),
            GateVerdict::Rejected { reason_code } => reason_code,
        }
    }

    fn inadmissible_code(err: &PlaybookError) -> &str {
        match err {
            PlaybookError::Inadmissible(_, code) => code,
            other => panic!("expected an inadmissible refusal, got {other:?}"),
        }
    }

    #[test]
    fn candidate_validate_enforces_divergence_and_evidence() {
        assert!(candidate().validate().is_ok());
        let mut single = candidate();
        single.candidate_refs.truncate(1);
        assert_eq!(
            inadmissible_code(&single.validate().unwrap_err()),
            "insufficient_divergence"
        );
        let mut no_baseline = candidate();
        no_baseline.baseline_ref.clear();
        assert_eq!(
            inadmissible_code(&no_baseline.validate().unwrap_err()),
            "missing_baseline"
        );
        let mut no_gates = candidate();
        no_gates.acceptance_gates.clear();
        assert_eq!(
            inadmissible_code(&no_gates.validate().unwrap_err()),
            "acceptance_gates_required"
        );
        let mut no_evidence = candidate();
        no_evidence.evidence_refs.clear();
        assert_eq!(
            inadmissible_code(&no_evidence.validate().unwrap_err()),
            "evidence_refs_required"
        );
        // Without the evidence requirement the thin package is lawful.
        let mut relaxed = candidate();
        relaxed.constraints.require_evidence_refs = false;
        relaxed.acceptance_gates.clear();
        relaxed.evidence_refs.clear();
        assert!(relaxed.validate().is_ok());
    }

    #[test]
    fn hybreed_plan_refuses_traits_that_do_not_trace_to_candidates() {
        let candidate = candidate();
        assert!(hybreed_plan().validate_against(&candidate).is_ok());
        let mut foreign = hybreed_plan();
        foreign.retained_traits[0].source_ref = "sha-not-in-package".into();
        assert_eq!(
            inadmissible_code(&foreign.validate_against(&candidate).unwrap_err()),
            "untraceable_trait_gap-integrity"
        );
        let mut orphan = hybreed_plan();
        orphan.transformed_traits[0].from_trait = "never-retained".into();
        assert_eq!(
            inadmissible_code(&orphan.validate_against(&candidate).unwrap_err()),
            "transformation_without_retained_source_never-retained"
        );
        let mut overconfident = hybreed_plan();
        overconfident.confidence_ppm = PPM_SCALE + 1;
        assert_eq!(
            overconfident.validate_against(&candidate).unwrap_err(),
            PlaybookError::Probability(PPM_SCALE + 1)
        );
    }

    #[test]
    fn emulsify_plan_class_and_unit_contract_is_bidirectional() {
        let mut plan = mix_plan();
        assert!(plan.validate().is_ok());
        plan.compatibility_class = CompatibilityClass::Suspend;
        assert_eq!(
            inadmissible_code(&plan.validate().unwrap_err()),
            "suspend_separate_carry_no_merge_units"
        );
        plan.compatibility_class = CompatibilityClass::Separate;
        assert_eq!(
            inadmissible_code(&plan.validate().unwrap_err()),
            "suspend_separate_carry_no_merge_units"
        );
        let empty_units = |mut plan: EmulsifyPlan| {
            plan.merge_units.clear();
            plan
        };
        assert_eq!(
            inadmissible_code(&empty_units(mix_plan()).validate().unwrap_err()),
            "mix_emulsify_require_merge_units"
        );
        let mut emulsify = empty_units(mix_plan());
        emulsify.compatibility_class = CompatibilityClass::Emulsify;
        assert_eq!(
            inadmissible_code(&emulsify.validate().unwrap_err()),
            "mix_emulsify_require_merge_units"
        );
        let mut suspended = empty_units(mix_plan());
        suspended.compatibility_class = CompatibilityClass::Suspend;
        suspended.suspended_components = vec!["parallel-coverage-shell".into()];
        assert!(suspended.validate().is_ok());
    }

    #[test]
    fn plan_digest_is_deterministic_and_field_sensitive() {
        let plan = mix_plan();
        let first = plan_digest(&plan).unwrap();
        let second = plan_digest(&plan).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        let mut changed = plan.clone();
        changed.adapters.push("adapter-coverage-shell".into());
        assert_ne!(first, plan_digest(&changed).unwrap());
    }

    #[test]
    fn kaptaind_gate_accepts_a_clean_consolidation() {
        let candidate = candidate();
        let plan = mix_plan();
        assert_eq!(
            kaptaind_gate(&candidate, &plan, 4_200, 4_200, true, true),
            GateVerdict::Accepted,
            "equal UNI is not a regression"
        );
        assert_eq!(
            kaptaind_gate(&candidate, &plan, 4_200, 4_500, true, true),
            GateVerdict::Accepted
        );
        let frozen = KaptaindGate {
            candidate_shas: candidate.candidate_refs.clone(),
            plan_digest: plan_digest(&plan).unwrap(),
            uni_before: 4_200,
            uni_after: 4_500,
            provenance_verified: true,
        };
        let serialised = serde_json::to_string(&frozen).unwrap();
        assert_eq!(
            serde_json::from_str::<KaptaindGate>(&serialised).unwrap(),
            frozen
        );
    }

    #[test]
    fn kaptaind_gate_rolls_back_uni_regression_with_exact_reason_code() {
        let verdict = kaptaind_gate(&candidate(), &mix_plan(), 4_200, 4_199, true, true);
        assert_eq!(
            refusal_code(&verdict),
            "uni_regression_4200_4199",
            "the rollback reason freezes both UNI readings"
        );
    }

    #[test]
    fn kaptaind_gate_rejects_unverified_provenance() {
        let verdict = kaptaind_gate(&candidate(), &mix_plan(), 4_200, 4_500, false, true);
        assert_eq!(refusal_code(&verdict), "provenance_unverified");
    }

    #[test]
    fn kaptaind_gate_rejects_when_acceptance_gates_were_not_rerun() {
        let verdict = kaptaind_gate(&candidate(), &mix_plan(), 4_200, 4_500, true, false);
        assert_eq!(refusal_code(&verdict), "acceptance_gates_not_rerun");
    }

    #[test]
    fn kaptaind_gate_rejects_missing_candidate_shas() {
        let mut candidate = candidate();
        candidate.candidate_refs[1] = String::new();
        let verdict = kaptaind_gate(&candidate, &mix_plan(), 4_200, 4_500, true, true);
        assert_eq!(refusal_code(&verdict), "candidate_sha_missing");
    }

    #[test]
    fn gate_outcomes_are_deterministic() {
        let candidate = candidate();
        let plan = mix_plan();
        assert_eq!(
            kaptaind_gate(&candidate, &plan, 100, 90, true, true),
            kaptaind_gate(&candidate, &plan, 100, 90, true, true)
        );
        assert_eq!(
            kaptaind_gate(&candidate, &plan, 100, 110, true, true),
            kaptaind_gate(&candidate, &plan, 100, 110, true, true)
        );
        assert_eq!(
            GateVerdict::Rejected {
                reason_code: "provenance_unverified".into()
            },
            kaptaind_gate(&candidate, &plan, 100, 110, false, true)
        );
    }
}
