//! Conservative defensive control.
//!
//! Two doctrines for fragile states. Prevent sacrifices short gains to
//! prevent explosive plays: during fragile system states, catastrophic-risk
//! avoidance outranks progress, so incremental work continues while large
//! unverified changes are blocked. Make Them Drive forces the offence to
//! execute many consecutive plays: a high-risk change arrives as a sequence
//! of small validated stages, never one large patch — maximum patch size,
//! validation after each stage. Useful against brittle repositories and
//! weak agents.

use serde::{Deserialize, Serialize};

use crate::domain::{RiskBand, TacticalContext};

/// The defensive risk classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskState {
    Stable,
    Fragile,
    Critical,
}

/// The deterministic classification. Critical when the measured band is
/// critical or the remaining budget share has sunk to 100_000 ppm or less
/// (the boundary is inclusive); fragile on elevated risk, observed
/// divergence, or a coherence debt of 400 or more; stable otherwise.
pub fn classify(context: &TacticalContext) -> RiskState {
    if context.risk_band == RiskBand::Critical || context.budget_remaining_ppm <= 100_000 {
        return RiskState::Critical;
    }
    if context.risk_band >= RiskBand::Elevated
        || context.divergence_observed
        || context.coherence_debt >= 400
    {
        return RiskState::Fragile;
    }
    RiskState::Stable
}

/// The maximum change envelope for one state. Under fragile and critical
/// states, every change is staged and large unverified changes are blocked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeEnvelope {
    pub max_files: u16,
    pub max_lines: u16,
    pub requires_validation_after_each_stage: bool,
    pub blocks_large_unverified_changes: bool,
}

/// The envelope table. Stable work is trusted with breadth; fragile work is
/// staged and capped; critical work is nearly frozen.
pub fn envelope(state: RiskState) -> ChangeEnvelope {
    match state {
        RiskState::Stable => ChangeEnvelope {
            max_files: 50,
            max_lines: 5_000,
            requires_validation_after_each_stage: false,
            blocks_large_unverified_changes: false,
        },
        RiskState::Fragile => ChangeEnvelope {
            max_files: 10,
            max_lines: 800,
            requires_validation_after_each_stage: true,
            blocks_large_unverified_changes: true,
        },
        RiskState::Critical => ChangeEnvelope {
            max_files: 3,
            max_lines: 100,
            requires_validation_after_each_stage: true,
            blocks_large_unverified_changes: true,
        },
    }
}

/// A proposed change: one patch, or an explicitly staged sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeRequest {
    Patch {
        files: u16,
        lines: u16,
    },
    StagedPatch {
        stages: u16,
        files_per_stage: u16,
        lines_per_stage: u16,
    },
}

/// The verdict on a change request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeVerdict {
    Admitted,
    SplitIntoStages { stages: u16 },
    Refused { reason_code: String },
}

fn refused(reason: &str) -> ChangeVerdict {
    ChangeVerdict::Refused {
        reason_code: reason.into(),
    }
}

/// Adjudicate a change against the envelope. A single patch over the
/// envelope is admitted under Stable (breadth is trusted) and refused under
/// Fragile/Critical with the exceeding dimension named — the files
/// dimension is checked before the lines dimension. A single patch that
/// fits a staging envelope still splits: under Fragile/Critical it returns
/// `SplitIntoStages { stages: 1 }` so the validation gate applies. A staged
/// patch declaring no stages is refused (`staged_patch_declares_no_stages`);
/// one whose stage overflows the envelope is refused under Fragile/Critical
/// (`stage_exceeds_envelope`), while under Stable the stage sizes are
/// informational and the patch is admitted.
pub fn adjudicate(request: &ChangeRequest, envelope: &ChangeEnvelope) -> ChangeVerdict {
    match request {
        ChangeRequest::Patch { files, lines } => {
            let over_files = *files > envelope.max_files;
            let over_lines = *lines > envelope.max_lines;
            if !envelope.blocks_large_unverified_changes {
                return ChangeVerdict::Admitted;
            }
            if over_files {
                return refused("files_exceed_envelope");
            }
            if over_lines {
                return refused("lines_exceed_envelope");
            }
            if envelope.requires_validation_after_each_stage {
                return ChangeVerdict::SplitIntoStages { stages: 1 };
            }
            ChangeVerdict::Admitted
        }
        ChangeRequest::StagedPatch {
            stages,
            files_per_stage,
            lines_per_stage,
        } => {
            if *stages == 0 {
                return refused("staged_patch_declares_no_stages");
            }
            if envelope.blocks_large_unverified_changes
                && (*files_per_stage > envelope.max_files || *lines_per_stage > envelope.max_lines)
            {
                return refused("stage_exceeds_envelope");
            }
            ChangeVerdict::Admitted
        }
    }
}

/// Make Them Drive: split an oversized patch into the minimum number of
/// stages that fit the per-stage allowance — the larger of the two ceiling
/// divisions binds. A zero allowance is clamped to one so the division
/// cannot panic. A patch with no files and no lines is one empty stage; a
/// patch that touches no files but claims lines cannot be staged honestly —
/// that is `None`.
pub fn drive_stages(
    files: u16,
    lines: u16,
    max_files_per_stage: u16,
    max_lines_per_stage: u16,
) -> Option<u16> {
    if files == 0 && lines == 0 {
        return Some(1);
    }
    if files == 0 {
        return None;
    }
    let file_stages = files.div_ceil(max_files_per_stage.max(1));
    let line_stages = lines.div_ceil(max_lines_per_stage.max(1));
    Some(file_stages.max(line_stages))
}

/// The doctrine directive for one risk state: `prevent` under Critical,
/// `make_them_drive` under Fragile (and, with a softer always-on shape,
/// under Stable). Both pass the frozen contract's validation. Each declared
/// counter composes per the conflict matrix — `make_them_drive` names
/// `quick_game`, a conditional pair — while `prevent` declares no counter:
/// its natural counter is an explosive doctrine, which the matrix marks
/// conflicting.
pub fn directive(state: RiskState, context: &TacticalContext) -> crate::domain::TacticalDirective {
    let (name, trigger, protection, tempo, counter) = match state {
        // Prevent: catastrophic-risk avoidance. Its natural counter would be
        // an explosive doctrine, which the matrix refuses — so Prevent
        // declares no counter in this sprint; a doctrine this defensive is
        // its own constraint.
        RiskState::Critical => (
            "prevent",
            crate::domain::TacticalTrigger::RiskBandAtLeast {
                band: RiskBand::Critical,
            },
            900u32,
            50u32,
            None,
        ),
        // Make Them Drive: staged small validated changes. Quick Game is its
        // declared tempo partner; the matrix allows the pair.
        RiskState::Fragile => (
            "make_them_drive",
            crate::domain::TacticalTrigger::RiskBandAtLeast {
                band: RiskBand::Elevated,
            },
            600,
            200,
            Some("quick_game"),
        ),
        RiskState::Stable => (
            "make_them_drive",
            crate::domain::TacticalTrigger::Always,
            300,
            300,
            Some("quick_game"),
        ),
    };
    crate::domain::TacticalDirective {
        id: playbook_core::TacticalId::new(format!("t-{name}")).expect("tactic id"),
        name: name.into(),
        class: crate::domain::TacticalClass::Doctrine,
        trigger,
        benefit_ppm: 700,
        cost_ppm: 300,
        risk_ppm: 100,
        shape: crate::domain::PersonnelShape {
            primary_executors: 1,
            support: 2,
            reserves: 1,
            observers: 0,
            assignments: vec![crate::domain::DomainAssignment {
                domain: context
                    .critical_domain
                    .clone()
                    .unwrap_or_else(|| "release".into()),
                personnel: vec![playbook_core::PersonnelId::new("executor").unwrap()],
                mode: crate::domain::OwnershipMode::Sole,
            }],
        },
        information: crate::domain::InformationPolicy {
            max_context_overlap_ppm: 400,
            huddle_access: crate::domain::HuddleAccess::Sugar,
            disclose_failures_to_replacement: true,
        },
        concurrency: crate::domain::ConcurrencyPolicy {
            max_concurrent: 2,
            allow_domain_overlap: false,
        },
        authority: crate::domain::AuthorityPolicy {
            may_reassign: false,
            may_reallocate_budget: false,
            may_change_information_topology: false,
            observe_only: false,
        },
        budget: crate::domain::TacticalBudget {
            max_tokens: 20_000,
            max_cost_microunits: 2_000_000,
            max_duration_ms: 60_000,
            max_plays: 6,
        },
        expected_effect: crate::domain::ExpectedEffect {
            breadth_delta_ppm: 100,
            protection_delta_ppm: protection,
            tempo_delta_ppm: tempo,
            information_delta_ppm: 100,
        },
        termination: crate::domain::TerminationCondition {
            max_plays: 6,
            abort_when: vec![
                crate::domain::AbortTrigger::BudgetExhausted,
                crate::domain::AbortTrigger::WinConditionSatisfied,
            ],
        },
        counter_tactic: counter.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::validated;
    use crate::composition::compatibility;

    fn context(band: RiskBand, budget_ppm: u32, debt: u32, diverged: bool) -> TacticalContext {
        TacticalContext {
            problem_breadth_ppm: 300,
            domain_coupling_ppm: 400,
            provider_instability_ppm: 100,
            coherence_debt: debt,
            repeated_failures: 0,
            risk_band: band,
            divergence_observed: diverged,
            critical_domain: None,
            ytg_milli: 10_000,
            budget_remaining_ppm: budget_ppm,
            short_ytg: false,
            at_ms: 0,
        }
    }

    #[test]
    fn classification_follows_the_documented_boundaries() {
        assert_eq!(
            classify(&context(RiskBand::Low, 900_000, 0, false)),
            RiskState::Stable
        );
        assert_eq!(
            classify(&context(RiskBand::Elevated, 900_000, 0, false)),
            RiskState::Fragile
        );
        assert_eq!(
            classify(&context(RiskBand::Low, 900_000, 400, false)),
            RiskState::Fragile
        );
        assert_eq!(
            classify(&context(RiskBand::Low, 900_000, 0, true)),
            RiskState::Fragile
        );
        assert_eq!(
            classify(&context(RiskBand::Critical, 900_000, 0, false)),
            RiskState::Critical
        );
        // An empty wallet is critical regardless of the band.
        assert_eq!(
            classify(&context(RiskBand::Low, 100_000, 0, false)),
            RiskState::Critical
        );
    }

    #[test]
    fn the_envelope_table_is_exact() {
        assert_eq!(
            envelope(RiskState::Stable),
            ChangeEnvelope {
                max_files: 50,
                max_lines: 5_000,
                requires_validation_after_each_stage: false,
                blocks_large_unverified_changes: false,
            }
        );
        assert_eq!(
            envelope(RiskState::Critical),
            ChangeEnvelope {
                max_files: 3,
                max_lines: 100,
                requires_validation_after_each_stage: true,
                blocks_large_unverified_changes: true,
            }
        );
    }

    #[test]
    fn prevent_blocks_large_changes_only_where_constitutional() {
        let big = ChangeRequest::Patch {
            files: 40,
            lines: 4_000,
        };
        // Stable work is trusted with breadth.
        assert_eq!(
            adjudicate(&big, &envelope(RiskState::Stable)),
            ChangeVerdict::Admitted
        );
        // Fragile refuses, naming the files dimension first.
        assert_eq!(
            adjudicate(&big, &envelope(RiskState::Fragile)),
            refused("files_exceed_envelope")
        );
        // A files-small, lines-big patch names the line dimension.
        let long = ChangeRequest::Patch {
            files: 5,
            lines: 4_000,
        };
        assert_eq!(
            adjudicate(&long, &envelope(RiskState::Fragile)),
            refused("lines_exceed_envelope")
        );
        // Critical refuses on files first.
        assert_eq!(
            adjudicate(&big, &envelope(RiskState::Critical)),
            refused("files_exceed_envelope")
        );
        // A small patch under critical still stages (validation required).
        let small = ChangeRequest::Patch {
            files: 2,
            lines: 50,
        };
        assert_eq!(
            adjudicate(&small, &envelope(RiskState::Critical)),
            ChangeVerdict::SplitIntoStages { stages: 1 }
        );
    }

    #[test]
    fn staged_patches_must_fit_the_per_stage_allowance() {
        let fits = ChangeRequest::StagedPatch {
            stages: 4,
            files_per_stage: 3,
            lines_per_stage: 100,
        };
        assert_eq!(
            adjudicate(&fits, &envelope(RiskState::Critical)),
            ChangeVerdict::Admitted
        );
        let over = ChangeRequest::StagedPatch {
            stages: 4,
            files_per_stage: 5,
            lines_per_stage: 100,
        };
        assert_eq!(
            adjudicate(&over, &envelope(RiskState::Critical)),
            refused("stage_exceeds_envelope")
        );
        let empty = ChangeRequest::StagedPatch {
            stages: 0,
            files_per_stage: 0,
            lines_per_stage: 0,
        };
        assert_eq!(
            adjudicate(&empty, &envelope(RiskState::Fragile)),
            refused("staged_patch_declares_no_stages")
        );
    }

    #[test]
    fn drive_stages_are_the_ceiling_minimum() {
        assert_eq!(drive_stages(10, 400, 3, 100), Some(4));
        assert_eq!(drive_stages(9, 300, 3, 100), Some(3));
        assert_eq!(drive_stages(0, 0, 3, 100), Some(1));
        // Lines bind when files do not: 1 file, 400 lines, 100 per stage.
        assert_eq!(drive_stages(1, 400, 3, 100), Some(4));
        // A patch that touches no files cannot be staged honestly.
        assert_eq!(drive_stages(0, 400, 3, 100), None);
    }

    #[test]
    fn doctrines_validate_and_declare_counters_the_matrix_admits() {
        let critical_context = context(RiskBand::Critical, 900_000, 0, false);
        let prevent = directive(RiskState::Critical, &critical_context);
        validated(&prevent).unwrap();
        assert_eq!(prevent.name, "prevent");
        assert!(prevent.expected_effect.protection_delta_ppm >= 800);
        assert_eq!(prevent.counter_tactic, None);

        let fragile_context = context(RiskBand::Elevated, 900_000, 0, false);
        let drive = directive(RiskState::Fragile, &fragile_context);
        validated(&drive).unwrap();
        assert_eq!(drive.name, "make_them_drive");
        let counter = drive
            .counter_tactic
            .as_deref()
            .expect("drive names a counter");
        assert!(
            !matches!(
                compatibility("make_them_drive", counter),
                crate::composition::Compatibility::Conflicting
                    | crate::composition::Compatibility::Invalid
            ),
            "a doctrine's counter must compose with it"
        );
    }

    #[test]
    fn the_budget_boundary_is_inclusive_at_one_hundred_thousand_ppm() {
        // Exactly at the boundary is already critical: the comparison is
        // at-most, not strictly below.
        assert_eq!(
            classify(&context(RiskBand::Low, 100_000, 0, false)),
            RiskState::Critical
        );
        // One ppm above it, with no other fragile reading, is not.
        assert_eq!(
            classify(&context(RiskBand::Low, 100_001, 0, false)),
            RiskState::Stable
        );
    }

    #[test]
    fn files_are_named_before_lines_when_both_dimensions_overflow() {
        // Both dimensions exceed the fragile envelope; the files dimension
        // is the reason named.
        let over_both = ChangeRequest::Patch {
            files: 11,
            lines: 801,
        };
        assert_eq!(
            adjudicate(&over_both, &envelope(RiskState::Fragile)),
            refused("files_exceed_envelope")
        );
    }

    #[test]
    fn the_binding_dimension_is_the_larger_ceiling_division() {
        // Files bind: 10 files at 3 per stage is 4 stages; 50 lines fit in
        // one.
        assert_eq!(drive_stages(10, 50, 3, 100), Some(4));
        // Lines bind: 400 lines at 100 per stage is 4 stages; 1 file fits
        // in one.
        assert_eq!(drive_stages(1, 400, 3, 100), Some(4));
    }

    #[test]
    fn prevent_omits_its_counter_because_the_matrix_conflicts_explosives() {
        // Prevent declares no counter, and the omission is principled: its
        // natural counter would be an explosive doctrine, and the matrix
        // refuses that pair outright.
        let critical_context = context(RiskBand::Critical, 900_000, 0, false);
        let prevent = directive(RiskState::Critical, &critical_context);
        assert_eq!(prevent.counter_tactic, None);
        assert!(matches!(
            compatibility("prevent", "verticals"),
            crate::composition::Compatibility::Conflicting
                | crate::composition::Compatibility::Invalid
        ));
        // Make Them Drive declares quick_game, and the pair composes.
        let fragile_context = context(RiskBand::Elevated, 900_000, 0, false);
        let drive = directive(RiskState::Fragile, &fragile_context);
        assert_eq!(drive.counter_tactic.as_deref(), Some("quick_game"));
        assert!(matches!(
            compatibility("make_them_drive", "quick_game"),
            crate::composition::Compatibility::Allowed
                | crate::composition::Compatibility::Conditional
        ));
    }

    #[test]
    fn both_doctrines_admit_on_the_phases_the_tactical_game_drives() {
        // Phase six of the acceptance game: a critical band with 80_000 ppm
        // of budget left admits prevent.
        let phase_six = context(RiskBand::Critical, 80_000, 100, false);
        let prevent = directive(RiskState::Critical, &phase_six);
        assert_eq!(prevent.name, "prevent");
        assert!(matches!(
            prevent.evaluate_admission(&phase_six, 100),
            crate::admission::Admission::Admitted
        ));
        // An elevated phase admits make_them_drive.
        let elevated = context(RiskBand::Elevated, 900_000, 100, false);
        let drive = directive(RiskState::Fragile, &elevated);
        assert_eq!(drive.name, "make_them_drive");
        assert!(matches!(
            drive.evaluate_admission(&elevated, 100),
            crate::admission::Admission::Admitted
        ));
    }
}
