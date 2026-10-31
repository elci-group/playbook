//! The Offensive Kicker: field-goal-range economics and the bounded
//! terminal action.
//!
//! Field-goal range is semantic and economic — never a fixed yardage. A
//! drive is in range when there exists one admissible frontier action whose
//! expected utility exceeds continued progression:
//!
//! ```text
//! U_kick  = P(score) × V(goal) − C(kick) − R(kick)
//! U_drive = P(progress) × V(goal) − C(drive) − R(drive)
//! ```
//!
//! admitted when `U_kick > U_drive` under the policy envelope. The kick is
//! a SINGLE bounded frontier action under an explicit contract: it cannot
//! expand scope, spawn a multi-step plan, redefine the objective, waive
//! validation, or reinterpret failure as partial success (I-15). A kick
//! cannot establish success without independent evidence (I-14). Kicker
//! personnel carry distinct priors from ordinary players: the best
//! progressive agent is not automatically the best terminal-action agent.

use serde::{Deserialize, Serialize};

use playbook_core::{EvidenceClass, PersonnelId, PlaybookError};

use crate::domain::{field_refusal, KickOutcome, KickUtility};

/// The bounded contract for one kick attempt (§18). Every ceiling is fixed
/// at admission and the attempt cannot exceed any of them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickContract {
    pub objective: String,
    /// The validated frontier the kick reasons from — never the raw
    /// provisional position.
    pub validated_frontier_yard: u32,
    pub remaining_mandatory_gates: Vec<String>,
    /// The one admitted action description.
    pub permitted_action: String,
    pub max_cost_tokens: u64,
    pub max_latency_ms: u64,
    /// The widest side-effect scope the attempt may touch.
    pub max_side_effect_scope: String,
    pub success_evidence: Vec<EvidenceClass>,
    pub rollback_policy: String,
}

impl KickContract {
    /// The contract cannot silently expand: every ceiling and authority
    /// field is mandatory.
    pub fn validate(&self) -> Result<(), PlaybookError> {
        if self.objective.trim().is_empty() {
            return Err(field_refusal("kick_requires_objective"));
        }
        if self.permitted_action.trim().is_empty() {
            return Err(field_refusal("kick_requires_action"));
        }
        if self.success_evidence.is_empty() {
            return Err(field_refusal("kick_requires_success_evidence"));
        }
        if self.max_cost_tokens == 0 {
            return Err(field_refusal("kick_requires_cost_ceiling"));
        }
        if self.max_latency_ms == 0 {
            return Err(field_refusal("kick_requires_latency_ceiling"));
        }
        if self.rollback_policy.trim().is_empty() {
            return Err(field_refusal("kick_requires_rollback_policy"));
        }
        Ok(())
    }

    /// Whether a candidate's measured cost and latency fit the contract.
    pub fn accommodates(&self, cost_tokens: u64, latency_ms: u64) -> bool {
        cost_tokens <= self.max_cost_tokens && latency_ms <= self.max_latency_ms
    }
}

/// The distinct personnel prior for one kicker candidate (§22). Terminal
/// history is separate from progressive-yard attribution: one-shot
/// completion probability, variance, rollback risk, and scope discipline
/// decide, not raw yardage ability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickerPrior {
    pub personnel: PersonnelId,
    pub one_shot_completion_probability_ppm: u32,
    pub variance_ppm: u32,
    pub cost_tokens: u64,
    pub latency_ms: u64,
    pub rollback_risk_ppm: u32,
    pub evidence_quality_ppm: u32,
    pub terminal_task_history: u32,
}

/// The deterministic kicker score (§22) in 0..=1_000_000 ppm. Weighted
/// u64 math: completion probability weighted ×4, evidence quality ×2, low
/// variance and low rollback risk each +1 weight, over 8 total weights.
/// Cost and latency filter admission (the contract's ceilings) rather than
/// entering the score.
pub fn kicker_score(prior: &KickerPrior) -> u32 {
    let probability = u64::from(prior.one_shot_completion_probability_ppm.min(1_000_000));
    let evidence = u64::from(prior.evidence_quality_ppm.min(1_000_000));
    let low_variance = 1_000_000u64.saturating_sub(u64::from(prior.variance_ppm.min(1_000_000)));
    let low_rollback =
        1_000_000u64.saturating_sub(u64::from(prior.rollback_risk_ppm.min(1_000_000)));
    let weighted = probability * 4 + evidence * 2 + low_variance + low_rollback;
    u32::try_from(weighted / 8).unwrap_or(0)
}

/// One candidate kick assessed against continued progression (§19).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldGoalCandidate {
    pub personnel: PersonnelId,
    pub action: String,
    pub prior: KickerPrior,
    /// Measured completion probability for THIS action, 0..=1_000_000 —
    /// never the candidate's self-declared confidence.
    pub score_probability_ppm: u32,
    pub expected_cost_tokens: u64,
    pub expected_latency_ms: u64,
    /// Expected downside in signed utility units.
    pub expected_downside: i64,
    pub continued_drive_cost_tokens: u64,
    pub continued_drive_risk: i64,
    /// V(goal): the value of crossing the objective threshold.
    pub goal_value: i64,
    /// Whether success can be independently verified if the kick scores.
    pub can_independently_verify: bool,
}

/// The §19 auditable range decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickAdmission {
    pub candidate_action: String,
    pub candidate_personnel: PersonnelId,
    pub score_probability_ppm: u32,
    pub expected_cost_tokens: u64,
    pub expected_latency_ms: u64,
    pub expected_downside: i64,
    pub continued_drive_cost_tokens: u64,
    pub continued_drive_risk: i64,
    pub utility: KickUtility,
    pub admitted: bool,
    pub reasons: Vec<String>,
}

/// Assess the candidate against the contract and the economics. The
/// utilities are computed from measured inputs:
///
/// ```text
/// U_kick  = P×V(goal) − C_kick − R_kick     (ppm-scaled probability)
/// U_drive = the counterfactual continued-drive estimate
/// ```
///
/// Admission requires ALL of: independent verification possible, both
/// contract ceilings accommodated, downside within policy, and strict
/// utility dominance. Every failed condition is recorded in `reasons` —
/// the decision is auditable even when refused.
pub fn assess_field_goal(candidate: &FieldGoalCandidate, contract: &KickContract) -> KickAdmission {
    let mut reasons = Vec::new();
    if !candidate.can_independently_verify {
        reasons.push("independent_verification_required".into());
    }
    if candidate.expected_cost_tokens > contract.max_cost_tokens {
        reasons.push("cost_exceeds_contract".into());
    }
    if candidate.expected_latency_ms > contract.max_latency_ms {
        reasons.push("latency_exceeds_contract".into());
    }
    if candidate.expected_downside < -1_000_000 {
        reasons.push("downside_exceeds_policy".into());
    }
    let probability = i64::from(candidate.score_probability_ppm.min(1_000_000));
    let kick_utility = probability * candidate.goal_value / 1_000_000 - candidate.expected_downside;
    let drive_utility = -candidate.continued_drive_risk;
    let utility = KickUtility {
        kick_utility,
        drive_utility,
    };
    if !utility.in_range() {
        reasons.push("kick_does_not_dominate_progression".into());
    }
    KickAdmission {
        candidate_action: candidate.action.clone(),
        candidate_personnel: candidate.personnel.clone(),
        score_probability_ppm: candidate.score_probability_ppm,
        expected_cost_tokens: candidate.expected_cost_tokens,
        expected_latency_ms: candidate.expected_latency_ms,
        expected_downside: candidate.expected_downside,
        continued_drive_cost_tokens: candidate.continued_drive_cost_tokens,
        continued_drive_risk: candidate.continued_drive_risk,
        utility,
        admitted: reasons.is_empty(),
        reasons,
    }
}

/// §21 — classify one attempt from its outcome triple. Precedence: harm
/// dominates (a harmful attempt is a turnover regardless of evidence);
/// executed without evidence is a clean miss; external blocks are blocked;
/// a scored attempt requires evidence.
pub fn classify_outcome(
    executed: bool,
    external_block: bool,
    evidence_present: bool,
    harmful: bool,
) -> KickOutcome {
    if harmful {
        return KickOutcome::Turnover;
    }
    if external_block {
        return KickOutcome::Blocked;
    }
    if !executed {
        return KickOutcome::CleanMiss;
    }
    if evidence_present {
        KickOutcome::Score
    } else {
        KickOutcome::CleanMiss
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> KickContract {
        KickContract {
            objective: "cross the objective threshold in one action".into(),
            validated_frontier_yard: 70,
            remaining_mandatory_gates: vec!["release_signoff".into()],
            permitted_action: "one decisive benchmark run against the release candidate".into(),
            max_cost_tokens: 20_000,
            max_latency_ms: 10_000,
            max_side_effect_scope: "read_only".into(),
            success_evidence: vec![EvidenceClass::Benchmark],
            rollback_policy: "restore validated frontier digest on failure".into(),
        }
    }

    fn candidate() -> FieldGoalCandidate {
        FieldGoalCandidate {
            personnel: PersonnelId::new("kicker-a").unwrap(),
            action: "final benchmark".into(),
            prior: KickerPrior {
                personnel: PersonnelId::new("kicker-a").unwrap(),
                one_shot_completion_probability_ppm: 900_000,
                variance_ppm: 100_000,
                cost_tokens: 10_000,
                latency_ms: 5_000,
                rollback_risk_ppm: 50_000,
                evidence_quality_ppm: 900_000,
                terminal_task_history: 7,
            },
            score_probability_ppm: 850_000,
            expected_cost_tokens: 15_000,
            expected_latency_ms: 8_000,
            expected_downside: -500,
            continued_drive_cost_tokens: 40_000,
            continued_drive_risk: -1_200,
            goal_value: 10_000,
            can_independently_verify: true,
        }
    }

    #[test]
    fn the_contract_validates_every_ceiling() {
        let good = contract();
        good.validate().unwrap();
        for (reason, break_field) in [
            ("kick_requires_objective", "objective"),
            ("kick_requires_action", "permitted_action"),
            ("kick_requires_rollback_policy", "rollback_policy"),
        ] {
            let mut broken = good.clone();
            match break_field {
                "objective" => broken.objective = "  ".into(),
                "permitted_action" => broken.permitted_action = String::new(),
                _ => broken.rollback_policy = String::new(),
            }
            assert!(
                broken.validate().unwrap_err().to_string().contains(reason),
                "{reason}"
            );
        }
        let mut costless = good.clone();
        costless.max_cost_tokens = 0;
        assert!(costless
            .validate()
            .unwrap_err()
            .to_string()
            .contains("kick_requires_cost_ceiling"));
        let mut timeless = good;
        timeless.max_latency_ms = 0;
        assert!(timeless
            .validate()
            .unwrap_err()
            .to_string()
            .contains("kick_requires_latency_ceiling"));
    }

    #[test]
    fn kicker_score_orders_terminal_specialists() {
        let steady = KickerPrior {
            personnel: PersonnelId::new("steady").unwrap(),
            one_shot_completion_probability_ppm: 800_000,
            variance_ppm: 50_000,
            cost_tokens: 4_000,
            latency_ms: 2_000,
            rollback_risk_ppm: 30_000,
            evidence_quality_ppm: 900_000,
            terminal_task_history: 12,
        };
        let volatile = KickerPrior {
            personnel: PersonnelId::new("volatile").unwrap(),
            one_shot_completion_probability_ppm: 950_000,
            variance_ppm: 800_000,
            cost_tokens: 4_000,
            latency_ms: 2_000,
            rollback_risk_ppm: 600_000,
            evidence_quality_ppm: 900_000,
            terminal_task_history: 12,
        };
        assert!(
            kicker_score(&steady) > kicker_score(&volatile),
            "the steady kicker outranks the volatile one at terminal work"
        );
    }

    #[test]
    fn field_goal_range_is_utility_dominance_not_yardage() {
        let assessment = assess_field_goal(&candidate(), &contract());
        assert!(assessment.admitted);
        assert!(assessment.utility.kick_utility > assessment.utility.drive_utility);
        assert!(assessment.reasons.is_empty());
        // The same candidate with tripled cost is no longer in range.
        let mut expensive = candidate();
        expensive.expected_cost_tokens = 60_000;
        let refused = assess_field_goal(&expensive, &contract());
        assert!(!refused.admitted);
        assert!(refused
            .reasons
            .iter()
            .any(|reason| reason == "cost_exceeds_contract"));
    }

    #[test]
    fn a_kick_without_independent_verification_is_refused() {
        let mut candidate = candidate();
        candidate.can_independently_verify = false;
        let assessment = assess_field_goal(&candidate, &contract());
        assert!(!assessment.admitted);
        assert!(assessment
            .reasons
            .iter()
            .any(|reason| reason == "independent_verification_required"));
    }

    #[test]
    fn outcome_classification_precedence_holds() {
        // Harm dominates everything.
        assert_eq!(
            classify_outcome(true, false, true, true),
            KickOutcome::Turnover
        );
        // External block before execution state.
        assert_eq!(
            classify_outcome(true, true, true, false),
            KickOutcome::Blocked
        );
        // Executed with evidence scores.
        assert_eq!(
            classify_outcome(true, false, true, false),
            KickOutcome::Score
        );
        // Executed without evidence is a clean miss.
        assert_eq!(
            classify_outcome(true, false, false, false),
            KickOutcome::CleanMiss
        );
        // Never executed, nothing harmful: clean miss.
        assert_eq!(
            classify_outcome(false, false, false, false),
            KickOutcome::CleanMiss
        );
        assert!(KickOutcome::Score.scored());
        assert!(!KickOutcome::Blocked.scored());
        assert!(KickOutcome::CleanMiss.possession_recoverable());
        assert!(!KickOutcome::Turnover.possession_recoverable());
    }
}
