//! Play admissibility and personnel routing.
//!
//! A candidate play is admissible only inside policy. Among admissible plays
//! that can remove yards, deterministic personnel are considered before any
//! specialist. Price alone does not win. Intelligence alone does not win.

#![forbid(unsafe_code)]

use playbook_core::{
    DecisionRecord, Down, DriveStatus, EvidenceId, Gate, Personnel, PersonnelId, PersonnelKind,
    PlayDefinition, PlayKind, PlaybookError, SwarmOrder, ToolFailureKind, PPM_SCALE,
    YTG_REFERENCE_MILLI,
};
use playbook_policy::Policy;
use playbook_security::authorize;
use playbook_ytg::gate_unresolved_milli;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldPosition {
    pub down: Down,
    pub preflight_complete: bool,
    pub field_surveyed: bool,
    pub status: DriveStatus,
    pub fingerprint: String,
    pub failure_fingerprint: Option<String>,
    pub last_failed_operation: Option<String>,
    pub last_failure_kind: Option<ToolFailureKind>,
    pub last_failed_personnel: Option<PersonnelId>,
    pub unresolved_effects: bool,
    pub gates: Vec<Gate>,
    pub personnel: Vec<Personnel>,
    pub evidence_refs: Vec<EvidenceId>,
    pub state_hash: String,
    pub policy_version: String,
    pub available_capabilities: Vec<String>,
    pub priors: HistoricalPriorTable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankedPersonnel {
    pub id: PersonnelId,
    pub kind: PersonnelKind,
    pub tier: u8,
    pub score_ppm: u32,
}

pub #[tracing::instrument]
fn admit(
    field: &FieldPosition,
    definition: &PlayDefinition,
    personnel_id: &PersonnelId,
    policy: &Policy,
) -> Result<(), PlaybookError> {
    if field.status != DriveStatus::Active {
        return Err(PlaybookError::Closed);
    }
    if field.unresolved_effects
        && definition.side_effect != playbook_core::SideEffectClass::ReadOnly
    {
        return Err(PlaybookError::Reconcile(definition.operation.clone()));
    }
    authorize(definition.side_effect, &policy.allowed_side_effects)?;
    if !field.preflight_complete && definition.kind != PlayKind::DiscoverCapabilities {
        return Err(PlaybookError::PreflightRequired);
    }
    if field.down == Down::FirstDown
        && !field.field_surveyed
        && !definition.survey
        && definition.kind != PlayKind::DiscoverCapabilities
    {
        return Err(PlaybookError::Inadmissible(
            definition.operation.clone(),
            "first down surveys the field before implementation".into(),
        ));
    }
    if field.last_failure_kind == Some(ToolFailureKind::Authorization)
        && field.last_failed_operation.as_deref() == Some(definition.operation.as_str())
    {
        return Err(PlaybookError::AuthorizationStop(
            definition.operation.clone(),
        ));
    }
    if field.last_failed_operation.as_deref() == Some(definition.operation.as_str())
        && field.failure_fingerprint.as_deref() == Some(field.fingerprint.as_str())
    {
        return Err(PlaybookError::BlindRetry(definition.operation.clone()));
    }
    if field.last_failure_kind == Some(ToolFailureKind::RateLimit)
        && field.last_failed_personnel.as_ref() == Some(personnel_id)
    {
        return Err(PlaybookError::Inadmissible(
            definition.operation.clone(),
            "rate limit requires different personnel".into(),
        ));
    }
    Ok(())
}

pub #[tracing::instrument]
fn rank(
    personnel: &[Personnel],
    policy: &Policy,
    priors: &HistoricalPriorTable,
) -> Vec<RankedPersonnel> {
    let mut ranked: Vec<RankedPersonnel> = personnel
        .iter()
        .map(|person| RankedPersonnel {
            id: person.id.clone(),
            kind: person.kind,
            tier: person.kind.tier(),
            score_ppm: score(person, policy, priors),
        })
        .collect();
    ranked.sort_by(|left, right| {
        left.tier
            .cmp(&right.tier)
            .then(right.score_ppm.cmp(&left.score_ppm))
            .then(left.id.as_str().cmp(right.id.as_str()))
    });
    ranked
}

pub #[tracing::instrument]
fn select(field: &FieldPosition, policy: &Policy) -> Result<DecisionRecord, PlaybookError> {
    let mut rejections = Vec::new();
    let mut candidates = Vec::new();
    for definition in &policy.plays {
        if !field
            .available_capabilities
            .iter()
            .any(|id| id == &definition.capability_id)
        {
            rejections.push(format!(
                "{} rejected: capability {} is not in the snapshot",
                definition.operation, definition.capability_id
            ));
            continue;
        }
        if definition.kind == PlayKind::DiscoverCapabilities && field.preflight_complete {
            rejections.push(format!(
                "{} rejected: preflight already recorded the capability snapshot",
                definition.operation
            ));
            continue;
        }
        let reduction = expected_reduction(definition, &field.gates);
        if reduction == 0 && !definition.survey {
            rejections.push(format!(
                "{} rejected: {}",
                definition.operation,
                target_gate_rejection(definition, &field.gates)
            ));
            continue;
        }
        let Some(person) = best_personnel(field, definition, policy, &mut rejections) else {
            continue;
        };
        let mut leverage = u64::from(reduction.max(1)) * u64::from(definition.reliability_ppm)
            / definition.marginal_cost.tokens.max(1);
        if field.down == Down::ThirdDownRush && definition.personnel_kind.deterministic() {
            let bonus = u64::from(PPM_SCALE + policy.rush_deterministic_bonus_ppm);
            leverage = leverage.saturating_mul(bonus) / u64::from(PPM_SCALE);
        }
        candidates.push(Candidate {
            definition,
            person,
            reduction,
            leverage,
        });
    }
    if let Some(best_tier) = candidates
        .iter()
        .filter(|candidate| candidate.reduction > 0)
        .map(|candidate| candidate.definition.personnel_kind.tier())
        .min()
    {
        candidates.retain(|candidate| {
            candidate.reduction == 0 || candidate.definition.personnel_kind.tier() == best_tier
        });
    }
    candidates.sort_by(|left, right| {
        right
            .leverage
            .cmp(&left.leverage)
            .then(left.definition.operation.cmp(&right.definition.operation))
            .then(left.person.id.as_str().cmp(right.person.id.as_str()))
    });
    let Some(chosen) = candidates.first() else {
        return Err(PlaybookError::NoPlay);
    };
    Ok(DecisionRecord {
        policy_version: policy.version.clone(),
        input_state_hash: field.state_hash.clone(),
        classification: field.down,
        selected_play: chosen.definition.kind,
        selected_operation: chosen.definition.operation.clone(),
        selected_personnel: Some(chosen.person.id.clone()),
        candidate_plays: candidates
            .iter()
            .map(|candidate| candidate.definition.operation.clone())
            .collect(),
        rejection_reasons: rejections,
        evidence_refs: field.evidence_refs.clone(),
    })
}

/// Phase 8. The table itself lives in `playbook_core` so an event log can
/// carry it across drives; routing re-exports it. It learns only from
/// measured outcomes: a play's realized reduction is the evidence-backed
/// yards-to-go drop recorded while it ran. Self-reported `own_ytg_milli`
/// never enters the table.
pub use playbook_core::{HistoricalPriorTable, PriorRow};

pub #[tracing::instrument]
fn admit_swarm(order: &SwarmOrder, policy: &Policy) -> Result<(), PlaybookError> {
    if order.expected_marginal_ytg_milli < policy.swarm_min_marginal_milli {
        return Err(PlaybookError::SwarmUnjustified);
    }
    if order.maximum_workers == 0 {
        return Err(PlaybookError::SwarmUnjustified);
    }
    authorize(
        playbook_core::SideEffectClass::ReadOnly,
        &policy.allowed_side_effects,
    )?;
    Ok(())
}

struct Candidate<'a> {
    definition: &'a PlayDefinition,
    person: &'a Personnel,
    reduction: u32,
    leverage: u64,
}

#[tracing::instrument]
fn best_personnel<'a>(
    field: &'a FieldPosition,
    definition: &PlayDefinition,
    policy: &Policy,
    rejections: &mut Vec<String>,
) -> Option<&'a Personnel> {
    let pool: Vec<Personnel> = field
        .personnel
        .iter()
        .filter(|person| person.kind == definition.personnel_kind)
        .cloned()
        .collect();
    if pool.is_empty() {
        rejections.push(format!(
            "{} rejected: no {} personnel",
            definition.operation,
            definition.kind.machine()
        ));
        return None;
    }
    for ranked in rank(&pool, policy, &field.priors) {
        let Some(person) = field.personnel.iter().find(|person| person.id == ranked.id) else {
            continue;
        };
        match admit(field, definition, &person.id, policy) {
            Ok(()) => return Some(person),
            Err(err) => rejections.push(format!("{} / {}: {err}", definition.operation, person.id)),
        }
    }
    None
}

trait PlayName {
    #[tracing::instrument]
fn machine(self) -> &'static str;
}

impl PlayName for PlayKind {
    #[tracing::instrument]
fn machine(self) -> &'static str {
        match self {
            Self::InspectEnvironment => "inspect",
            Self::DiscoverRepository => "discover_repository",
            Self::DecomposeTask => "decompose",
            Self::BaselineTest => "baseline",
            Self::DiscoverCapabilities => "discover",
            Self::RunDeterministicTest => "deterministic_test",
            Self::ExploitProvenTool => "exploit",
            Self::RouteSpecialist => "specialist",
            Self::TargetedInference => "targeted_inference",
            Self::NarrowSearch => "narrow_search",
            Self::ParallelValidation => "parallel_validation",
            Self::ReplaceInferenceWithTool => "replace_inference",
            Self::AcquireCapability => "acquire",
            Self::RetryCorrected => "retry_corrected",
            Self::SubstitutePersonnel => "substitute",
            Self::FieldGoalExtract => "field_goal",
            Self::PuntPossession => "punt",
        }
    }
}

#[tracing::instrument]
fn expected_reduction(definition: &PlayDefinition, gates: &[Gate]) -> u32 {
    let Some(target) = definition.target_gate.as_deref() else {
        return if definition.survey {
            0
        } else {
            definition.expected_ytg_reduction_milli
        };
    };
    let Some(gate) = gates.iter().find(|gate| gate.id.as_str() == target) else {
        return 0;
    };
    if gate.is_terminal() {
        return 0;
    }
    let ready = gate.dependencies.iter().all(|dep| {
        gates
            .iter()
            .any(|item| &item.id == dep && item.is_terminal())
    });
    if !ready {
        return 0;
    }
    gate_unresolved_milli(gate, gates)
}

#[tracing::instrument]
fn target_gate_rejection(definition: &PlayDefinition, gates: &[Gate]) -> &'static str {
    let Some(target) = definition.target_gate.as_deref() else {
        return "target gate has no unresolved yards";
    };
    let Some(gate) = gates.iter().find(|gate| gate.id.as_str() == target) else {
        return "target gate is absent";
    };
    if gate.is_terminal() {
        return "target gate is terminal";
    }
    if gate.dependencies.iter().any(|dep| {
        !gates
            .iter()
            .any(|item| &item.id == dep && item.is_terminal())
    }) {
        return "dependency is not terminal";
    }
    "target gate has no unresolved yards"
}

#[tracing::instrument]
fn score(person: &Personnel, policy: &Policy, priors: &HistoricalPriorTable) -> u32 {
    let weights = &policy.routing;
    // A learned prior is measured ground truth, so it outranks the
    // personnel's own estimate. The table stores realized reductions; the
    // routing term wants a residual, so the average reduction converts into
    // the yards the field still holds after this person's play. The
    // self-report only fills the gap until enough outcomes exist.
    let ytg = match priors.prior_milli(&person.id, weights.prior_min_samples) {
        Some(avg_reduction) => {
            YTG_REFERENCE_MILLI.saturating_sub(avg_reduction.min(YTG_REFERENCE_MILLI))
        }
        None => person
            .own_ytg_milli
            .unwrap_or(YTG_REFERENCE_MILLI)
            .min(YTG_REFERENCE_MILLI),
    };
    let ytg_fit = (YTG_REFERENCE_MILLI - ytg).saturating_mul(PPM_SCALE) / YTG_REFERENCE_MILLI;
    let cost_fit = PPM_SCALE.saturating_mul(1_000)
        / u32::try_from(person.cost_tokens.saturating_add(1_000))
            .unwrap_or(PPM_SCALE)
            .max(1);
    let latency_fit = PPM_SCALE.saturating_mul(1_000)
        / u32::try_from(person.latency_ms.saturating_add(1_000))
            .unwrap_or(PPM_SCALE)
            .max(1);
    let parts = [
        (weights.ytg, ytg_fit),
        (weights.success, person.reliability_ppm.min(PPM_SCALE)),
        (weights.cost, cost_fit),
        (weights.latency, latency_fit.min(PPM_SCALE)),
        (
            weights.capability_fit,
            person.capability_fit_ppm.min(PPM_SCALE),
        ),
        (weights.reliability, person.reliability_ppm.min(PPM_SCALE)),
        (weights.quota, person.quota_remaining_ppm.min(PPM_SCALE)),
        (
            weights.evidence_quality,
            person.evidence_quality_ppm.min(PPM_SCALE),
        ),
    ];
    let weight: u64 = parts.iter().map(|(weight, _)| u64::from(*weight)).sum();
    if weight == 0 {
        return 0;
    }
    let total: u64 = parts
        .iter()
        .map(|(weight, value)| u64::from(*weight) * u64::from(*value))
        .sum();
    u32::try_from(total / weight).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{DriveStatus, EvidenceClass, Gate, GateId, PersonnelId, PersonnelKind};

    #[tracing::instrument]
fn person(id: &str, kind: PersonnelKind, cost: u64) -> Personnel {
        Personnel {
            id: PersonnelId::new(id).unwrap(),
            kind,
            display_name: id.to_string(),
            cost_tokens: cost,
            latency_ms: 100,
            capability_fit_ppm: 800_000,
            reliability_ppm: 800_000,
            quota_remaining_ppm: 800_000,
            evidence_quality_ppm: 800_000,
            own_ytg_milli: None,
        }
    }

    #[test]
    #[tracing::instrument]
fn deterministic_personnel_sort_ahead_of_specialists() {
        let policy = Policy::builtin();
        let ranked = rank(
            &[
                person("specialist", PersonnelKind::SpecialistAgent, 10),
                person("cargo", PersonnelKind::DeterministicProcedure, 50),
            ],
            &policy,
            &HistoricalPriorTable::empty(),
        );
        assert_eq!(ranked[0].id.as_str(), "cargo");
        assert!(ranked[0].tier < ranked[1].tier);
    }

    #[test]
    #[tracing::instrument]
fn the_learned_prior_replaces_the_self_report() {
        let policy = Policy::builtin();
        let mut priors = HistoricalPriorTable::empty();
        let specialist = PersonnelId::new("measured").unwrap();
        for _ in 0..policy.routing.prior_min_samples {
            priors.record(&specialist, 2_000);
        }
        // The same id, two opposite self-reports: with enough outcomes behind
        // the id, both score identically. The table decides, not the claim.
        let mut shy = person("measured", PersonnelKind::SpecialistAgent, 10);
        shy.own_ytg_milli = Some(YTG_REFERENCE_MILLI);
        let mut bold = person("measured", PersonnelKind::SpecialistAgent, 10);
        bold.own_ytg_milli = Some(0);
        let with_prior = score(&shy, &policy, &priors);
        assert_eq!(with_prior, score(&bold, &policy, &priors));
        // Without the table the two self-reports score apart, so the equality
        // above can only come from the measured record.
        assert_ne!(
            score(&shy, &policy, &HistoricalPriorTable::empty()),
            score(&bold, &policy, &HistoricalPriorTable::empty())
        );
        // The effective residual is the measured one (10_000 - 2_000), not
        // either self-report.
        let mut claimant = person("claimant", PersonnelKind::SpecialistAgent, 10);
        claimant.own_ytg_milli = Some(YTG_REFERENCE_MILLI - 2_000);
        assert_eq!(
            with_prior,
            score(&claimant, &policy, &HistoricalPriorTable::empty())
        );
    }

    #[test]
    #[tracing::instrument]
fn priors_below_the_sample_threshold_do_not_apply() {
        let policy = Policy::builtin();
        let mut priors = HistoricalPriorTable::empty();
        let id = PersonnelId::new("scarce").unwrap();
        priors.record(&id, 9_999);
        let person = person("scarce", PersonnelKind::SpecialistAgent, 10);
        let with_prior = score(&person, &policy, &priors);
        let without_prior = score(&person, &policy, &HistoricalPriorTable::empty());
        assert_eq!(with_prior, without_prior);
    }

    #[test]
    #[tracing::instrument]
fn failed_outcomes_drag_the_prior_down() {
        let policy = Policy::builtin();
        let mut priors = HistoricalPriorTable::empty();
        let id = PersonnelId::new("worker").unwrap();
        priors.record(&id, 4_000);
        priors.record(&id, 4_000);
        priors.record(&id, 0);
        let person = person("worker", PersonnelKind::SpecialistAgent, 10);
        let prior = priors
            .prior_milli(&id, policy.routing.prior_min_samples)
            .unwrap();
        assert_eq!(prior, 2_666);
        let scored = score(&person, &policy, &priors);
        let perfect = {
            let mut perfect = HistoricalPriorTable::empty();
            for _ in 0..policy.routing.prior_min_samples {
                perfect.record(&id, 4_000);
            }
            score(&person, &policy, &perfect)
        };
        assert!(scored < perfect);
    }

    #[test]
    #[tracing::instrument]
fn a_swarm_with_no_marginal_gain_is_rejected() {
        let err = admit_swarm(
            &SwarmOrder {
                objective: "look busy".into(),
                maximum_workers: 8,
                worker_roles: vec!["worker".into()],
                budget_envelope: playbook_core::DriveBudget::zero(),
                evidence_requirements: vec![],
                termination: "never".into(),
                merge_strategy: "concat".into(),
                expected_marginal_ytg_milli: 0,
            },
            &Policy::builtin(),
        )
        .unwrap_err();
        assert_eq!(err, PlaybookError::SwarmUnjustified);
    }

    #[test]
    #[tracing::instrument]
fn a_blocked_gate_does_not_outrank_its_dependency() {
        let policy = Policy::builtin();
        let implementation = Gate::declare(
            GateId::new("implementation").unwrap(),
            "implementation",
            400_000,
            Vec::new(),
            vec![EvidenceClass::CompilerResult],
            true,
        )
        .unwrap();
        let unit = Gate::declare(
            GateId::new("unit_tests").unwrap(),
            "unit",
            300_000,
            vec![GateId::new("implementation").unwrap()],
            vec![EvidenceClass::TestReport],
            true,
        )
        .unwrap();
        let integration = Gate::declare(
            GateId::new("integration_tests").unwrap(),
            "integration",
            300_000,
            vec![GateId::new("unit_tests").unwrap()],
            vec![EvidenceClass::TestReport],
            true,
        )
        .unwrap();
        let field = FieldPosition {
            down: Down::SecondDownGain,
            preflight_complete: true,
            field_surveyed: true,
            status: DriveStatus::Active,
            fingerprint: "field".into(),
            failure_fingerprint: None,
            last_failed_operation: None,
            last_failure_kind: None,
            last_failed_personnel: None,
            unresolved_effects: false,
            gates: vec![implementation, unit, integration],
            personnel: vec![person(
                "cargo-toolchain",
                PersonnelKind::DeterministicProcedure,
                40,
            )],
            evidence_refs: Vec::new(),
            state_hash: "hash".into(),
            policy_version: policy.version.clone(),
            available_capabilities: vec!["elci.cargo".into()],
            priors: HistoricalPriorTable::empty(),
        };
        let decision = select(&field, &policy).unwrap();
        assert_eq!(decision.selected_operation, "compile");
        assert!(decision
            .rejection_reasons
            .iter()
            .any(|reason| { reason == "test.integration rejected: dependency is not terminal" }));
    }

    #[test]
    #[tracing::instrument]
fn a_terminal_target_gate_has_a_specific_rejection_reason() {
        let policy = Policy::builtin();
        let mut implementation = Gate::declare(
            GateId::new("implementation").unwrap(),
            "implementation",
            400_000,
            Vec::new(),
            vec![EvidenceClass::CompilerResult],
            true,
        )
        .unwrap();
        implementation.state = playbook_core::GateState::Satisfied;
        let field = FieldPosition {
            down: Down::SecondDownGain,
            preflight_complete: true,
            field_surveyed: true,
            status: DriveStatus::Active,
            fingerprint: "field".into(),
            failure_fingerprint: None,
            last_failed_operation: None,
            last_failure_kind: None,
            last_failed_personnel: None,
            unresolved_effects: false,
            gates: vec![implementation],
            personnel: vec![person(
                "cargo-toolchain",
                PersonnelKind::DeterministicProcedure,
                40,
            )],
            evidence_refs: Vec::new(),
            state_hash: "hash".into(),
            policy_version: policy.version.clone(),
            available_capabilities: vec!["elci.cargo".into()],
            priors: HistoricalPriorTable::empty(),
        };

        let decision = select(&field, &policy).unwrap();
        assert!(decision
            .rejection_reasons
            .iter()
            .any(|reason| { reason == "compile rejected: target gate is terminal" }));
    }
}
