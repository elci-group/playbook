//! The 3form presentation protocol: every important Playbook noun or
//! event has a **glance** form (one line), a **tactical** form (spatial),
//! and an **analytical** form (forensic rows). The forms are projections
//! of the same event — they cannot disagree because they cannot interpret.

use serde::{Deserialize, Serialize};

use playbook_events::DriveEvent;

/// The three presentation forms of one semantic object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreeForm {
    /// One compact line: `FIRST DOWN • +8.2 • G17 ✓`.
    pub glance: String,
    /// The spatial/tactical block: the field, the route, the topology.
    pub tactical: String,
    /// The forensic rows: `(label, value)` pairs for depth disclosure.
    pub analytical: Vec<(String, String)>,
}

/// The event's vocabulary key (stable, snake case) for filters and
/// frame navigation.
pub fn kind(event: &DriveEvent) -> &'static str {
    event.name()
}

/// Project one journal event into its three forms. Deterministic.
pub fn present_event(event: &DriveEvent) -> ThreeForm {
    match event {
        DriveEvent::DriveStarted { drive_id, task, .. } => ThreeForm {
            glance: format!("KICKOFF • {}", drive_id.as_str()),
            tactical: format!("│──────────▶│  drive opens: {}", task.statement),
            analytical: vec![
                ("drive".into(), drive_id.as_str().into()),
                ("workload".into(), task.workload_class.clone()),
                (
                    "integrity".into(),
                    format!("{:?}", task.integrity),
                ),
            ],
        },
        DriveEvent::GateDeclared { gate, .. } => ThreeForm {
            glance: format!("GATE DECLARED • {}", gate.id.as_str()),
            tactical: format!("──────────● {}  (mandatory: {})", gate.id.as_str(), gate.mandatory),
            analytical: vec![
                ("gate".into(), gate.id.as_str().into()),
                ("weight_ppm".into(), gate.weight_ppm.to_string()),
                ("mandatory".into(), gate.mandatory.to_string()),
            ],
        },
        DriveEvent::EvidenceAccepted { evidence } => ThreeForm {
            glance: format!("EVIDENCE • {:?}", evidence.class),
            tactical: "      ●━━━▶  measured fact sealed".into(),
            analytical: vec![
                ("class".into(), format!("{:?}", evidence.class)),
                ("evidence".into(), evidence.id.as_str().into()),
            ],
        },
        DriveEvent::YtgRecalculated { ytg_milli, .. } => ThreeForm {
            glance: format!("YTG • {:.1}", f64::from(*ytg_milli) / 1000.0),
            tactical: format!("──────────●━━━▶│  line of scrimmage {:.1}", f64::from(*ytg_milli) / 1000.0),
            analytical: vec![("ytg_milli".into(), ytg_milli.to_string())],
        },
        DriveEvent::DownChanged { down, .. } => ThreeForm {
            glance: format!("DOWN • {}", down.presentation()),
            tactical: format!("◈ {}", down.presentation()),
            analytical: vec![("down".into(), down.machine_id().into())],
        },
        DriveEvent::ValidationBoundaryPassed { yard, .. } => ThreeForm {
            glance: format!("FIRST DOWN • +{yard} • boundary ✓"),
            tactical: format!("────────────●━━━━━━━━━━━━▶│\n           {yard}"),
            analytical: vec![("banked_yard".into(), yard.to_string())],
        },
        DriveEvent::ValidationBoundaryFailed { yard, reason } => ThreeForm {
            glance: format!("TURNOVER ON DOWNS • yard {yard}"),
            tactical: format!("────────────✕···········│\n           {yard}  gates reopened"),
            analytical: vec![
                ("yard".into(), yard.to_string()),
                ("reason".into(), reason.clone()),
            ],
        },
        DriveEvent::ProgressBanked { banked_yards_total, .. } => ThreeForm {
            glance: format!("BANKED • {banked_yards_total} yd"),
            tactical: format!("██████████│  {banked_yards_total} validated yards"),
            analytical: vec![("banked_total".into(), banked_yards_total.to_string())],
        },
        DriveEvent::RedZoneEntered { .. } => ThreeForm {
            glance: "RED ZONE • closure feasible".into(),
            tactical: "╔═ RED ZONE ═════════════╗\n║  every mandatory gate  ║\n║  assessable closable   ║\n╚════════════════════════╝".into(),
            analytical: vec![("red_zone".into(), "true".into())],
        },
        DriveEvent::FieldGoalRangeEntered { candidate_action } => ThreeForm {
            glance: "FIELD GOAL RANGE • U_kick > U_drive".into(),
            tactical: format!("┌ FIELD GOAL RANGE ┐\n│ {candidate_action} │\n└──────────────────┘"),
            analytical: vec![("candidate_action".into(), candidate_action.clone())],
        },
        DriveEvent::KickScored { personnel, .. } => ThreeForm {
            glance: "FIELD GOAL • GOOD".into(),
            tactical: "──────────────────●━━━▶◯\n                   splits the uprights".into(),
            analytical: vec![("kicker".into(), personnel.clone())],
        },
        DriveEvent::KickMissed { personnel, .. } => ThreeForm {
            glance: "FIELD GOAL • MISSED".into(),
            tactical: "──────────────────●───✕\n                   wide left".into(),
            analytical: vec![("kicker".into(), personnel.clone())],
        },
        DriveEvent::KickBlocked { personnel, reason } => ThreeForm {
            glance: "FIELD GOAL • BLOCKED".into(),
            tactical: "──────────────────●───▣\n                   line held".into(),
            analytical: vec![
                ("kicker".into(), personnel.clone()),
                ("reason".into(), reason.clone()),
            ],
        },
        DriveEvent::KickTurnover { personnel, harm } => ThreeForm {
            glance: "TURNOVER • kick harmed the field".into(),
            tactical: "──────────────────●───⚠\n                   possession lost".into(),
            analytical: vec![
                ("kicker".into(), personnel.clone()),
                ("harm".into(), harm.clone()),
            ],
        },
        DriveEvent::TouchdownScored { banked_yards_total, .. } => ThreeForm {
            glance: "TOUCHDOWN • objective crossed".into(),
            tactical: "██████████████████████████ TOUCHDOWN".into(),
            analytical: vec![
                ("touchdown".into(), "true".into()),
                ("banked_total".into(), banked_yards_total.to_string()),
            ],
        },
        DriveEvent::FieldGoalSelected { .. } => ThreeForm {
            glance: "CALL • FIELD GOAL".into(),
            tactical: "◈ coach signals: kick".into(),
            analytical: vec![("call".into(), "field_goal".into())],
        },
        DriveEvent::PuntIssued { .. } => ThreeForm {
            glance: "PUNT • possession transferred".into(),
            tactical: "◈ coach signals: punt (Lastdown)".into(),
            analytical: vec![("call".into(), "punt".into())],
        },
        DriveEvent::DriveCompleted { .. } => ThreeForm {
            glance: "FINAL • drive completed".into(),
            tactical: "██████████████████████████ FINAL".into(),
            analytical: vec![("status".into(), "completed".into())],
        },
        DriveEvent::HuddleOpened { huddle_id } => ThreeForm {
            glance: format!("HUDDLE • {}", huddle_id.as_str()),
            tactical: format!("◎──◎ huddle {}\n     │\n    ◎", huddle_id.as_str()),
            analytical: vec![("huddle".into(), huddle_id.as_str().into())],
        },
        DriveEvent::HuddleClosed { huddle_id, .. } => ThreeForm {
            glance: format!("HUDDLE BROKE • {}", huddle_id.as_str()),
            tactical: "◎    ◎    ◎  →  one call".into(),
            analytical: vec![("huddle".into(), huddle_id.as_str().into())],
        },
        DriveEvent::BlitzProposed { blitz_id, .. } => ThreeForm {
            glance: format!("BLITZ PROPOSED • {}", blitz_id.as_str()),
            tactical: "◉ → ◉ ◉ ◉ ◉  parallel runners form".into(),
            analytical: vec![("blitz".into(), blitz_id.as_str().into())],
        },
        DriveEvent::BlitzWinnerSelected { blitz_id, candidate_id, .. } => ThreeForm {
            glance: format!("BLITZ WON • {}", blitz_id.as_str()),
            tactical: "◉ ✓ ← verified winner\n◉ ✕ ◉ ✕  losing branches killed".into(),
            analytical: vec![
                ("blitz".into(), blitz_id.as_str().into()),
                ("candidate".into(), candidate_id.as_str().into()),
            ],
        },
        DriveEvent::TacticAdmitted { tactic, .. } => ThreeForm {
            glance: format!("PLAY CALLED • {tactic}"),
            tactical: format!("◈ formation holds: {tactic}"),
            analytical: vec![("tactic".into(), tactic.clone())],
        },
        DriveEvent::PlayerSubstituted { out, replacement, .. } => {
            let onto = replacement
                .as_ref()
                .map(|id| id.as_str().to_string())
                .unwrap_or_else(|| "none".into());
            ThreeForm {
                glance: format!("SUBSTITUTION • {} → {}", out.as_str(), onto),
                tactical: format!("⇄ {} off, {} on", out.as_str(), onto),
                analytical: vec![
                    ("out".into(), out.as_str().into()),
                    ("replacement".into(), onto),
                ],
            }
        }
        other => ThreeForm {
            glance: format!("• {}", other.name().to_uppercase()),
            tactical: format!("· {}", other.name()),
            analytical: vec![("event".into(), other.name().into())],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{DriveBudget, DriveId, TaskContract, TaskIntegrity};

    fn started() -> DriveEvent {
        DriveEvent::DriveStarted {
            drive_id: DriveId::new("drive-421").unwrap(),
            task: TaskContract {
                statement: "ship the release".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            budget: DriveBudget::zero(),
            at_ms: 0,
        }
    }

    #[test]
    fn the_three_forms_project_the_same_event_without_disagreement() {
        let forms = present_event(&started());
        assert_eq!(forms.glance, "KICKOFF • drive-421");
        assert!(forms.tactical.contains("ship the release"));
        assert_eq!(
            forms.analytical,
            vec![
                ("drive".to_string(), "drive-421".to_string()),
                ("workload".to_string(), "engineering".to_string()),
                ("integrity".to_string(), "Sound".to_string()),
            ]
        );
        // Deterministic: the same event projects identically.
        assert_eq!(present_event(&started()), forms);
    }

    #[test]
    fn the_kind_key_is_the_vocabulary_name() {
        assert_eq!(kind(&started()), "drive_started");
    }
}
