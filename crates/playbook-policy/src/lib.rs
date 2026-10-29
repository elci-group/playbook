//! Data-driven tactical policy.
//!
//! Thresholds, plays, costs, and permissions live in this bundle. The state
//! engine reads them. It does not grow a second football strategy in `if` chains.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use playbook_core::{
    accept_schema, BudgetWeights, DriveBudget, EvidenceClass, HuddleBudget, PersonnelKind,
    PlayDefinition, PlayKind, PlaybookError, SideEffectClass, SCHEMA_VERSION,
};

fn default_prior_min_samples() -> u32 {
    3
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingWeights {
    pub ytg: u32,
    pub success: u32,
    pub cost: u32,
    pub latency: u32,
    pub capability_fit: u32,
    pub reliability: u32,
    pub quota: u32,
    pub evidence_quality: u32,
    #[serde(default = "default_prior_min_samples")]
    pub prior_min_samples: u32,
}

/// Coherence-debt component weights. Components are 0..=1000 per-mille; the
/// weighted sum is divided by 1000 and clamped to 1000.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleDebtWeights {
    pub assumption: u32,
    pub dependency: u32,
    pub contradiction: u32,
    pub turnover: u32,
    pub cross_impact: u32,
    pub blocker: u32,
    pub context_age: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantWeights {
    pub dependency_overlap: u32,
    pub contradiction_relevance: u32,
    pub ownership: u32,
    pub downstream_impact: u32,
    pub specialist_value: u32,
    pub context_cost: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleBudgets {
    pub position: HuddleBudget,
    pub unit: HuddleBudget,
    pub coordinator: HuddleBudget,
    pub team: HuddleBudget,
    pub franchise: HuddleBudget,
}

/// Huddle thresholds. Exact bands, budgets, and admission floors live here,
/// never in compiled business logic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddlePolicy {
    pub failure_threshold: u32,
    pub minimum_expected_value: i64,
    pub debt_monitor: u32,
    pub debt_eligible: u32,
    pub debt_coordinator: u32,
    pub debt_mandatory: u32,
    pub min_participant_score: i64,
    pub debt_weights: HuddleDebtWeights,
    pub participant_weights: ParticipantWeights,
    pub budgets: HuddleBudgets,
}

impl HuddlePolicy {
    /// The envelope for a scope key: position, unit, coordinator, team, or
    /// franchise.
    pub fn budget(&self, scope: &str) -> HuddleBudget {
        match scope {
            "position" => self.budgets.position,
            "unit" => self.budgets.unit,
            "coordinator" => self.budgets.coordinator,
            "team" => self.budgets.team,
            _ => self.budgets.franchise,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: String,
    pub schema_version: u32,
    pub first_down_attempt_threshold: u32,
    pub rush_budget_fraction_ppm: u32,
    pub conversion_threshold_ppm: u32,
    pub workload_thresholds: BTreeMap<String, u32>,
    pub min_attempt_ppm: u32,
    pub punt_preserve_ppm: u32,
    pub receiver_finish_ppm: u32,
    pub reset_bonus_ppm: u32,
    pub retry_limit: u32,
    pub stall_penalty_ppm: u32,
    pub capability_acquisition_limit: u32,
    pub swarm_min_marginal_milli: u32,
    pub rush_deterministic_bonus_ppm: u32,
    pub budget_weights: BudgetWeights,
    pub allowed_side_effects: Vec<SideEffectClass>,
    pub plays: Vec<PlayDefinition>,
    pub routing: RoutingWeights,
    pub huddle: HuddlePolicy,
}

impl Policy {
    pub fn conversion_threshold(&self, workload_class: &str) -> u32 {
        self.workload_thresholds
            .get(workload_class)
            .copied()
            .unwrap_or(self.conversion_threshold_ppm)
    }

    pub fn play(&self, operation: &str) -> Option<&PlayDefinition> {
        self.plays.iter().find(|play| play.operation == operation)
    }

    pub fn builtin() -> Self {
        let mut workload_thresholds = BTreeMap::new();
        workload_thresholds.insert("engineering".to_string(), 450_000);
        Self {
            version: "playbook-policy-1".to_string(),
            schema_version: SCHEMA_VERSION,
            first_down_attempt_threshold: 1,
            rush_budget_fraction_ppm: 500_000,
            conversion_threshold_ppm: 450_000,
            workload_thresholds,
            min_attempt_ppm: 550_000,
            punt_preserve_ppm: 980_000,
            receiver_finish_ppm: 800_000,
            reset_bonus_ppm: 0,
            retry_limit: 1,
            stall_penalty_ppm: 100_000,
            capability_acquisition_limit: 2,
            swarm_min_marginal_milli: 1,
            rush_deterministic_bonus_ppm: 200_000,
            budget_weights: BudgetWeights {
                tokens: 1,
                monetary_cost_micros: 0,
                wall_clock_ms: 0,
                tool_calls: 0,
                inference_calls: 0,
                retry_count: 0,
                context_capacity: 0,
                provider_quota: 1,
            },
            allowed_side_effects: vec![SideEffectClass::ReadOnly, SideEffectClass::LocalMutation],
            plays: builtin_plays(),
            routing: RoutingWeights {
                ytg: 1,
                success: 1,
                cost: 1,
                latency: 1,
                capability_fit: 1,
                reliability: 1,
                quota: 1,
                evidence_quality: 1,
                prior_min_samples: default_prior_min_samples(),
            },
            huddle: HuddlePolicy {
                failure_threshold: 2,
                minimum_expected_value: 0,
                debt_monitor: 200,
                debt_eligible: 400,
                debt_coordinator: 600,
                debt_mandatory: 800,
                min_participant_score: 2_000,
                debt_weights: HuddleDebtWeights {
                    assumption: 300,
                    dependency: 200,
                    contradiction: 400,
                    turnover: 100,
                    cross_impact: 200,
                    blocker: 300,
                    context_age: 50,
                },
                participant_weights: ParticipantWeights {
                    dependency_overlap: 3,
                    contradiction_relevance: 4,
                    ownership: 2,
                    downstream_impact: 3,
                    specialist_value: 2,
                    context_cost: 1,
                },
                budgets: HuddleBudgets {
                    position: HuddleBudget {
                        max_tokens: 2_000,
                        max_cost_microunits: 200_000,
                        max_duration_ms: 5_000,
                        max_rounds: 1,
                        max_participants: 3,
                    },
                    unit: HuddleBudget {
                        max_tokens: 6_000,
                        max_cost_microunits: 600_000,
                        max_duration_ms: 15_000,
                        max_rounds: 2,
                        max_participants: 5,
                    },
                    coordinator: HuddleBudget {
                        max_tokens: 8_000,
                        max_cost_microunits: 800_000,
                        max_duration_ms: 20_000,
                        max_rounds: 2,
                        max_participants: 6,
                    },
                    team: HuddleBudget {
                        max_tokens: 16_000,
                        max_cost_microunits: 1_600_000,
                        max_duration_ms: 60_000,
                        max_rounds: 3,
                        max_participants: 12,
                    },
                    franchise: HuddleBudget {
                        max_tokens: 32_000,
                        max_cost_microunits: 3_200_000,
                        max_duration_ms: 120_000,
                        max_rounds: 3,
                        max_participants: 24,
                    },
                },
            },
        }
    }

    /// Same plays, with the rush line low enough that a 40% reserve still rushes.
    pub fn pass_heavy_contrast() -> Self {
        let mut policy = Self::builtin();
        policy.version = "playbook-policy-pass-heavy".to_string();
        policy.rush_budget_fraction_ppm = 100_000;
        policy
    }
}

pub fn from_str(text: &str) -> Result<Policy, PlaybookError> {
    let policy: Policy = serde_json::from_str(text).map_err(PlaybookError::json)?;
    accept_schema(policy.schema_version)?;
    if policy.version.is_empty() || policy.plays.is_empty() {
        return Err(PlaybookError::Evidence("policy is missing plays".into()));
    }
    if policy.rush_budget_fraction_ppm > playbook_core::PPM_SCALE
        || policy.conversion_threshold_ppm > playbook_core::PPM_SCALE
    {
        return Err(PlaybookError::Probability(policy.rush_budget_fraction_ppm));
    }
    if !(policy.huddle.debt_monitor < policy.huddle.debt_eligible
        && policy.huddle.debt_eligible < policy.huddle.debt_coordinator
        && policy.huddle.debt_coordinator < policy.huddle.debt_mandatory
        && policy.huddle.debt_mandatory <= 1_000)
    {
        return Err(PlaybookError::Evidence(
            "huddle coherence-debt bands must ascend below 1000".into(),
        ));
    }
    Ok(policy)
}

fn cost(tokens: u64) -> DriveBudget {
    DriveBudget {
        tokens,
        monetary_cost_micros: tokens.saturating_mul(100),
        tool_calls: 1,
        ..DriveBudget::zero()
    }
}

struct PlaySeed {
    kind: PlayKind,
    name: &'static str,
    personnel_kind: PersonnelKind,
    operation: &'static str,
    capability_id: &'static str,
    target_gate: Option<&'static str>,
    evidence_class: EvidenceClass,
    expected: u32,
    tokens: u64,
    reliability_ppm: u32,
    survey: bool,
}

fn play(seed: PlaySeed) -> PlayDefinition {
    PlayDefinition {
        kind: seed.kind,
        name: seed.name.to_string(),
        personnel_kind: seed.personnel_kind,
        operation: seed.operation.to_string(),
        capability_id: seed.capability_id.to_string(),
        target_gate: seed.target_gate.map(str::to_string),
        side_effect: SideEffectClass::ReadOnly,
        expected_ytg_reduction_milli: seed.expected,
        marginal_cost: cost(seed.tokens),
        reliability_ppm: seed.reliability_ppm,
        survey: seed.survey,
        evidence_class: seed.evidence_class,
    }
}

fn builtin_plays() -> Vec<PlayDefinition> {
    vec![
        play(PlaySeed {
            kind: PlayKind::DiscoverCapabilities,
            name: "discover",
            personnel_kind: PersonnelKind::DeterministicProcedure,
            operation: "discover",
            capability_id: "elci.cargo",
            target_gate: None,
            evidence_class: EvidenceClass::CapabilitySnapshot,
            expected: 0,
            tokens: 10,
            reliability_ppm: 990_000,
            survey: true,
        }),
        play(PlaySeed {
            kind: PlayKind::InspectEnvironment,
            name: "inspect",
            personnel_kind: PersonnelKind::DeterministicProcedure,
            operation: "inspect",
            capability_id: "elci.cargo",
            target_gate: None,
            evidence_class: EvidenceClass::EnvironmentSnapshot,
            expected: 0,
            tokens: 20,
            reliability_ppm: 980_000,
            survey: true,
        }),
        play(PlaySeed {
            kind: PlayKind::RunDeterministicTest,
            name: "compile",
            personnel_kind: PersonnelKind::DeterministicProcedure,
            operation: "compile",
            capability_id: "elci.cargo",
            target_gate: Some("implementation"),
            evidence_class: EvidenceClass::CompilerResult,
            expected: 4_000,
            tokens: 200,
            reliability_ppm: 960_000,
            survey: false,
        }),
        play(PlaySeed {
            kind: PlayKind::RunDeterministicTest,
            name: "unit",
            personnel_kind: PersonnelKind::DeterministicProcedure,
            operation: "test.unit",
            capability_id: "elci.cargo",
            target_gate: Some("unit_tests"),
            evidence_class: EvidenceClass::TestReport,
            expected: 3_000,
            tokens: 400,
            reliability_ppm: 950_000,
            survey: false,
        }),
        play(PlaySeed {
            kind: PlayKind::RunDeterministicTest,
            name: "integration",
            personnel_kind: PersonnelKind::DeterministicProcedure,
            operation: "test.integration",
            capability_id: "elci.cargo",
            target_gate: Some("integration_tests"),
            evidence_class: EvidenceClass::TestReport,
            expected: 3_000,
            tokens: 100,
            reliability_ppm: 940_000,
            survey: false,
        }),
        play(PlaySeed {
            kind: PlayKind::RouteSpecialist,
            name: "specialist",
            personnel_kind: PersonnelKind::SpecialistAgent,
            operation: "specialist.review",
            capability_id: "personnel.specialist",
            target_gate: Some("integration_tests"),
            evidence_class: EvidenceClass::TestReport,
            expected: 3_000,
            tokens: 800,
            reliability_ppm: 700_000,
            survey: false,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_policy_round_trips() {
        let text = serde_json::to_string(&Policy::builtin()).unwrap();
        let loaded = from_str(&text).unwrap();
        assert_eq!(loaded, Policy::builtin());
    }

    /// Deterministic regeneration for the policy fixtures. Run explicitly:
    /// cargo test -p playbook-policy --offline -- --ignored regenerate_policy_fixtures
    #[test]
    #[ignore]
    fn regenerate_policy_fixtures() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (name, policy) in [
            ("policy.json", Policy::builtin()),
            ("policy_pass_heavy.json", Policy::pass_heavy_contrast()),
        ] {
            let text = serde_json::to_string_pretty(&policy).unwrap();
            std::fs::write(root.join("fixtures").join(name), text).unwrap();
        }
    }
}
