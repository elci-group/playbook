//! The film-room refusal panel (§36) and recovery-chain view (§37).
//! Pure functions from a `ResolutionSnapshot` (or a `ResolutionLineage`)
//! to a fixed-width text panel in the house style. Nothing here
//! interprets: the mandatory `Acceptance standard changed: NO` line is
//! rendered straight from the snapshot's invariant, never decided here.

use playbook_presentation::ResolutionSnapshot;
use playbook_resolution::lineage::{AttemptNode, ResolutionLineage};

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

/// The refusal-under-review panel: FLAG ON PLAY header, the diagnosed
/// cause, the review rationale, the authorised adjustments, the
/// escalation marker when escalated, and the mandatory acceptance
/// line — `Acceptance standard changed: NO` unless an explicit policy
/// revision hash was recorded on the snapshot itself.
pub fn render_refusal(snapshot: &ResolutionSnapshot, width: usize) -> String {
    let mut out = Vec::new();
    out.push(box_top("FLAG ON PLAY", width));
    out.push(box_row(
        &format!(
            "CAUSE: {} · family {} · confidence {} · {}",
            snapshot.refusal_key(),
            snapshot.family_key(),
            snapshot.confidence_key(),
            if snapshot.deterministic() {
                "deterministic"
            } else {
                "inferred"
            }
        ),
        width,
    ));
    out.push(box_row(&"─".repeat(width.saturating_sub(3)), width));
    out.push(box_row("REVIEW", width));
    for line in snapshot.rationale().lines() {
        out.push(box_row(&fit(line, width.saturating_sub(4)), width));
    }
    out.push(box_row(&"─".repeat(width.saturating_sub(3)), width));
    out.push(box_row("ADJUSTMENT", width));
    if snapshot.actions().is_empty() {
        out.push(box_row("  no authorised adjustment", width));
    }
    for action in snapshot.actions() {
        out.push(box_row(&format!("  {action}"), width));
    }
    out.push(box_row(&"─".repeat(width.saturating_sub(3)), width));
    if snapshot.escalated() {
        out.push(box_row("ESCALATED: no authorised autonomous repair", width));
    }
    // The mandatory line. The snapshot type cannot carry `true` here
    // except through the recorded policy revision (§36).
    if let Some(hash) = snapshot.policy_hash() {
        out.push(box_row(
            &format!("Acceptance standard changed: YES (policy {hash})"),
            width,
        ));
    } else {
        out.push(box_row("Acceptance standard changed: NO", width));
    }
    out.push(box_bottom(width));
    out.join("\n")
}

/// The recovery chain (§37): attempt → refusal → resolution → child,
/// one row per hop of the lineage, with the loop-guard depth.
pub fn render_lineage(lineage: &ResolutionLineage, width: usize) -> String {
    let mut out = Vec::new();
    out.push(box_top("RECOVERY CHAIN", width));
    let attempts: &[AttemptNode] = lineage.attempts();
    if attempts.is_empty() {
        out.push(box_row("no attempts recorded", width));
    }
    for node in attempts {
        let mut row = format!("ATTEMPT {}", node.attempt_id);
        if let Some(digest) = &node.refusal_digest {
            let shown: String = digest.chars().take(8).collect();
            row.push_str(&format!(" → refusal {shown}"));
        }
        if let Some(resolution) = &node.resolution {
            row.push_str(&format!(" → {resolution}"));
        }
        if let Some(parent) = &node.parent {
            row.push_str(&format!(" ← child of {parent}"));
        }
        out.push(box_row(&fit(&row, width.saturating_sub(4)), width));
    }
    out.push(box_row(&"─".repeat(width.saturating_sub(3)), width));
    out.push(box_row(&format!("DEPTH: {}", lineage.depth()), width));
    out.push(box_bottom(width));
    out.join("\n")
}
