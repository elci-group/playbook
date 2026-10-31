//! The possession heat map: style of work, not token consumption. Recent
//! plays are placed on four action phases — defensive, possession,
//! progressive, clinical — by their vocabulary kind. Deterministic.

use playbook_presentation::PresentationSnapshot;

/// The four action phases, cold to hot left to right.
pub const PHASES: [&str; 4] = ["DEFENSIVE", "POSSESSION", "PROGRESSIVE", "CLINICAL"];

fn phase_of(kind: &str) -> usize {
    if kind.contains("blitz") || kind.contains("tactic_rejected") || kind.contains("tool_failed") {
        0
    } else if kind.contains("huddle") || kind.contains("substitut") || kind.contains("punt") {
        1
    } else if kind.contains("tactic") || kind.contains("gate") || kind.contains("evidence") {
        2
    } else if kind.contains("kick") || kind.contains("touchdown") || kind.contains("field_goal") {
        3
    } else {
        2
    }
}

/// The heat map block for the current drive.
pub fn render_heatmap(_snapshot: &PresentationSnapshot, recent_kinds: &[&str]) -> String {
    let mut columns = [0u32; 4];
    for kind in recent_kinds {
        columns[phase_of(kind)] += 1;
    }
    let peak = columns.iter().copied().max().unwrap_or(0).max(1);
    let rows = 4;
    let mut out = vec![String::new(); rows];
    for (row, line_out) in out.iter_mut().enumerate() {
        let threshold = peak * (rows - row) as u32 / rows as u32;
        let line: String = columns
            .iter()
            .map(|count| {
                if *count >= threshold && *count > 0 {
                    if row == rows - 1 {
                        '●'
                    } else {
                        '◉'
                    }
                } else {
                    '·'
                }
            })
            .fold(String::new(), |mut acc, ch| {
                acc.push(' ');
                acc.push(ch);
                acc.push_str("        ");
                acc
            });
        let label = if row == rows - 1 {
            format!("{:12}", PHASES[0])
        } else {
            " ".repeat(12)
        };
        *line_out = format!("{label}{}", line.trim_end());
    }
    let mut block = out;
    block.push(" ".repeat(12));
    block.push(PHASES.iter().fold(" ".repeat(12), |mut acc, phase| {
        acc.push_str(phase);
        acc.push(' ');
        acc
    }));
    block.join("\n")
}
