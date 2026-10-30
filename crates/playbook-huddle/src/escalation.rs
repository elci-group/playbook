//! Scope escalation.
//!
//! Escalation is deterministic and budgeted. Without scope justification
//! there is no escalation.

use playbook_core::PlaybookError;

use crate::domain::{ActiveHuddle, ContradictionSeverity, HuddleScope};

/// The scope a huddle should escalate to, if any. Justification is an
/// unresolved contradiction load or coordinator-band debt; Franchise scope
/// additionally requires manual authority, which the caller expresses by
/// passing `manual`.
pub #[tracing::instrument]
fn escalation_target(
    huddle: &ActiveHuddle,
    debt: u32,
    manual: bool,
    policy: &playbook_policy::HuddlePolicy,
) -> Result<Option<HuddleScope>, PlaybookError> {
    let blocking = huddle
        .contradictions
        .iter()
        .any(|contradiction| contradiction.severity >= ContradictionSeverity::High);
    let justified = blocking || debt >= policy.debt_coordinator;
    if !justified {
        return Ok(None);
    }
    let target = huddle.scope.next_up().ok_or_else(|| {
        PlaybookError::Inadmissible("huddle".into(), "already at franchise scope".into())
    })?;
    if target == HuddleScope::Franchise && !manual {
        return Err(PlaybookError::Inadmissible(
            "huddle".into(),
            "franchise escalation requires manual authority".into(),
        ));
    }
    Ok(Some(target))
}

/// A scope is a valid escalation target only upward.
pub #[tracing::instrument]
fn is_upward(current: HuddleScope, target: HuddleScope) -> bool {
    target.rank() > current.rank()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Contradiction, ContradictionId, HuddlePhase, HuddleTrigger, HuddleTriggerKind,
    };
    use playbook_core::{BarrierMode, EventId, HuddleBudget, HuddleId};
    use playbook_policy::Policy;

    #[tracing::instrument]
fn huddle(severity: ContradictionSeverity) -> ActiveHuddle {
        ActiveHuddle {
            id: HuddleId::new("h-1").unwrap(),
            scope: HuddleScope::Unit,
            barrier: BarrierMode::Scope,
            phase: HuddlePhase::CollectReports,
            trigger: HuddleTrigger {
                kind: HuddleTriggerKind::CoherenceDebt,
                reason_code: "coherence_debt".into(),
                evidence_refs: Vec::new(),
                originating_event: EventId::new("evt-1").unwrap(),
            },
            budget: HuddleBudget::zero(),
            spent: crate::domain::HuddleCost::zero(),
            participants: Vec::new(),
            reports: Vec::new(),
            contradictions: vec![Contradiction {
                id: ContradictionId::new("con-000").unwrap(),
                subject: "interface".into(),
                claims: Vec::new(),
                severity,
            }],
            exhausted: false,
            escalation: None,
            partition: Vec::new(),
            field_fingerprint: "f".into(),
            at_ms: 1,
            debt_before: 0,
            ytg_milli_before: 0,
            rework_avoided: 0,
        }
    }

    #[test]
    #[tracing::instrument]
fn escalation_needs_justification_and_stays_upward() {
        let policy = Policy::builtin().huddle;
        assert!(
            escalation_target(&huddle(ContradictionSeverity::Low), 0, false, &policy)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            escalation_target(&huddle(ContradictionSeverity::High), 0, false, &policy).unwrap(),
            Some(HuddleScope::Coordinator)
        );
        assert_eq!(
            escalation_target(
                &huddle(ContradictionSeverity::Low),
                policy.debt_coordinator,
                false,
                &policy
            )
            .unwrap(),
            Some(HuddleScope::Coordinator)
        );
        assert!(is_upward(HuddleScope::Unit, HuddleScope::Coordinator));
        assert!(!is_upward(HuddleScope::Unit, HuddleScope::Position));
    }

    #[test]
    #[tracing::instrument]
fn franchise_escalation_requires_manual_authority() {
        let policy = Policy::builtin().huddle;
        let mut team = huddle(ContradictionSeverity::High);
        team.scope = HuddleScope::Team;
        assert!(escalation_target(&team, 0, false, &policy).is_err());
        assert_eq!(
            escalation_target(&team, 0, true, &policy).unwrap(),
            Some(HuddleScope::Franchise)
        );
    }
}
