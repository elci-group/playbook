//! Defensive pressure and the deterministic trigger taxonomy.
//!
//! Blitz is not the default response to difficulty. Instinctive conditions
//! are evaluated first — predefined, policy-thresholded, most severe first —
//! and a strategic blitz is eligible only when measured urgency and measured
//! pressure both clear their policy floors. Solver uncertainty raises the
//! pressure score but never the urgency score: difficulty is not urgency.

use playbook_core::{EventId, EvidenceId};

use playbook_policy::{BlitzPolicy, BlitzPressureWeights};

use crate::domain::{BlitzClass, BlitzTrigger, Severity};

/// The measured defensive situation at trigger time. Every numeric component
/// is a deterministic integer: per-mille rates and confidences in 0..=1000
/// unless stated otherwise, so the same field readings always produce the
/// same scores and the same trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PressureInputs {
    pub severity: Severity,
    /// Milliseconds until the defensive objective is lost. `None` means the
    /// deadline is not the binding constraint and contributes nothing.
    pub deadline_ms: Option<u64>,
    pub elapsed_ms: u64,
    pub deterioration_rate_ppm: u32,
    pub blast_radius_ppm: u32,
    /// How little the machine knows about *which* solver can recover. This
    /// feeds the pressure score only; it never raises urgency.
    pub solver_uncertainty_ppm: u32,
    pub containment_confidence_ppm: u32,
    pub recovery_confidence_ppm: u32,
    pub resource_decay_ppm: u32,
    /// Utility lost per second of delay, already normalised to 0..=1000.
    pub expected_delay_loss: u32,
    // Instinctive condition flags. Each maps onto one predefined reason
    // code; none consults a model.
    pub critical_security_remediation: bool,
    pub affected_component_present: bool,
    pub verified_patch_available: bool,
    pub production_outage: bool,
    pub production_health_ppm: u32,
    pub rollback_failed: bool,
    pub connectivity_loss_imminent: bool,
    pub battery_remaining_ppm: u32,
    pub compute_declining: bool,
    pub quota_remaining_ppm: u32,
    pub critical_dependency_failed: bool,
    pub runtime_state_deteriorating: bool,
    pub state_preservation_window: bool,
    pub evidence_refs: Vec<EvidenceId>,
    pub originating_event: EventId,
}

/// How much of the deadline has already burned, in 0..=1000. `None` — no
/// binding deadline — contributes nothing; a lapsed or degenerate deadline
/// is fully consumed.
fn deadline_consumed(deadline_ms: Option<u64>, elapsed_ms: u64) -> u32 {
    match deadline_ms {
        None => 0,
        Some(0) => 1_000,
        Some(deadline_ms) => (1_000u64.saturating_mul(elapsed_ms) / deadline_ms).min(1_000) as u32,
    }
}

/// Deadline proximity against the policy horizon, in 0..=1000. `None`
/// contributes nothing; once `elapsed_ms >= deadline_ms` the score is 1000;
/// otherwise it is `1000 - 1000*(deadline_ms - elapsed_ms) /
/// deadline_horizon_ms`, clamped — urgency rises linearly across the final
/// policy horizon before the deadline and never before it.
fn deadline_proximity(deadline_ms: Option<u64>, elapsed_ms: u64, horizon_ms: u64) -> u32 {
    let Some(deadline_ms) = deadline_ms else {
        return 0;
    };
    if elapsed_ms >= deadline_ms {
        return 1_000;
    }
    let remaining = deadline_ms - elapsed_ms;
    let consumed = (1_000u64.saturating_mul(remaining) / horizon_ms.max(1)).min(1_000);
    (1_000 - consumed) as u32
}

/// The strategic difficulty of the defensive situation: a weighted sum of
/// clamped per-mille components, clamped to 1000. High solver uncertainty
/// makes the problem *harder* — it belongs here, and nowhere near urgency.
pub fn pressure_score(inputs: &PressureInputs, weights: &BlitzPressureWeights) -> u32 {
    let clamp = |value: u32| value.min(1_000);
    let raw = u64::from(inputs.severity.milli()) * u64::from(weights.severity)
        + u64::from(deadline_consumed(inputs.deadline_ms, inputs.elapsed_ms))
            * u64::from(weights.deadline_pressure)
        + u64::from(clamp(inputs.deterioration_rate_ppm)) * u64::from(weights.deterioration)
        + u64::from(clamp(inputs.blast_radius_ppm)) * u64::from(weights.blast_radius)
        + u64::from(clamp(inputs.solver_uncertainty_ppm)) * u64::from(weights.solver_uncertainty)
        + u64::from(clamp(inputs.resource_decay_ppm)) * u64::from(weights.resource_decay)
        + u64::from(clamp(inputs.expected_delay_loss)) * u64::from(weights.delay_loss);
    (raw / 1_000).min(1_000) as u32
}

/// The strategic urgency of the defensive situation: an equal-weight mean of
/// four clamped per-mille components — deadline proximity against the policy
/// horizon, deterioration rate, resource decay, and expected delay loss.
/// Solver uncertainty deliberately does not contribute: difficulty is not
/// urgency, and an unsolved-but-patient problem never buys a blitz.
pub fn urgency_score(inputs: &PressureInputs, policy: &BlitzPolicy) -> u32 {
    let deadline = deadline_proximity(
        inputs.deadline_ms,
        inputs.elapsed_ms,
        policy.deadline_horizon_ms,
    );
    let raw = u64::from(deadline)
        + u64::from(inputs.deterioration_rate_ppm.min(1_000))
        + u64::from(inputs.resource_decay_ppm.min(1_000))
        + u64::from(inputs.expected_delay_loss.min(1_000));
    ((raw / 4).min(1_000)) as u32
}

/// Deterministic eligibility evaluation, most severe first. Instinctive
/// conditions — predefined policy-threshold states that admit no debate —
/// fire before any strategic recommendation; the strategic class is reached
/// only when no instinctive condition holds, and then only above both policy
/// floors. Every trigger freezes the evidence bundle and the originating
/// event it was called on.
pub fn evaluate_eligibility(inputs: &PressureInputs, policy: &BlitzPolicy) -> Option<BlitzTrigger> {
    let trigger = |class, reason_code: &str| BlitzTrigger {
        class,
        reason_code: reason_code.to_string(),
        evidence_refs: inputs.evidence_refs.clone(),
        originating_event: inputs.originating_event.clone(),
    };
    if inputs.critical_security_remediation
        && inputs.affected_component_present
        && !inputs.verified_patch_available
    {
        return Some(trigger(BlitzClass::Instinctive, "critical_cve_unpatched"));
    }
    if inputs.production_outage && inputs.rollback_failed {
        return Some(trigger(BlitzClass::Instinctive, "outage_rollback_failed"));
    }
    if inputs.production_health_ppm < policy.production_health_floor_ppm && inputs.rollback_failed {
        return Some(trigger(
            BlitzClass::Instinctive,
            "health_floor_rollback_failed",
        ));
    }
    if inputs.battery_remaining_ppm < policy.battery_critical_ppm {
        return Some(trigger(BlitzClass::Instinctive, "power_critical"));
    }
    if inputs.quota_remaining_ppm < policy.quota_critical_ppm {
        return Some(trigger(
            BlitzClass::Instinctive,
            "quota_exhaustion_imminent",
        ));
    }
    if inputs.connectivity_loss_imminent {
        return Some(trigger(
            BlitzClass::Instinctive,
            "connectivity_loss_imminent",
        ));
    }
    if urgency_score(inputs, policy) >= policy.min_urgency_score
        && pressure_score(inputs, &policy.pressure_weights) >= policy.min_pressure_score
    {
        return Some(trigger(
            BlitzClass::Strategic,
            "strategic_defensive_pressure",
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_policy::Policy;

    fn base_inputs() -> PressureInputs {
        PressureInputs {
            severity: Severity::Low,
            deadline_ms: None,
            elapsed_ms: 0,
            deterioration_rate_ppm: 0,
            blast_radius_ppm: 0,
            solver_uncertainty_ppm: 0,
            containment_confidence_ppm: 1_000,
            recovery_confidence_ppm: 1_000,
            resource_decay_ppm: 0,
            expected_delay_loss: 0,
            critical_security_remediation: false,
            affected_component_present: false,
            verified_patch_available: false,
            production_outage: false,
            production_health_ppm: 1_000_000,
            rollback_failed: false,
            connectivity_loss_imminent: false,
            battery_remaining_ppm: 1_000_000,
            compute_declining: false,
            quota_remaining_ppm: 1_000_000,
            critical_dependency_failed: false,
            runtime_state_deteriorating: false,
            state_preservation_window: false,
            evidence_refs: Vec::new(),
            originating_event: EventId::new("evt-1").unwrap(),
        }
    }

    #[test]
    fn instinctive_conditions_fire_most_severe_first() {
        let policy = Policy::builtin().blitz;
        let mut inputs = base_inputs();
        assert_eq!(evaluate_eligibility(&inputs, &policy), None);

        // Every instinctive condition at once: the most severe reason wins.
        inputs.critical_security_remediation = true;
        inputs.affected_component_present = true;
        inputs.production_outage = true;
        inputs.rollback_failed = true;
        inputs.production_health_ppm = 0;
        inputs.battery_remaining_ppm = 0;
        inputs.quota_remaining_ppm = 0;
        inputs.connectivity_loss_imminent = true;
        let trigger = evaluate_eligibility(&inputs, &policy).unwrap();
        assert_eq!(trigger.class, BlitzClass::Instinctive);
        assert_eq!(trigger.reason_code, "critical_cve_unpatched");

        // Descend the ladder one rung at a time.
        inputs.verified_patch_available = true;
        assert_eq!(
            evaluate_eligibility(&inputs, &policy).unwrap().reason_code,
            "outage_rollback_failed"
        );

        inputs.production_outage = false;
        assert_eq!(
            evaluate_eligibility(&inputs, &policy).unwrap().reason_code,
            "health_floor_rollback_failed"
        );

        inputs.rollback_failed = false;
        assert_eq!(
            evaluate_eligibility(&inputs, &policy).unwrap().reason_code,
            "power_critical"
        );

        // The floors are strict: exactly at the threshold is not critical.
        inputs.battery_remaining_ppm = policy.battery_critical_ppm;
        assert_eq!(
            evaluate_eligibility(&inputs, &policy).unwrap().reason_code,
            "quota_exhaustion_imminent"
        );

        inputs.quota_remaining_ppm = policy.quota_critical_ppm;
        assert_eq!(
            evaluate_eligibility(&inputs, &policy).unwrap().reason_code,
            "connectivity_loss_imminent"
        );

        inputs.connectivity_loss_imminent = false;
        assert_eq!(evaluate_eligibility(&inputs, &policy), None);
    }

    #[test]
    fn difficulty_without_urgency_is_not_eligible() {
        let policy = Policy::builtin().blitz;
        let mut inputs = base_inputs();
        inputs.severity = Severity::Critical;
        inputs.blast_radius_ppm = 1_000;
        inputs.solver_uncertainty_ppm = 1_000;
        inputs.containment_confidence_ppm = 0;
        inputs.recovery_confidence_ppm = 0;
        assert!(pressure_score(&inputs, &policy.pressure_weights) >= policy.min_pressure_score);
        assert_eq!(urgency_score(&inputs, &policy), 0);
        assert_eq!(evaluate_eligibility(&inputs, &policy), None);

        // Solver uncertainty never moves urgency: difficulty is not urgency.
        let uncertain = PressureInputs {
            solver_uncertainty_ppm: 1_000,
            ..inputs.clone()
        };
        let calm = PressureInputs {
            solver_uncertainty_ppm: 0,
            ..inputs
        };
        assert_eq!(
            urgency_score(&uncertain, &policy),
            urgency_score(&calm, &policy)
        );
    }

    #[test]
    fn strategic_eligibility_requires_both_floors() {
        let policy = Policy::builtin().blitz;
        let mut inputs = base_inputs();
        // A deadline deep inside the policy horizon plus real delay loss:
        // urgency clears its floor, but low severity keeps pressure under
        // its own. No instinctive condition fires, so nothing is eligible.
        inputs.deadline_ms = Some(60_000);
        inputs.elapsed_ms = 59_000;
        inputs.expected_delay_loss = 400;
        assert!(urgency_score(&inputs, &policy) >= policy.min_urgency_score);
        assert!(pressure_score(&inputs, &policy.pressure_weights) < policy.min_pressure_score);
        assert_eq!(evaluate_eligibility(&inputs, &policy), None);

        // Raising the severity lifts pressure over its floor: now strategic.
        inputs.severity = Severity::High;
        assert!(pressure_score(&inputs, &policy.pressure_weights) >= policy.min_pressure_score);
        let trigger = evaluate_eligibility(&inputs, &policy).unwrap();
        assert_eq!(trigger.class, BlitzClass::Strategic);
        assert_eq!(trigger.reason_code, "strategic_defensive_pressure");
    }

    #[test]
    fn scores_are_deterministic_and_clamped() {
        let policy = Policy::builtin().blitz;
        let mut maxed = base_inputs();
        maxed.severity = Severity::Critical;
        maxed.deadline_ms = Some(1_000);
        maxed.elapsed_ms = 2_000;
        maxed.deterioration_rate_ppm = 5_000;
        maxed.blast_radius_ppm = 5_000;
        maxed.solver_uncertainty_ppm = 5_000;
        maxed.resource_decay_ppm = 5_000;
        maxed.expected_delay_loss = 5_000;
        assert_eq!(pressure_score(&maxed, &policy.pressure_weights), 1_000);
        assert_eq!(urgency_score(&maxed, &policy), 1_000);
        assert_eq!(
            pressure_score(&maxed, &policy.pressure_weights),
            pressure_score(&maxed, &policy.pressure_weights)
        );

        let zero = base_inputs();
        // Even a Low severity carries its per-mille floor: 100 * 300 / 1000.
        assert_eq!(pressure_score(&zero, &policy.pressure_weights), 30);
        assert_eq!(urgency_score(&zero, &policy), 0);
    }

    #[test]
    fn triggers_carry_their_evidence_bundle() {
        let policy = Policy::builtin().blitz;
        let mut inputs = base_inputs();
        inputs.evidence_refs = vec![
            EvidenceId::new("ev-1").unwrap(),
            EvidenceId::new("ev-2").unwrap(),
        ];
        inputs.battery_remaining_ppm = 0;
        let trigger = evaluate_eligibility(&inputs, &policy).unwrap();
        assert_eq!(trigger.evidence_refs, inputs.evidence_refs);
        assert_eq!(trigger.originating_event, inputs.originating_event);
        assert_eq!(trigger.class, BlitzClass::Instinctive);
    }
}
