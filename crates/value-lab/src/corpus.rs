//! The task corpus: heterogeneous, pre-classified, frozen before execution
//! (§6–§7). Difficulty labels are never retroactively altered because a
//! system performed unexpectedly.

use serde::{Deserialize, Serialize};

/// The workload classes of record (§6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskClass {
    BugDiagnosis,
    BugRepair,
    MultiFileRepair,
    DependencyMigration,
    Refactoring,
    PerformanceOptimisation,
    SecurityRemediation,
    TestGeneration,
    RepositoryAnalysis,
    ArchitectureModification,
    SourceDiscovery,
    TechnicalComparison,
    EvidenceSynthesis,
    ContradictionResolution,
    LiteratureAnalysis,
    ArchitecturePlanning,
    MigrationPlanning,
    ImplementationDecomposition,
    RiskAnalysis,
    CostOptimisation,
    RepositoryNavigation,
    FileManipulation,
    ApiInteraction,
    TestExecution,
    BuildDebugLoop,
    LongHorizon,
}

/// The frozen difficulty stratum (§7). S5 tasks cannot reasonably be
/// assumed completable by any available line-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stratum {
    Routine,
    Moderate,
    Difficult,
    Adversarial,
    Frontier,
}

impl Stratum {
    pub fn key(self) -> &'static str {
        match self {
            Self::Routine => "S1_routine",
            Self::Moderate => "S2_moderate",
            Self::Difficult => "S3_difficult",
            Self::Adversarial => "S4_adversarial",
            Self::Frontier => "S5_frontier",
        }
    }
}

/// The frozen pre-run difficulty profile (§7): six dimensions, 0..=100.
/// Frozen at classification time; never edited after results exist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifficultyProfile {
    pub complexity: u8,
    pub uncertainty: u8,
    pub tool_dependence: u8,
    pub horizon: u8,
    pub failure_exposure: u8,
    pub verification_difficulty: u8,
}

/// One assignment of the corpus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub id: String,
    pub class: TaskClass,
    pub statement: String,
    pub stratum: Stratum,
    pub profile: DifficultyProfile,
    /// Externally defined Deliver Gates — the subject does not decide
    /// whether it succeeded (§11).
    pub deliver_gates: Vec<String>,
}

impl Assignment {
    /// Whether the assignment is eligible for the fault campaign: at least
    /// 20% of the corpus must carry controlled degradation (§17).
    pub fn adversarial_enough(&self) -> bool {
        self.profile.failure_exposure >= 50 || self.stratum >= Stratum::Adversarial
    }
}

/// The corpus. Frozen once; after `freeze`, mutation is refused.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    assignments: Vec<Assignment>,
    frozen: bool,
}

impl Corpus {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Add an assignment, refusing duplicates. Refused after freeze.
    pub fn add(&mut self, assignment: Assignment) -> Result<(), String> {
        if self.frozen {
            return Err("corpus is frozen".into());
        }
        if self
            .assignments
            .iter()
            .any(|existing| existing.id == assignment.id)
        {
            return Err(format!("duplicate assignment {}", assignment.id));
        }
        self.assignments.push(assignment);
        Ok(())
    }

    /// Freeze the corpus: classification can no longer drift (§6, §27).
    pub fn freeze(&mut self) -> usize {
        self.frozen = true;
        self.assignments.len()
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn len(&self) -> usize {
        self.assignments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty()
    }

    pub fn assignments(&self) -> &[Assignment] {
        &self.assignments
    }

    /// The assignments of one stratum — value is reported per stratum
    /// because an aggregate can conceal opposite effects (§7).
    pub fn by_stratum(&self, stratum: Stratum) -> Vec<&Assignment> {
        self.assignments
            .iter()
            .filter(|assignment| assignment.stratum == stratum)
            .collect()
    }

    /// The fraction of the corpus eligible for the fault campaign. The
    /// study needs at least 20% (§17).
    pub fn adversarial_fraction_ppm(&self) -> u32 {
        if self.assignments.is_empty() {
            return 0;
        }
        let adversarial = self
            .assignments
            .iter()
            .filter(|assignment| assignment.adversarial_enough())
            .count();
        ((adversarial as u64 * 1_000_000) / self.assignments.len() as u64) as u32
    }

    /// Corpus adequacy for a primary season (§6: at least 100 assignments,
    /// at least 20% adversarial).
    pub fn validate_primary(&self) -> Result<(), String> {
        if !self.frozen {
            return Err("corpus must be frozen before the primary season".into());
        }
        if self.len() < 100 {
            return Err(format!(
                "primary corpus needs at least 100 assignments, has {}",
                self.len()
            ));
        }
        if self.adversarial_fraction_ppm() < 200_000 {
            return Err(format!(
                "fault campaign needs at least 20% adversarial assignments, has {} ppm",
                self.adversarial_fraction_ppm()
            ));
        }
        Ok(())
    }
}

/// Build a legal 100-assignment primary corpus: five strata, a quarter of
/// it adversarial. The lab's own fixture; classes are drawn across the
/// §6 taxonomy.
pub fn corpus_for_study() -> Corpus {
    let mut corpus = Corpus::empty();
    let strata = [
        (Stratum::Routine, 30),
        (Stratum::Moderate, 30),
        (Stratum::Difficult, 20),
        (Stratum::Adversarial, 15),
        (Stratum::Frontier, 5),
    ];
    let classes = [
        TaskClass::BugDiagnosis,
        TaskClass::DependencyMigration,
        TaskClass::EvidenceSynthesis,
        TaskClass::RepositoryNavigation,
        TaskClass::LongHorizon,
        TaskClass::TestGeneration,
    ];
    let mut index = 0usize;
    for (stratum, count) in strata {
        for _ in 0..count {
            let class = classes[index % classes.len()];
            let exposure = if stratum >= Stratum::Adversarial {
                80
            } else {
                30
            };
            let _ = corpus.add(Assignment {
                id: format!("value-{index:03}"),
                class,
                statement: format!("assignment {index}: {class:?} at {stratum:?}"),
                stratum,
                profile: DifficultyProfile {
                    complexity: 40 + (index % 50) as u8,
                    uncertainty: 30 + (index % 60) as u8,
                    tool_dependence: 20 + (index % 70) as u8,
                    horizon: 25 + (index % 65) as u8,
                    failure_exposure: exposure,
                    verification_difficulty: 35 + (index % 55) as u8,
                },
                deliver_gates: vec!["tests pass".into(), "lint clean".into()],
            });
            index += 1;
        }
    }
    corpus.freeze();
    corpus
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment(id: &str, stratum: Stratum, failure_exposure: u8) -> Assignment {
        Assignment {
            id: id.into(),
            class: TaskClass::BugRepair,
            statement: format!("repair {id}"),
            stratum,
            profile: DifficultyProfile {
                complexity: 50,
                uncertainty: 50,
                tool_dependence: 50,
                horizon: 50,
                failure_exposure,
                verification_difficulty: 50,
            },
            deliver_gates: vec!["tests pass".into()],
        }
    }

    #[test]
    fn a_frozen_corpus_refuses_mutation_and_duplicates() {
        let mut corpus = Corpus::empty();
        corpus.add(assignment("a", Stratum::Routine, 0)).unwrap();
        assert!(corpus.add(assignment("a", Stratum::Routine, 0)).is_err());
        corpus.freeze();
        assert!(corpus.add(assignment("b", Stratum::Routine, 0)).is_err());
        assert!(corpus.is_frozen());
    }

    #[test]
    fn primary_validation_demands_scale_and_adversarial_share() {
        let mut corpus = Corpus::empty();
        for index in 0..100 {
            let stratum = if index < 25 {
                Stratum::Adversarial
            } else {
                Stratum::Routine
            };
            corpus
                .add(assignment(&format!("t{index}"), stratum, 80))
                .unwrap();
        }
        corpus.freeze();
        corpus.validate_primary().unwrap();
        // Below the adversarial floor, the corpus is refused.
        let mut thin = Corpus::empty();
        for index in 0..100 {
            thin.add(assignment(&format!("t{index}"), Stratum::Routine, 0))
                .unwrap();
        }
        thin.freeze();
        assert!(thin.validate_primary().is_err());
    }
}
