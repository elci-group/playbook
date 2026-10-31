//! Frame composition. Every renderer is a pure function
//! `snapshot → String` at a fixed width: large empty regions, strong
//! typographic hierarchy, a small number of huge statistics, restrained
//! semantics. Football Manager × broadcast telemetry, not a hacker grid.

use playbook_presentation::{
    BlitzOverlay, FieldProjection, PresentationSnapshot, ScoreBug, ShadowCompare,
};

/// The default broadcast width.
pub const BROADCAST_WIDTH: usize = 80;
/// The default dugout width.
pub const MANAGER_WIDTH: usize = 78;

fn pad(line: &str, width: usize) -> String {
    let len = line.chars().count();
    if len >= width {
        line.chars().take(width).collect()
    } else {
        let mut out = line.to_string();
        out.push_str(&" ".repeat(width - len));
        out
    }
}

fn fit(line: &str, width: usize) -> String {
    line.chars().take(width).collect()
}

fn box_top(title: &str, width: usize) -> String {
    let inner = width.saturating_sub(2);
    let title_len = title.chars().count() + 2;
    if title.is_empty() {
        format!("┌{}┐", "─".repeat(inner))
    } else {
        format!(
            "┌─ {} {}┐",
            title,
            "─".repeat(inner.saturating_sub(title_len))
        )
    }
}

fn box_row(content: &str, width: usize) -> String {
    format!("│{}│", pad(content, width.saturating_sub(2)))
}

fn box_bottom(width: usize) -> String {
    format!("└{}┘", "─".repeat(width.saturating_sub(2)))
}

fn yards(milli: u32) -> String {
    format!("{:.1}", f64::from(milli) / 1000.0)
}

fn signed_yards(milli: i64) -> String {
    let value = milli as f64 / 1000.0;
    format!("{value:+.1}")
}

/// The scorebug: the one persistent line — what is happening right now.
pub fn render_scorebug(bug: &ScoreBug, width: usize) -> String {
    let clock = format!(
        "{:02}:{:02}",
        bug.elapsed_ms / 60_000,
        (bug.elapsed_ms / 1_000) % 60
    );
    let dot = if bug.status == "live" { "●" } else { "○" };
    let core = format!(
        " PLAYBOOK LIVE │ {} │ {} │ YTG {} │ Δ{} │ {}m¢ │ {} │ {}",
        bug.drive_id,
        bug.down_label.to_uppercase(),
        yards(bug.ytg_milli),
        signed_yards(bug.delta_milli),
        bug.spend_micros / 1_000,
        clock,
        dot
    );
    format!(
        "┌{}┐\n│{}│\n└{}┘",
        "─".repeat(width - 2),
        pad(&fit(&core, width - 2), width - 2),
        "─".repeat(width - 2)
    )
}

/// The projected field: the dominant visual object. Banked yards are
/// solid, unvalidated yards are hollow, boundaries are marked, the
/// semantic zones are named.
pub fn render_field(field: &FieldProjection, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let total = field.field_yards.max(1);
    // One column per 2 yards across the available rail width.
    let rail = width.saturating_sub(16);
    let columns = rail.min(total.max(10) as usize);
    let scale = |yard: u32| -> usize {
        ((f64::from(yard) / f64::from(total)) * (columns.saturating_sub(1)) as f64).round() as usize
    };
    let mut rail_line: Vec<char> = vec!['·'; columns];
    for (index, segment) in field.segments.iter().enumerate() {
        let at = scale(segment.end_yard).min(columns - 1);
        rail_line[at] = match segment.state {
            "passed" => '▓',
            "failed" => '✕',
            _ => '│',
        };
        let _ = index;
    }
    let position = scale(field.banked_yards + field.unvalidated_yards).min(columns - 1);
    rail_line[position] = if field.unvalidated_yards > 0 {
        '○'
    } else {
        '●'
    };
    lines.push(format!(
        "OWN {}│{}│ GOAL",
        " ".repeat(4),
        rail_line.iter().collect::<String>()
    ));
    let mut zones = String::new();
    if field.red_zone {
        zones.push_str("RED ZONE ");
    }
    if field.field_goal_range {
        zones.push_str("FIELD-GOAL RANGE ");
    }
    if field.touchdown {
        zones.push_str("TOUCHDOWN ");
    }
    if let Some(outcome) = field.kick_outcome {
        zones.push_str(&format!("KICK:{outcome} "));
    }
    lines.push(fit(&zones, width));
    lines
}

/// Manager Mode — the dugout.
pub fn render_manager(snapshot: &PresentationSnapshot, width: usize) -> String {
    let bug = &snapshot.scorebug;
    let mut out = Vec::new();
    out.push(box_top("PLAYBOOK", width));
    out.push(box_row(
        &format!(
            "{}  {:<14} {} → {}   CLOCK {:02}:{:02}",
            bug.drive_id,
            bug.down_label.to_uppercase(),
            if bug.status == "live" {
                "LIVE".to_string()
            } else {
                bug.status.to_uppercase()
            },
            if snapshot.field.red_zone {
                "RED ZONE"
            } else {
                "OPEN FIELD"
            },
            bug.elapsed_ms / 60_000,
            (bug.elapsed_ms / 1_000) % 60
        ),
        width,
    ));
    out.push(box_row(&"─".repeat(width.saturating_sub(3)), width));
    // Left pane: play call and squad; right pane: field.
    let field_lines = render_field(&snapshot.field, width / 2);
    let formation = snapshot
        .plays
        .last()
        .map(|play| play.name.to_uppercase())
        .unwrap_or_else(|| "NO PLAY CALLED".into());
    out.push(box_row(
        &format!("FORMATION            │ {}", field_lines[0]),
        width,
    ));
    out.push(box_row(
        &format!(
            "                     │ {}",
            " ".repeat(field_lines[0].chars().count())
        ),
        width,
    ));
    out.push(box_row(
        &format!(
            "{:<20}│ {}",
            fit(&formation, 19),
            field_lines.get(1).cloned().unwrap_or_default()
        ),
        width,
    ));
    out.push(box_row(
        &format!(
            "MOTION: NONE         │ YTG {:<6} Δ{:<7} CONF {}",
            yards(bug.ytg_milli),
            signed_yards(bug.delta_milli),
            format!("{:.2}", f64::from(bug.completion_ppm) / 1_000_000.0).as_str()
        ),
        width,
    ));
    out.push(box_row(&"─".repeat(width.saturating_sub(3)), width));
    // Roster.
    out.push(box_row("ROSTER", width));
    for player in snapshot.roster.iter().take(4) {
        out.push(box_row(
            &format!(
                "{:<4} {:<14} {:<9} {}",
                fit(&player.id, 3),
                fit(&player.display_name, 13),
                player.kind.to_uppercase(),
                player.role.to_uppercase()
            ),
            width,
        ));
    }
    // Play call with expected economics when available.
    if let Some(play) = snapshot.plays.last() {
        out.push(box_row(
            &format!(
                "PLAY CALL: {} / {}",
                play.name.to_uppercase(),
                play.kind.to_uppercase()
            ),
            width,
        ));
    }
    if let Some(shadow) = &snapshot.shadow {
        out.push(box_row(&render_shadow_line(shadow), width));
    }
    out.push(box_row(&"─".repeat(width.saturating_sub(3)), width));
    out.push(box_row(
        "[P] PLAY CALL   [A] AUDIBLE   [H] HUDDLE   [B] BLITZ   [F] FIELD GOAL",
        width,
    ));
    out.push(box_bottom(width));
    out.join("\n")
}

fn render_shadow_line(shadow: &ShadowCompare) -> String {
    format!(
        "SHADOW DEFENCE: {} vs {} Δ{:+.1} frontier: {}",
        fit(&shadow.starters, 16),
        fit(&shadow.shadow, 16),
        shadow.value_delta_deci as f64 / 10.0,
        if shadow.actual_on_frontier {
            "yes"
        } else {
            "NO"
        }
    )
}

/// Broadcast Mode — what is happening, why, and is the drive winning.
pub fn render_broadcast(snapshot: &PresentationSnapshot, width: usize) -> String {
    let mut out = Vec::new();
    out.push("              PLAYBOOK LIVE".to_string());
    out.push(render_scorebug(&snapshot.scorebug, width));
    out.push(String::new());
    for line in render_field(&snapshot.field, width) {
        out.push(line);
    }
    out.push(String::new());
    let bar_columns = width.saturating_sub(10);
    let filled =
        ((f64::from(snapshot.scorebug.completion_ppm) / 1_000_000.0) * bar_columns as f64) as usize;
    out.push(format!(
        "        {}{}  {:>3}%",
        "█".repeat(filled),
        "░".repeat(bar_columns - filled),
        snapshot.scorebug.completion_ppm / 10_000
    ));
    out.push(String::new());
    out.push(format!(
        "     EVIDENCE       {} sealed ({} classes)",
        snapshot.evidence_total,
        snapshot.evidence_classes.len()
    ));
    out.push(format!(
        "     BANKED         {} yd validated / {} unvalidated",
        snapshot.field.banked_yards, snapshot.field.unvalidated_yards
    ));
    if let Some(blitz) = &snapshot.blitz {
        out.push(String::new());
        for line in render_blitz_overlay(blitz).lines() {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

/// The Blitz overlay: parallel runners, losing branches killed on the
/// verified winner.
pub fn render_blitz_overlay(blitz: &BlitzOverlay) -> String {
    let width = 46;
    let mut out = Vec::new();
    out.push(box_top("BLITZ", width));
    out.push(box_row(
        &format!("PRESSURE PHASE: {}", blitz.phase.to_uppercase()),
        width,
    ));
    out.push(box_row(
        &format!("{} WORKERS DEPLOYED", blitz.workers.len()),
        width,
    ));
    let done = (blitz.candidates as usize).min(blitz.workers.len());
    for (index, worker) in blitz.workers.iter().enumerate() {
        let bars = usize::from(index < done) * 10;
        let state = if blitz.winner.is_some() && index == 0 {
            "verified"
        } else if index < done {
            "running"
        } else {
            "racing"
        };
        out.push(box_row(
            &format!(
                "{:<10} {}{}{}  {}",
                fit(worker, 9),
                "█".repeat(bars),
                "░".repeat(10 - bars),
                if bars > 0 { " " } else { "  " },
                state
            ),
            width,
        ));
    }
    if let Some(winner) = &blitz.winner {
        out.push(box_row(&format!("WINNER: {winner}"), width));
    }
    out.push(box_bottom(width));
    out.join("\n")
}

/// The kick decision moment: range by utility dominance, not yardage.
pub fn render_kick_decision(field: &FieldProjection, width: usize) -> String {
    let mut out = Vec::new();
    out.push(box_top("FIELD GOAL RANGE", width));
    if field.field_goal_range {
        out.push(box_row(
            "U_kick > U_drive — one bounded action dominates.",
            width,
        ));
    } else {
        out.push(box_row("Out of range: continue the drive.", width));
    }
    if let Some(outcome) = field.kick_outcome {
        out.push(box_row(&format!("OUTCOME: {outcome}"), width));
    }
    out.push(box_row("[F] ATTEMPT", width));
    out.push(box_bottom(width));
    out.join("\n")
}

/// The Lastdown decision moment: a televised call.
pub fn render_lastdown(chosen: &str, width: usize) -> String {
    let mut out = Vec::new();
    out.push(box_top("4TH DOWN", width));
    for option in ["convert", "field_goal", "punt", "reset", "abandon"] {
        let marker = if option == chosen { " ← CHOSEN" } else { "" };
        out.push(box_row(&format!("{option}{marker}"), width));
    }
    out.push(box_bottom(width));
    out.join("\n")
}

/// The progressive-disclosure depth switch: glance → tactical → forensic.
pub fn render_player_depth(rows: &[playbook_presentation::PlayerRow]) -> String {
    let mut out = Vec::new();
    for player in rows {
        out.push(format!(
            "{} • {} • {} • {}",
            player.display_name.to_uppercase(),
            player.kind.to_uppercase(),
            player.role.to_uppercase(),
            player.reliability_ppm / 10_000
        ));
        out.push(format!(
            "  quota {}%  cost {} tok",
            player.quota_ppm / 10_000,
            player.cost_tokens
        ));
    }
    out.join("\n")
}

/// Render one of the three forms (the depth switch: glance → tactical →
/// forensic), with the full three-form stack available for the frame.
pub fn render_three(
    index: usize,
    kind: &str,
    forms: &playbook_presentation::ThreeForm,
    form: FormKey,
) -> String {
    let mut out = Vec::new();
    out.push(format!("FRAME {:04}  {}", index, kind.to_uppercase()));
    match form {
        FormKey::Glance => {
            out.push(forms.glance.clone());
        }
        FormKey::Tactical => {
            out.push(forms.glance.clone());
            out.push(forms.tactical.clone());
        }
        FormKey::Analytical => {
            out.push(forms.glance.clone());
            out.push(forms.tactical.clone());
            for (label, value) in &forms.analytical {
                out.push(format!(
                    "{:<18} {}",
                    format!("{label}:").to_uppercase(),
                    value
                ));
            }
        }
    }
    out.push("(ENTER next · p back · f form · q quit)".into());
    out.join("\n")
}

/// The form selector for the depth switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormKey {
    Glance,
    Tactical,
    Analytical,
}

impl FormKey {
    pub fn next(self) -> Self {
        match self {
            Self::Glance => Self::Tactical,
            Self::Tactical => Self::Analytical,
            Self::Analytical => Self::Glance,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "glance" => Some(Self::Glance),
            "tactical" => Some(Self::Tactical),
            "analytical" => Some(Self::Analytical),
            _ => None,
        }
    }
}
