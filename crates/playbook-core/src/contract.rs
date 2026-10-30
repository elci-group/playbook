//! The game contract.
//!
//! A game is won only when every mandatory win condition in its sealed
//! contract is independently verified against measured state. A model
//! reporting that work is complete is a claim, never a win; a task ending
//! because an agent stopped responding is never a completion. Every
//! condition here is externally inspectable, individually addressable, and
//! deterministically checkable — no condition consults a model, and no
//! condition can be satisfied by narrative.

use serde::{Deserialize, Serialize};

use crate::canonical::{sha256_hex, to_canonical_bytes};
use crate::error::PlaybookError;
use crate::ids::{GameId, GateId, WinConditionId};
use crate::{ArtefactRef, DriveBudget, Evidence, EvidenceClass, Gate, GateState, TaskContract};

/// What one win condition checks, and everything needed to check it. The
/// directive's examples map one-to-one: build succeeds and tests pass are
/// [`WinConditionKind::GateSatisfied`], a security scan clean is
/// [`WinConditionKind::EvidencePresent`], a required file is
/// [`WinConditionKind::ArtefactValidated`], and a prohibited dependency
/// absent is [`WinConditionKind::EnvironmentFact`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum WinConditionKind {
    /// The named gate is terminal-satisfied.
    GateSatisfied { gate: GateId },
    /// At least `minimum` sealed pieces of evidence of the class exist and
    /// none has been invalidated.
    EvidencePresent { class: EvidenceClass, minimum: u32 },
    /// At least `minimum` validated artefacts exist.
    ArtefactValidated { minimum: u32 },
    /// The environment snapshot records `key` with exactly `value`.
    EnvironmentFact { key: String, value: String },
}

impl WinConditionKind {
    /// The machine key for reports and journals.
    pub #[tracing::instrument]
fn key(&self) -> &'static str {
        match self {
            Self::GateSatisfied { .. } => "gate_satisfied",
            Self::EvidencePresent { .. } => "evidence_present",
            Self::ArtefactValidated { .. } => "artefact_validated",
            Self::EnvironmentFact { .. } => "environment_fact",
        }
    }
}

/// One individually addressable win condition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WinCondition {
    pub id: WinConditionId,
    pub kind: WinConditionKind,
    pub description: String,
    /// A mandatory condition that is unmet blocks the win. Advisory
    /// conditions are recorded but never gate completion.
    pub mandatory: bool,
}

/// How a game is lost. Losses are contract facts too, checked with the same
/// determinism as wins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LossCondition {
    pub id: WinConditionId,
    pub kind: LossConditionKind,
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LossConditionKind {
    /// The game budget was exceeded.
    BudgetExhausted,
    /// A mandatory gate failed terminally.
    MandatoryGateFailed,
    /// The deadline passed without a win.
    DeadlinePassed,
}

/// Which evidence classes may count toward win conditions. Narrative claims
/// are never accepted — the verifier enforces that independently of policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePolicy {
    pub accepted_classes: Vec<EvidenceClass>,
}

impl EvidencePolicy {
    pub #[tracing::instrument]
fn accepts(&self, class: EvidenceClass) -> bool {
        class.can_satisfy_gate() && self.accepted_classes.contains(&class)
    }
}

/// The sealed contract for one game: the objective, what a win means, how a
/// loss is recognised, the budget envelope, and the evidence policy. Sealed
/// content-addressably; amendment happens only through a new sealed contract
/// that names its predecessor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameContract {
    pub game_id: GameId,
    pub objective: TaskContract,
    pub win_conditions: Vec<WinCondition>,
    pub loss_conditions: Vec<LossCondition>,
    pub budget: DriveBudget,
    pub evidence_policy: EvidencePolicy,
    pub deadline_ms: Option<u64>,
    pub predecessor_hash: Option<String>,
    pub contract_hash: String,
}

impl GameContract {
    /// Seal the contract. Win condition ids must be unique; at least one
    /// mandatory win condition is required, or the contract could never be
    /// won and could never lose honestly.
    pub #[tracing::instrument]
fn seal(mut self) -> Result<Self, PlaybookError> {
        if self.win_conditions.is_empty() {
            return Err(PlaybookError::Evidence(
                "game contract requires at least one win condition".into(),
            ));
        }
        if !self
            .win_conditions
            .iter()
            .any(|condition| condition.mandatory)
        {
            return Err(PlaybookError::Evidence(
                "game contract requires at least one mandatory win condition".into(),
            ));
        }
        let mut ids: Vec<&str> = self
            .win_conditions
            .iter()
            .map(|condition| condition.id.as_str())
            .collect();
        ids.sort_unstable();
        let unique = ids.len();
        ids.dedup();
        if ids.len() != unique {
            return Err(PlaybookError::Evidence(
                "win condition ids must be unique".into(),
            ));
        }
        self.contract_hash.clear();
        let bytes = to_canonical_bytes(&self)?;
        self.contract_hash = sha256_hex(&bytes);
        Ok(self)
    }

    /// The contract is well-formed and the digest matches its contents.
    pub #[tracing::instrument]
fn verify(&self) -> Result<(), PlaybookError> {
        let mut unsealed = self.clone();
        unsealed.contract_hash.clear();
        let sealed = sha256_hex(&to_canonical_bytes(&unsealed)?);
        if sealed != self.contract_hash {
            return Err(PlaybookError::DigestMismatch);
        }
        Ok(())
    }

    /// An audited amendment: a new sealed contract naming this one as its
    /// predecessor. Immutability is preserved — the old contract is never
    /// mutated, only superseded.
    pub #[tracing::instrument]
fn amend(mut self, evidence_policy: EvidencePolicy) -> Self {
        self.predecessor_hash = Some(self.contract_hash.clone());
        self.evidence_policy = evidence_policy;
        self.contract_hash.clear();
        self.seal()
            .expect("an amendment of a valid contract seals cleanly")
    }
}

/// One checked win condition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionResult {
    pub condition_id: WinConditionId,
    pub kind_key: String,
    pub passed: bool,
    pub reason_code: String,
}

/// The deterministic verdict over a whole contract. `verified_win` is true
/// only when every mandatory win condition passed — the only definition of
/// victory the system recognises.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractVerification {
    pub contract_hash: String,
    pub results: Vec<ConditionResult>,
    pub verified_win: bool,
    pub mandatory_unmet: u32,
}

/// Check every win condition against measured drive state. Deterministic:
/// the same state and contract always produce the same verdict.
pub #[tracing::instrument]
fn verify_win_conditions(
    contract: &GameContract,
    gates: &[Gate],
    evidence: &[Evidence],
    artefacts: &[ArtefactRef],
    environment_snapshot: &serde_json::Value,
) -> ContractVerification {
    let results: Vec<ConditionResult> = contract
        .win_conditions
        .iter()
        .map(|condition| {
            let (passed, reason_code) = check(
                condition,
                contract,
                gates,
                evidence,
                artefacts,
                environment_snapshot,
            );
            ConditionResult {
                condition_id: condition.id.clone(),
                kind_key: condition.kind.key().to_string(),
                passed,
                reason_code,
            }
        })
        .collect();
    let mandatory_unmet = contract
        .win_conditions
        .iter()
        .zip(results.iter())
        .filter(|(condition, result)| condition.mandatory && !result.passed)
        .count() as u32;
    ContractVerification {
        contract_hash: contract.contract_hash.clone(),
        verified_win: mandatory_unmet == 0,
        mandatory_unmet,
        results,
    }
}

#[tracing::instrument]
fn check(
    condition: &WinCondition,
    contract: &GameContract,
    gates: &[Gate],
    evidence: &[Evidence],
    artefacts: &[ArtefactRef],
    environment_snapshot: &serde_json::Value,
) -> (bool, String) {
    match &condition.kind {
        WinConditionKind::GateSatisfied { gate } => {
            match gates.iter().find(|declared| &declared.id == gate) {
                Some(declared)
                    if declared.is_terminal() && declared.state == GateState::Satisfied =>
                {
                    (true, "gate_satisfied".into())
                }
                Some(_) => (false, "gate_not_satisfied".into()),
                None => (false, "unknown_gate".into()),
            }
        }
        WinConditionKind::EvidencePresent { class, minimum } => {
            if !contract.evidence_policy.accepts(*class) {
                return (false, "evidence_class_rejected_by_policy".into());
            }
            let count = evidence.iter().filter(|item| item.class == *class).count() as u32;
            if count >= *minimum {
                (true, "evidence_present".into())
            } else {
                (false, format!("evidence_shortfall_{count}_of_{minimum}"))
            }
        }
        WinConditionKind::ArtefactValidated { minimum } => {
            let count = artefacts.iter().filter(|item| item.validated).count() as u32;
            if count >= *minimum {
                (true, "artefacts_validated".into())
            } else {
                (false, format!("artefact_shortfall_{count}_of_{minimum}"))
            }
        }
        WinConditionKind::EnvironmentFact { key, value } => match environment_snapshot.get(key) {
            Some(observed) if observed == &serde_json::Value::String(value.clone()) => {
                (true, "environment_fact_holds".into())
            }
            Some(_) => (false, "environment_fact_conflicts".into()),
            None => (false, "environment_fact_absent".into()),
        },
    }
}

/// A loss declared by the contract, checked with the same determinism.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractLoss {
    BudgetExhausted,
    MandatoryGateFailed,
    DeadlinePassed,
}

/// Check the loss conditions. The first recognised loss names itself; an
/// unrecognised loss never terminates a game.
pub #[tracing::instrument]
fn check_loss_conditions(
    contract: &GameContract,
    gates: &[Gate],
    budget_consumed: crate::DriveBudget,
    elapsed_ms: u64,
) -> Option<ContractLoss> {
    let recognised = |kind: &LossConditionKind| -> bool {
        match kind {
            LossConditionKind::BudgetExhausted => {
                budget_consumed.tokens > contract.budget.tokens
                    || budget_consumed.monetary_cost_micros > contract.budget.monetary_cost_micros
            }
            LossConditionKind::MandatoryGateFailed => gates
                .iter()
                .any(|gate| gate.mandatory && gate.state == GateState::Failed),
            LossConditionKind::DeadlinePassed => contract
                .deadline_ms
                .is_some_and(|deadline| elapsed_ms > deadline),
        }
    };
    contract
        .loss_conditions
        .iter()
        .find(|condition| recognised(&condition.kind))
        .map(|condition| match condition.kind {
            LossConditionKind::BudgetExhausted => ContractLoss::BudgetExhausted,
            LossConditionKind::MandatoryGateFailed => ContractLoss::MandatoryGateFailed,
            LossConditionKind::DeadlinePassed => ContractLoss::DeadlinePassed,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArtefactId, Constraint, Down, DriveStatus, EvidenceId, TaskIntegrity};

    #[tracing::instrument]
fn gate(id: &str) -> Gate {
        Gate::declare(
            GateId::new(id).unwrap(),
            "compiler accepts",
            500_000,
            Vec::new(),
            vec![EvidenceClass::CompilerResult],
            true,
        )
        .unwrap()
    }

    #[tracing::instrument]
fn contract(win: WinCondition) -> GameContract {
        GameContract {
            game_id: GameId::new("game-1").unwrap(),
            objective: TaskContract {
                statement: "repair the parser".into(),
                workload_class: "engineering".into(),
                constraints: vec![Constraint {
                    key: "offline".into(),
                    value: "true".into(),
                }],
                integrity: TaskIntegrity::Sound,
            },
            win_conditions: vec![win],
            loss_conditions: vec![
                LossCondition {
                    id: WinConditionId::new("loss-budget").unwrap(),
                    kind: LossConditionKind::BudgetExhausted,
                    description: "budget gone".into(),
                },
                LossCondition {
                    id: WinConditionId::new("loss-gate").unwrap(),
                    kind: LossConditionKind::MandatoryGateFailed,
                    description: "a mandatory gate failed".into(),
                },
                LossCondition {
                    id: WinConditionId::new("loss-deadline").unwrap(),
                    kind: LossConditionKind::DeadlinePassed,
                    description: "the deadline passed".into(),
                },
            ],
            budget: DriveBudget {
                tokens: 1_000,
                ..DriveBudget::zero()
            },
            evidence_policy: EvidencePolicy {
                accepted_classes: vec![EvidenceClass::CompilerResult],
            },
            deadline_ms: Some(60_000),
            predecessor_hash: None,
            contract_hash: String::new(),
        }
        .seal()
        .unwrap()
    }

    #[tracing::instrument]
fn win() -> WinCondition {
        WinCondition {
            id: WinConditionId::new("win-build").unwrap(),
            kind: WinConditionKind::GateSatisfied {
                gate: GateId::new("build").unwrap(),
            },
            description: "the build gate is satisfied".into(),
            mandatory: true,
        }
    }

    #[test]
    #[tracing::instrument]
fn a_sealed_contract_round_trips_and_detects_tampering() {
        let sealed = contract(win());
        sealed.verify().unwrap();
        let mut tampered = sealed.clone();
        tampered.budget.tokens = 5_000;
        assert!(matches!(
            tampered.verify(),
            Err(PlaybookError::DigestMismatch)
        ));
    }

    #[test]
    #[tracing::instrument]
fn a_contract_without_mandatory_conditions_is_refused() {
        let mut relaxed = contract(win());
        relaxed.win_conditions[0].mandatory = false;
        relaxed.contract_hash.clear();
        assert!(relaxed.seal().is_err());
    }

    #[test]
    #[tracing::instrument]
fn duplicate_condition_ids_are_refused() {
        let mut duplicated = contract(win());
        duplicated.win_conditions.push(win());
        duplicated.contract_hash.clear();
        assert!(duplicated.seal().is_err());
    }

    #[test]
    #[tracing::instrument]
fn only_mandatory_conditions_gate_the_win() {
        let mut sealed = contract(win());
        sealed.win_conditions.push(WinCondition {
            id: WinConditionId::new("win-advisory").unwrap(),
            kind: WinConditionKind::ArtefactValidated { minimum: 3 },
            description: "advisory".into(),
            mandatory: false,
        });
        let sealed = sealed.seal().unwrap();
        let gates = vec![{
            let mut satisfied = gate("build");
            satisfied.state = GateState::Satisfied;
            satisfied.satisfaction_ppm = 1_000_000;
            satisfied.satisfying_evidence = Some(EvidenceId::new("ev-1").unwrap());
            satisfied
        }];
        let verdict = verify_win_conditions(&sealed, &gates, &[], &[], &serde_json::json!({}));
        assert!(verdict.verified_win);
        assert_eq!(verdict.mandatory_unmet, 0);
        assert_eq!(verdict.results.len(), 2);
        assert!(!verdict.results[1].passed);
    }

    #[test]
    #[tracing::instrument]
fn an_unsatisfied_gate_is_never_a_win() {
        let sealed = contract(win());
        let verdict = verify_win_conditions(&sealed, &[], &[], &[], &serde_json::json!({}));
        assert!(!verdict.verified_win);
        assert_eq!(verdict.mandatory_unmet, 1);
        assert_eq!(verdict.results[0].reason_code, "unknown_gate");
    }

    #[test]
    #[tracing::instrument]
fn evidence_policy_and_narrative_rejection_bind() {
        let mut sealed = contract(WinCondition {
            id: WinConditionId::new("win-scan").unwrap(),
            kind: WinConditionKind::EvidencePresent {
                class: EvidenceClass::StaticAnalysis,
                minimum: 1,
            },
            description: "security scan clean".into(),
            mandatory: true,
        });
        // The policy only accepts compiler results; the scan evidence cannot
        // count even though it exists.
        sealed.evidence_policy.accepted_classes = vec![EvidenceClass::CompilerResult];
        let sealed = sealed.seal().unwrap();
        let scan = Evidence::seal(
            EvidenceClass::StaticAnalysis,
            serde_json::json!({"findings": 0}),
            10,
        )
        .unwrap();
        let verdict = verify_win_conditions(&sealed, &[], &[scan], &[], &serde_json::json!({}));
        assert_eq!(
            verdict.results[0].reason_code,
            "evidence_class_rejected_by_policy"
        );
        // Narrative claims are refused even when the policy names them.
        let mut narrative_policy = sealed.clone();
        narrative_policy.evidence_policy.accepted_classes =
            vec![EvidenceClass::NarrativeClaim, EvidenceClass::CompilerResult];
        let claim = Evidence::seal(
            EvidenceClass::NarrativeClaim,
            serde_json::json!({"statement": "tests pass"}),
            11,
        )
        .unwrap();
        let verdict = verify_win_conditions(
            &narrative_policy,
            &[],
            &[claim],
            &[],
            &serde_json::json!({}),
        );
        assert_eq!(
            verdict.results[0].reason_code,
            "evidence_class_rejected_by_policy"
        );
    }

    #[test]
    #[tracing::instrument]
fn artefacts_and_environment_facts_are_checked_deterministically() {
        let sealed = contract(WinCondition {
            id: WinConditionId::new("win-artefact").unwrap(),
            kind: WinConditionKind::ArtefactValidated { minimum: 1 },
            description: "a validated artefact exists".into(),
            mandatory: true,
        })
        .seal()
        .unwrap();
        let unvalidated = ArtefactRef {
            id: ArtefactId::new("art-1").unwrap(),
            digest: "d".into(),
            validated: false,
            rejected: false,
            evidence_id: None,
        };
        assert!(
            !verify_win_conditions(&sealed, &[], &[], &[unvalidated], &serde_json::json!({}))
                .verified_win
        );
        let validated = ArtefactRef {
            id: ArtefactId::new("art-1").unwrap(),
            digest: "d".into(),
            validated: true,
            rejected: false,
            evidence_id: None,
        };
        assert!(
            verify_win_conditions(&sealed, &[], &[], &[validated], &serde_json::json!({}))
                .verified_win
        );

        let environment = contract(WinCondition {
            id: WinConditionId::new("win-env").unwrap(),
            kind: WinConditionKind::EnvironmentFact {
                key: "offline".into(),
                value: "true".into(),
            },
            description: "work stayed offline".into(),
            mandatory: true,
        })
        .seal()
        .unwrap();
        assert!(
            verify_win_conditions(
                &environment,
                &[],
                &[],
                &[],
                &serde_json::json!({"offline": "true"})
            )
            .verified_win
        );
        assert!(
            !verify_win_conditions(
                &environment,
                &[],
                &[],
                &[],
                &serde_json::json!({"offline": "false"})
            )
            .verified_win
        );
    }

    #[test]
    #[tracing::instrument]
fn amendment_supersedes_without_mutating_the_predecessor() {
        let original = contract(win());
        let amended = original.clone().amend(EvidencePolicy {
            accepted_classes: vec![EvidenceClass::CompilerResult, EvidenceClass::Benchmark],
        });
        assert_eq!(
            amended.predecessor_hash.as_deref(),
            Some(original.contract_hash.as_str())
        );
        assert_ne!(original.contract_hash, amended.contract_hash);
        amended.verify().unwrap();
    }

    #[test]
    #[tracing::instrument]
fn loss_conditions_recognise_budget_gates_and_deadlines() {
        let sealed = contract(win());
        assert_eq!(
            check_loss_conditions(
                &sealed,
                &[],
                DriveBudget {
                    tokens: 2_000,
                    ..DriveBudget::zero()
                },
                10
            ),
            Some(ContractLoss::BudgetExhausted)
        );
        let failed = {
            let mut gate = gate("build");
            gate.state = GateState::Failed;
            gate
        };
        assert_eq!(
            check_loss_conditions(&sealed, &[failed], DriveBudget::zero(), 10),
            Some(ContractLoss::MandatoryGateFailed)
        );
        assert_eq!(
            check_loss_conditions(&sealed, &[], DriveBudget::zero(), 60_001),
            Some(ContractLoss::DeadlinePassed)
        );
        assert_eq!(
            check_loss_conditions(&sealed, &[], DriveBudget::zero(), 10),
            None
        );
        let _ = (Down::FirstDown, DriveStatus::Active);
    }
}
