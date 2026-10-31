//! The `PresentationSnapshot`: a layout-independent projection of one
//! authoritative `DriveState` into the objects the broadcast and manager
//! surfaces render. Football semantics are presentation metadata over
//! machine state — nothing here interprets, it maps.

use serde::{Deserialize, Serialize};

use playbook_core::{Disposition, Down, DriveStatus, EvidenceClass, PersonnelKind};
use playbook_state::DriveState;

/// The persistent scorebug: the one line that is always on screen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreBug {
    pub drive_id: String,
    pub down_label: String,
    /// Yards-to-goal, in milli-yards.
    pub ytg_milli: u32,
    /// Yards gained this cycle: previous YTG − current YTG, in milli-yards.
    pub delta_milli: i64,
    /// Drive completion, 0..=1_000_000 ppm.
    pub completion_ppm: u32,
    /// Monetary spend so far, in micros.
    pub spend_micros: u64,
    /// Wall-clock consumed, in milliseconds (the broadcast clock counts up).
    pub elapsed_ms: u64,
    pub status: &'static str,
    pub disposition: Option<&'static str>,
}

/// One validation boundary on the projected field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentMark {
    pub end_yard: u32,
    /// `pending` | `passed` | `failed`.
    pub state: &'static str,
}

/// The semantic field: geometry from the gate graph, not yardage guesses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldProjection {
    /// Declared field length from the Opposition Kicker (0 before kickoff).
    pub field_yards: u32,
    pub banked_yards: u32,
    pub unvalidated_yards: u32,
    pub red_zone: bool,
    pub field_goal_range: bool,
    pub segments: Vec<SegmentMark>,
    pub touchdown: bool,
    pub kick_outcome: Option<&'static str>,
}

/// One roster row: measured record and role, never a self-report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerRow {
    pub id: String,
    pub display_name: String,
    pub kind: &'static str,
    /// `active` | `assessed` | `benched`.
    pub role: &'static str,
    pub quota_ppm: u32,
    pub reliability_ppm: u32,
    pub cost_tokens: u64,
}

/// One admitted tactical play on the call sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayRow {
    pub name: String,
    pub kind: String,
    pub at_ms: u64,
}

/// One gate on the game-plan board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateRow {
    pub id: String,
    pub mandatory: bool,
    /// Gate state label, lower snake case.
    pub state: &'static str,
    pub satisfaction_ppm: u32,
    pub confidence_ppm: u32,
}

/// The Shadow Defence comparison: the best credible line-up Playbook did
/// not field, frozen at decision time. Built by the caller from
/// `playbook_field::shadow` — the snapshot only carries the verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowCompare {
    pub starters: String,
    pub shadow: String,
    pub starters_epa_deci: i64,
    pub shadow_epa_deci: i64,
    /// Signed opportunity-cost delta in deci-units; positive favours the
    /// starters.
    pub value_delta_deci: i64,
    pub actual_on_frontier: bool,
}

/// The live Blitz overlay: parallel runners and the verified winner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlitzOverlay {
    pub id: String,
    pub phase: String,
    pub workers: Vec<String>,
    pub candidates: u32,
    pub winner: Option<String>,
    pub waves: u16,
}

/// The live Huddle strip: coordination topology, not a field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleStrip {
    pub id: String,
    pub phase: String,
    pub participants: u32,
}

/// One bench substitution on the medical/board record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionRow {
    pub out: String,
    pub replacement: String,
    pub at_ms: u64,
}

/// The full projection of one drive state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationSnapshot {
    pub scorebug: ScoreBug,
    pub field: FieldProjection,
    pub active_player: Option<PlayerRow>,
    pub roster: Vec<PlayerRow>,
    pub plays: Vec<PlayRow>,
    pub gates: Vec<GateRow>,
    pub evidence_total: u32,
    pub evidence_classes: Vec<(String, u32)>,
    pub substitutions: Vec<SubstitutionRow>,
    pub shadow: Option<ShadowCompare>,
    pub blitz: Option<BlitzOverlay>,
    pub huddles: Vec<HuddleStrip>,
    pub lastdown_recommended: Option<&'static str>,
}

pub fn status_label(status: DriveStatus) -> &'static str {
    match status {
        DriveStatus::Active => "live",
        DriveStatus::Punted => "punted",
        DriveStatus::Completed => "final",
        DriveStatus::Abandoned => "abandoned",
        DriveStatus::FieldGoalScored => "field_goal_good",
    }
}

pub fn disposition_label(disposition: Disposition) -> &'static str {
    match disposition {
        Disposition::Convert => "convert",
        Disposition::FieldGoal => "field_goal",
        Disposition::Punt => "punt",
        Disposition::Reset => "reset",
        Disposition::Abandon => "abandon",
    }
}

pub fn kind_label(kind: PersonnelKind) -> &'static str {
    match kind {
        PersonnelKind::Model => "model",
        PersonnelKind::Tool => "tool",
        PersonnelKind::Toolchain => "toolchain",
        PersonnelKind::SpecialistAgent => "specialist",
        PersonnelKind::DeterministicProcedure => "procedure",
        PersonnelKind::HumanEscalation => "human",
    }
}

pub fn down_label(down: Down) -> String {
    down.presentation().to_string()
}

fn class_label(class: &EvidenceClass) -> String {
    format!("{class:?}")
        .chars()
        .enumerate()
        .fold(String::new(), |mut acc, (index, ch)| {
            if ch.is_uppercase() && index > 0 {
                acc.push('_');
            }
            acc.extend(ch.to_lowercase());
            acc
        })
}

impl PresentationSnapshot {
    /// Project one authoritative drive state. Pure: the same state always
    /// yields the same snapshot, so `Audit` and `Resimulate` replays of the
    /// same journal project identically.
    pub fn from_state(state: &DriveState) -> Self {
        let previous = i64::from(state.cycle_previous_ytg());
        let ytg = i64::from(state.ytg_milli());
        let consumed = state.budget_consumed();
        let scorebug = ScoreBug {
            drive_id: state.drive_id().as_str().to_string(),
            down_label: down_label(state.down()),
            ytg_milli: state.ytg_milli(),
            delta_milli: previous - ytg,
            completion_ppm: state.completion_ppm(),
            spend_micros: consumed.monetary_cost_micros,
            elapsed_ms: consumed.wall_clock_ms,
            status: status_label(state.status()),
            disposition: state.disposition().map(disposition_label),
        };
        let field_state = state.field();
        let segments = field_state
            .segments
            .records()
            .iter()
            .map(|record| SegmentMark {
                end_yard: record.end_yard,
                state: match record.state {
                    playbook_field::domain::BoundaryState::Pending => "pending",
                    playbook_field::domain::BoundaryState::Passed => "passed",
                    playbook_field::domain::BoundaryState::Failed => "failed",
                },
            })
            .collect();
        let field = FieldProjection {
            field_yards: field_state.field_yards,
            banked_yards: field_state.position.banked,
            unvalidated_yards: field_state.position.unvalidated,
            red_zone: field_state.red_zone_entered,
            field_goal_range: field_state.field_goal_range_entered,
            segments,
            touchdown: field_state.touchdown,
            kick_outcome: field_state.kick_outcome.map(|outcome| match outcome {
                playbook_field::domain::KickOutcome::Score => "score",
                playbook_field::domain::KickOutcome::CleanMiss => "clean_miss",
                playbook_field::domain::KickOutcome::Blocked => "blocked",
                playbook_field::domain::KickOutcome::Turnover => "turnover",
            }),
        };
        let mut roster: Vec<PlayerRow> = Vec::new();
        if let Some(active) = state.personnel() {
            roster.push(player_row(active, "active"));
        }
        for assessed in state.assessments() {
            if state.personnel().map(|p| p.id.clone()).as_deref()
                != Some(assessed.id.as_str())
            {
                roster.push(player_row(assessed, "assessed"));
            }
        }
        for benched in state.benched() {
            roster.push(PlayerRow {
                id: benched.as_str().to_string(),
                display_name: benched.as_str().to_string(),
                kind: "bench",
                role: "benched",
                quota_ppm: 0,
                reliability_ppm: 0,
                cost_tokens: 0,
            });
        }
        let plays = state
            .tactics()
            .history()
            .iter()
            .map(|record| PlayRow {
                name: record.tactic.clone(),
                kind: format!("{:?}", record.kind),
                at_ms: record.at_ms,
            })
            .collect();
        let gates = state
            .gates()
            .iter()
            .map(|gate| GateRow {
                id: gate.id.as_str().to_string(),
                mandatory: gate.mandatory,
                state: match gate.state {
                    playbook_core::GateState::Unassessed => "unassessed",
                    playbook_core::GateState::Blocked => "blocked",
                    playbook_core::GateState::InProgress => "in_progress",
                    playbook_core::GateState::Satisfied => "satisfied",
                    playbook_core::GateState::Failed => "failed",
                    playbook_core::GateState::Waived => "waived",
                },
                satisfaction_ppm: gate.satisfaction_ppm,
                confidence_ppm: gate.confidence_ppm,
            })
            .collect();
        let mut classes: Vec<(String, u32)> = Vec::new();
        for evidence in state.evidence() {
            let label = class_label(&evidence.class);
            if let Some(entry) = classes.iter_mut().find(|(name, _)| *name == label) {
                entry.1 += 1;
            } else {
                classes.push((label, 1));
            }
        }
        classes.sort();
        let blitz = state.blitzes().active_blitz().map(|blitz| BlitzOverlay {
            id: blitz.id.as_str().to_string(),
            phase: format!("{:?}", blitz.phase),
            workers: blitz
                .workers
                .iter()
                .map(|worker| worker.personnel_id.as_str().to_string())
                .collect(),
            candidates: blitz.candidates.len() as u32,
            winner: blitz
                .winner
                .as_ref()
                .map(|winner| winner.candidate_id.as_str().to_string()),
            waves: blitz.closed_waves,
        });
        let huddles = state
            .huddles()
            .active()
            .iter()
            .map(|huddle| HuddleStrip {
                id: huddle.id.as_str().to_string(),
                phase: format!("{:?}", huddle.phase),
                participants: huddle.participants.len() as u32,
            })
            .collect();
        let substitutions = state
            .substitutions()
            .iter()
            .map(|record| SubstitutionRow {
                out: record.out.as_str().to_string(),
                replacement: record.replacement.as_str().to_string(),
                at_ms: record.at_ms,
            })
            .collect();
        Self {
            scorebug,
            field,
            active_player: state.personnel().map(|p| player_row(p, "active")),
            roster,
            plays,
            gates,
            evidence_total: state.evidence().len() as u32,
            evidence_classes: classes,
            substitutions,
            shadow: None,
            blitz,
            huddles,
            lastdown_recommended: state.disposition().map(disposition_label),
        }
    }
}

fn player_row(personnel: &playbook_core::Personnel, role: &'static str) -> PlayerRow {
    PlayerRow {
        id: personnel.id.as_str().to_string(),
        display_name: personnel.display_name.clone(),
        kind: kind_label(personnel.kind),
        role,
        quota_ppm: personnel.quota_remaining_ppm,
        reliability_ppm: personnel.reliability_ppm,
        cost_tokens: personnel.cost_tokens,
    }
}
