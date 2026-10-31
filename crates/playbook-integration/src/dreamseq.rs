//! The Dreamseq post-drive review seam: inference over a *completed* drive,
//! feeding the next Franchise planning cycle as evidence.
//!
//! Football: the film session after the whistle. The drive is over, the
//! yardage is in the book, and nobody re-spots the ball. Dreamseq receives a
//! structured review package projected FROM the journal
//! ([`review_package`]), analyses it with deterministic rules
//! ([`evaluate`]), and produces structured recommendations that enter the
//! next Franchise planning cycle as evidence.
//!
//! The constitutional line, enforced by the types: Dreamseq MAY analyse and
//! SHALL NOT reopen or rewrite the completed drive. [`review_package`]
//! consumes a shared `&[DriveEvent]` — a reference the compiler guarantees
//! cannot append, reorder, or rewrite the journal — and [`PostDriveReview`]
//! is plain data (ids, counters, vectors) with no journal handle, so the
//! module exposes no method by which a review could take authority over the
//! drive. Reviews are redacted before they leave the seam ([`redact`]):
//! no credentials, no bearer tokens, no private environment values, no
//! absolute home paths.

use serde::{Deserialize, Serialize};

use playbook_core::ToolFailure;
use playbook_events::DriveEvent;

/// The §17 minimum review payload: everything the journal carries about a
/// closed drive, projected verbatim and nothing more. Built only by
/// [`review_package`] from a shared journal slice; carries no journal
/// handle, so it cannot reach back into the drive it reviews.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostDriveReview {
    pub franchise_id: String,
    pub objective_id: String,
    pub drive_id: String,
    /// Yardage-to-go at the first yardage signal in the journal.
    pub opening_ytg_milli: u32,
    /// Yardage-to-go at the last yardage signal in the journal.
    pub closing_ytg_milli: u32,
    /// Personnel ids that took the field, in first-seen order.
    pub personnel_used: Vec<String>,
    /// Tactic names admitted, in journal order.
    pub tactics_called: Vec<String>,
    /// Per completed tactic: `(tactic, after - before)` in signed milli.
    pub tactic_ytg_deltas: Vec<(String, i64)>,
    /// Per substitution: `(out, replacement)`, with `"none"` when the
    /// journal recorded no replacement.
    pub substitutions: Vec<(String, String)>,
    /// Inference tokens charged across the drive.
    pub inference_spend_tokens: u64,
    /// Tool invocations recorded across the drive.
    pub tool_spend_calls: u64,
    /// Failure modes recorded while the drive ran, in journal order.
    pub failures: Vec<String>,
    /// CERS artefact ids recovered during the drive, in journal order.
    pub recovered_knowledge: Vec<String>,
    /// Number of evidence items accepted across the drive.
    pub evidence_count: u32,
}

/// Why a review package could not be produced or forwarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewError {
    /// The journal slice carries no terminal drive event, so the drive may
    /// still be live and must not be reviewed.
    DriveNotTerminal,
    /// A strict transport refused to forward a package that still carries a
    /// credential needle. [`review_package`] projects verbatim and
    /// [`redact`] replaces needles rather than refusing, so this variant is
    /// raised by caller-side strict policies, not by this module's own
    /// procedures.
    RedactionRefused,
}

/// The replacement string substituted for every redacted field.
const REDACTED: &str = "[REDACTED]";

/// Credential needles. A string field carrying any of these is redacted.
const CREDENTIAL_NEEDLES: [&str; 5] = [
    "api_key=",
    "bearer ",
    "password=",
    "private_key=",
    "authorization:",
];

/// An absolute home path is a private environment value: redacted.
const HOME_PATH_NEEDLE: &str = "/home/";

/// A single tactic delta above this many milli is a huge one-off gain that
/// deserves an outlier review rather than quiet acceptance.
const OUTLIER_GAIN_MILLI: i64 = 1_000_000;

/// Personnel reason code: the personnel spent drive spend without any
/// positive yardage attributable to them.
const REASON_CONSUMED_SPEND_WITHOUT_YTG: &str = "consumed_spend_without_ytg";

/// Tactical finding: the tactic lost yardage.
const FINDING_NEGATIVE_YTG: &str = "negative_ytg";

/// Tactical finding: the tactic gained more than [`OUTLIER_GAIN_MILLI`] in
/// one completed call.
const FINDING_OUTLIER_GAIN_REVIEW: &str = "outlier_gain_review";

/// Salary finding: inference tokens were spent and the yardage-to-go never
/// moved.
const FINDING_FULL_SPEND_ZERO_YTG: &str = "full_spend_zero_ytg";

/// Infrastructure pattern: at least two failures and nothing harvested into
/// CERS.
const PATTERN_REPEATED_FAILURES_NO_CERS: &str = "repeated_failures_no_cers";

/// Infrastructure suggestion attached to [`PATTERN_REPEATED_FAILURES_NO_CERS`].
const SUGGESTION_ROUTE_FAILURES_THROUGH_CERS: &str = "route failures through cers harvesting";

/// Infrastructure pattern: at least two failures, all with the identical
/// failure mode — the §20 inference displacement KPI.
const PATTERN_REPEATED_IDENTICAL_FAILURE: &str = "repeated_identical_failure";

/// Infrastructure suggestion attached to
/// [`PATTERN_REPEATED_IDENTICAL_FAILURE`].
const SUGGESTION_DETERMINISTIC_TOOLING: &str = "deterministic tooling candidate";

/// Build the review package FROM the journal. Deterministic: the same
/// events always produce the same package, and the journal slice is only
/// ever read — a review cannot append, reorder, or rewrite it.
///
/// Refuses [`ReviewError::DriveNotTerminal`] unless the events contain a
/// terminal drive event: `DriveCompleted`, `PuntIssued`, `AbandonIssued`,
/// or `FieldGoalSelected` with no later `DriveStarted`.
///
/// The package projects only what the journal carries, in journal order:
///
/// - `tactics_called`: names from `TacticAdmitted`.
/// - `tactic_ytg_deltas`: one entry per `TacticCompleted`, the delta
///   `ytg_after_milli - ytg_before_milli` as a signed `i64` (a lossless
///   `u32` widening, integer-only arithmetic).
/// - `substitutions`: one `(out, replacement)` pair per
///   `PlayerSubstituted`; a missing replacement projects as `"none"`.
/// - `personnel_used`: first-seen, deduplicated personnel ids from
///   `PersonnelChanged`, `PossessionStarted`, and the incoming replacement
///   of each `PlayerSubstituted` — the personnel that took the field.
/// - `failures`: one string per `ToolFailed`, the failure mode
///   (`ToolFailure.kind`) rendered as its lowercase snake-case name.
/// - `recovered_knowledge`: artefact ids from `CersRecovered`.
/// - `inference_spend_tokens`: saturating sum of the token fields over
///   every `BudgetConsumed` and `BlitzBudgetConsumed`.
/// - `tool_spend_calls`: count of `ToolInvoked`.
/// - `evidence_count`: count of `EvidenceAccepted`.
/// - `opening_ytg_milli` / `closing_ytg_milli`: the first/last yardage
///   signal — `previous_milli`/`ytg_milli` of `YtgRecalculated` or
///   `ytg_before_milli`/`ytg_after_milli` of `TacticCompleted` — and
///   `0`/`0` when the journal carries no yardage signal at all.
pub fn review_package(
    franchise_id: &str,
    objective_id: &str,
    drive_id: &str,
    events: &[DriveEvent],
) -> Result<PostDriveReview, ReviewError> {
    if !drive_is_terminal(events) {
        return Err(ReviewError::DriveNotTerminal);
    }

    let mut personnel_used: Vec<String> = Vec::new();
    let mut tactics_called: Vec<String> = Vec::new();
    let mut tactic_ytg_deltas: Vec<(String, i64)> = Vec::new();
    let mut substitutions: Vec<(String, String)> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut recovered_knowledge: Vec<String> = Vec::new();
    let mut inference_spend_tokens: u64 = 0;
    let mut tool_spend_calls: u64 = 0;
    let mut evidence_count: u32 = 0;
    let mut opening_ytg_milli: Option<u32> = None;
    let mut closing_ytg_milli: u32 = 0;

    for event in events {
        match event {
            DriveEvent::PersonnelChanged { personnel } => {
                push_unique(&mut personnel_used, personnel.id.as_str().to_owned());
            }
            DriveEvent::PossessionStarted { personnel, .. } => {
                push_unique(&mut personnel_used, personnel.id.as_str().to_owned());
            }
            DriveEvent::PlayerSubstituted {
                out, replacement, ..
            } => {
                let incoming = replacement
                    .as_ref()
                    .map(|id| id.as_str().to_owned())
                    .unwrap_or_else(|| "none".to_owned());
                substitutions.push((out.as_str().to_owned(), incoming));
                if let Some(id) = replacement {
                    push_unique(&mut personnel_used, id.as_str().to_owned());
                }
            }
            DriveEvent::TacticAdmitted { tactic, .. } => {
                tactics_called.push(tactic.clone());
            }
            DriveEvent::TacticCompleted {
                tactic,
                ytg_before_milli,
                ytg_after_milli,
                ..
            } => {
                let delta =
                    i64::from(*ytg_after_milli).saturating_sub(i64::from(*ytg_before_milli));
                tactic_ytg_deltas.push((tactic.clone(), delta));
                if opening_ytg_milli.is_none() {
                    opening_ytg_milli = Some(*ytg_before_milli);
                }
                closing_ytg_milli = *ytg_after_milli;
            }
            DriveEvent::ToolFailed { failure } => {
                failures.push(failure_mode(failure));
            }
            DriveEvent::CersRecovered { artefact_id, .. } => {
                recovered_knowledge.push(artefact_id.as_str().to_owned());
            }
            DriveEvent::BudgetConsumed { cost, .. } => {
                inference_spend_tokens = inference_spend_tokens.saturating_add(cost.tokens);
            }
            DriveEvent::BlitzBudgetConsumed { cost, .. } => {
                inference_spend_tokens = inference_spend_tokens.saturating_add(cost.tokens);
            }
            DriveEvent::ToolInvoked { .. } => tool_spend_calls = tool_spend_calls.saturating_add(1),
            DriveEvent::EvidenceAccepted { .. } => {
                evidence_count = evidence_count.saturating_add(1)
            }
            DriveEvent::YtgRecalculated {
                ytg_milli,
                previous_milli,
            } => {
                if opening_ytg_milli.is_none() {
                    opening_ytg_milli = Some(*previous_milli);
                }
                closing_ytg_milli = *ytg_milli;
            }
            _ => {}
        }
    }

    Ok(PostDriveReview {
        franchise_id: franchise_id.to_owned(),
        objective_id: objective_id.to_owned(),
        drive_id: drive_id.to_owned(),
        opening_ytg_milli: opening_ytg_milli.unwrap_or(0),
        closing_ytg_milli,
        personnel_used,
        tactics_called,
        tactic_ytg_deltas,
        substitutions,
        inference_spend_tokens,
        tool_spend_calls,
        failures,
        recovered_knowledge,
        evidence_count,
    })
}

/// Redact in place: every string field in the package carrying a credential
/// needle (`"api_key="`, `"bearer "`, `"password="`, `"private_key="`,
/// `"authorization:"`) or an absolute home path (`"/home/"`) is replaced
/// wholesale with `"[REDACTED]"`. Deterministic and idempotent; returns the
/// number of fields redacted.
pub fn redact(review: &mut PostDriveReview) -> u32 {
    let mut redactions = 0;
    redact_field(&mut review.franchise_id, &mut redactions);
    redact_field(&mut review.objective_id, &mut redactions);
    redact_field(&mut review.drive_id, &mut redactions);
    for id in &mut review.personnel_used {
        redact_field(id, &mut redactions);
    }
    for tactic in &mut review.tactics_called {
        redact_field(tactic, &mut redactions);
    }
    for (tactic, _) in &mut review.tactic_ytg_deltas {
        redact_field(tactic, &mut redactions);
    }
    for (out, replacement) in &mut review.substitutions {
        redact_field(out, &mut redactions);
        redact_field(replacement, &mut redactions);
    }
    for failure in &mut review.failures {
        redact_field(failure, &mut redactions);
    }
    for artefact in &mut review.recovered_knowledge {
        redact_field(artefact, &mut redactions);
    }
    redactions
}

/// A personnel adjustment recommendation for the next Franchise planning
/// cycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonnelRecommendation {
    pub personnel: String,
    pub reason_code: String,
}

/// A tactical finding about one admitted tactic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalFinding {
    pub tactic: String,
    pub finding: String,
    pub delta_ytg_milli: i64,
}

/// A salary-side finding about spend versus yardage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SalaryFinding {
    pub finding: String,
}

/// An infrastructure opportunity observed in the drive's failure record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureOpportunity {
    pub pattern: String,
    pub suggestion: String,
}

/// The §18/§20 structured evaluation of a review package: deterministic
/// recommendations that enter the next Franchise planning cycle as
/// evidence. Evaluating never touches the drive — it reads a package, not
/// a journal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriveReviewResult {
    pub personnel_adjustments: Vec<PersonnelRecommendation>,
    pub tactical_findings: Vec<TacticalFinding>,
    pub salary_findings: Vec<SalaryFinding>,
    pub infrastructure_opportunities: Vec<InfrastructureOpportunity>,
}

/// Evaluate a review package. Deterministic rules, in fixed order:
///
/// - **Personnel** — a personnel with zero positive tactic deltas overall
///   produces `PersonnelRecommendation { reason_code:
///   "consumed_spend_without_ytg" }`. Attribution is positional and only
///   defined when `personnel_used.len() == tactics_called.len()`; then
///   `personnel_used[i]` is attributed `tactics_called[i]`, and the
///   personnel is recommended out unless some recorded delta for that
///   tactic name is positive. When the lengths differ the attribution is
///   undefined, so personnel findings are skipped entirely (documented
///   limitation; tactical, salary, and infrastructure findings are
///   unaffected).
/// - **Tactical** — a tactic whose delta is negative produces
///   `TacticalFinding { finding: "negative_ytg", delta as-is }`; a tactic
///   whose delta exceeds 1,000,000 milli produces
///   `TacticalFinding { finding: "outlier_gain_review", delta }`. The two
///   are mutually exclusive, so at most one finding fires per tactic, in
///   package order.
/// - **Salary** — `inference_spend_tokens > 0` with
///   `closing_ytg_milli == opening_ytg_milli` produces
///   `SalaryFinding { finding: "full_spend_zero_ytg" }`.
/// - **Infrastructure** — at least two failures with empty
///   `recovered_knowledge` produces `InfrastructureOpportunity {
///   pattern: "repeated_failures_no_cers", suggestion: "route failures
///   through cers harvesting" }`; at least two failures whose failure
///   strings are all identical produces `InfrastructureOpportunity {
///   pattern: "repeated_identical_failure", suggestion: "deterministic
///   tooling candidate" }` — the §20 inference displacement KPI. Both may
///   fire for the same package; they are emitted in that order.
pub fn evaluate(review: &PostDriveReview) -> DriveReviewResult {
    let personnel_adjustments = personnel_findings(review);

    let tactical_findings: Vec<TacticalFinding> = review
        .tactic_ytg_deltas
        .iter()
        .filter_map(|(tactic, delta)| {
            let finding = if *delta < 0 {
                FINDING_NEGATIVE_YTG
            } else if *delta > OUTLIER_GAIN_MILLI {
                FINDING_OUTLIER_GAIN_REVIEW
            } else {
                return None;
            };
            Some(TacticalFinding {
                tactic: tactic.clone(),
                finding: finding.to_owned(),
                delta_ytg_milli: *delta,
            })
        })
        .collect();

    let salary_findings = if review.inference_spend_tokens > 0
        && review.closing_ytg_milli == review.opening_ytg_milli
    {
        vec![SalaryFinding {
            finding: FINDING_FULL_SPEND_ZERO_YTG.to_owned(),
        }]
    } else {
        Vec::new()
    };

    let mut infrastructure_opportunities = Vec::new();
    if review.failures.len() >= 2 {
        if review.recovered_knowledge.is_empty() {
            infrastructure_opportunities.push(InfrastructureOpportunity {
                pattern: PATTERN_REPEATED_FAILURES_NO_CERS.to_owned(),
                suggestion: SUGGESTION_ROUTE_FAILURES_THROUGH_CERS.to_owned(),
            });
        }
        let first = &review.failures[0];
        if review.failures.iter().all(|failure| failure == first) {
            infrastructure_opportunities.push(InfrastructureOpportunity {
                pattern: PATTERN_REPEATED_IDENTICAL_FAILURE.to_owned(),
                suggestion: SUGGESTION_DETERMINISTIC_TOOLING.to_owned(),
            });
        }
    }

    DriveReviewResult {
        personnel_adjustments,
        tactical_findings,
        salary_findings,
        infrastructure_opportunities,
    }
}

/// True when the slice ends a drive: any `DriveCompleted`, `PuntIssued`, or
/// `AbandonIssued`, or a `FieldGoalSelected` that no later `DriveStarted`
/// has superseded.
fn drive_is_terminal(events: &[DriveEvent]) -> bool {
    let mut terminal = false;
    let mut field_goal_open = false;
    for event in events {
        match event {
            DriveEvent::DriveCompleted { .. }
            | DriveEvent::PuntIssued { .. }
            | DriveEvent::AbandonIssued { .. } => terminal = true,
            DriveEvent::FieldGoalSelected { .. } => field_goal_open = true,
            DriveEvent::DriveStarted { .. } => field_goal_open = false,
            _ => {}
        }
    }
    terminal || field_goal_open
}

/// The failure mode of a tool failure as a stable lowercase snake-case
/// string, straight off the typed kind.
fn failure_mode(failure: &ToolFailure) -> String {
    format!("{:?}", failure.kind).to_lowercase()
}

/// Append unless already present: first-seen order, no duplicates.
fn push_unique(target: &mut Vec<String>, value: String) {
    if !target.contains(&value) {
        target.push(value);
    }
}

/// Redact one string field if it carries a needle.
fn redact_field(field: &mut String, redactions: &mut u32) {
    let secret = CREDENTIAL_NEEDLES
        .iter()
        .chain(std::iter::once(&HOME_PATH_NEEDLE))
        .any(|needle| field.contains(needle));
    if secret {
        *field = REDACTED.to_owned();
        *redactions += 1;
    }
}

/// Personnel findings under the documented positional-pairing rule.
fn personnel_findings(review: &PostDriveReview) -> Vec<PersonnelRecommendation> {
    if review.personnel_used.len() != review.tactics_called.len() {
        // Documented limitation: the journal carries no deeper
        // personnel→tactic binding, so attribution is undefined and the
        // finding is skipped rather than guessed.
        return Vec::new();
    }
    review
        .personnel_used
        .iter()
        .zip(review.tactics_called.iter())
        .filter(|(_, tactic)| !has_positive_delta(review, tactic))
        .map(|(personnel, _)| PersonnelRecommendation {
            personnel: personnel.clone(),
            reason_code: REASON_CONSUMED_SPEND_WITHOUT_YTG.to_owned(),
        })
        .collect()
}

/// Whether any recorded delta for this tactic name is strictly positive.
fn has_positive_delta(review: &PostDriveReview, tactic: &str) -> bool {
    review
        .tactic_ytg_deltas
        .iter()
        .any(|(name, delta)| name == tactic && *delta > 0)
}

#[cfg(test)]
mod tests {
    use playbook_core::{
        CapabilityId, DriveBudget, Evidence, EvidenceClass, EvidenceId, GateId, PersonnelId,
        PlayId, PlayKind, ToolFailure, ToolFailureKind,
    };
    use serde_json::json;

    use super::*;

    const FRANCHISE: &str = "franchise-1";
    const OBJECTIVE: &str = "objective-1";
    const DRIVE: &str = "drive-1";

    fn packaged(events: &[DriveEvent]) -> PostDriveReview {
        review_package(FRANCHISE, OBJECTIVE, DRIVE, events).expect("terminal journal")
    }

    fn drive_completed() -> DriveEvent {
        DriveEvent::DriveCompleted { at_ms: 9_000 }
    }

    fn admitted(tactic: &str) -> DriveEvent {
        DriveEvent::TacticAdmitted {
            tactic: tactic.to_owned(),
            at_ms: 1_000,
        }
    }

    fn completed(tactic: &str, before_milli: u32, after_milli: u32) -> DriveEvent {
        DriveEvent::TacticCompleted {
            tactic: tactic.to_owned(),
            success: true,
            ytg_before_milli: before_milli,
            ytg_after_milli: after_milli,
            tokens_spent: 0,
            at_ms: 2_000,
        }
    }

    fn ytg(previous_milli: u32, ytg_milli: u32) -> DriveEvent {
        DriveEvent::YtgRecalculated {
            ytg_milli,
            previous_milli,
        }
    }

    fn tokens(spent: u64) -> DriveEvent {
        DriveEvent::BudgetConsumed {
            cost: DriveBudget {
                tokens: spent,
                ..DriveBudget::zero()
            },
            at_ms: 3_000,
        }
    }

    fn substituted(out: &str, replacement: Option<&str>) -> DriveEvent {
        DriveEvent::PlayerSubstituted {
            at_ms: 4_000,
            class: "inference".to_owned(),
            out: PersonnelId::new(out).expect("valid id"),
            replacement: replacement.map(|id| PersonnelId::new(id).expect("valid id")),
            reasons: Vec::new(),
        }
    }

    fn tool_failed(kind: ToolFailureKind) -> DriveEvent {
        DriveEvent::ToolFailed {
            failure: ToolFailure {
                play_id: PlayId::new("play-1").expect("valid id"),
                play_kind: PlayKind::TargetedInference,
                operation: "invoke".to_owned(),
                capability_id: CapabilityId::new("cap-1").expect("valid id"),
                personnel_id: PersonnelId::new("p-1").expect("valid id"),
                kind,
                diagnostics: Vec::new(),
                cost: DriveBudget::zero(),
                at_ms: 5_000,
                idempotency_key: None,
                side_effects: Vec::new(),
            },
        }
    }

    fn cers_recovered(artefact: &str) -> DriveEvent {
        DriveEvent::CersRecovered {
            artefact_id: EvidenceId::new(artefact).expect("valid id"),
            source: "film-session".to_owned(),
            confirmed_facts: 1,
            failed_assumptions: 0,
            reuse_hint: String::new(),
            at_ms: 6_000,
        }
    }

    fn evidence() -> DriveEvent {
        DriveEvent::EvidenceAccepted {
            evidence: Evidence::seal(
                EvidenceClass::ToolInvocation,
                json!({"tool": "probe", "ok": true}),
                7_000,
            )
            .expect("valid evidence"),
        }
    }

    #[test]
    fn a_journal_without_a_terminal_event_refuses_the_review() {
        let open_drive = [admitted("quick-slant")];
        assert_eq!(
            review_package(FRANCHISE, OBJECTIVE, DRIVE, &open_drive),
            Err(ReviewError::DriveNotTerminal)
        );
        // A field goal with no later DriveStarted is a terminal drive event.
        let kicked = [DriveEvent::FieldGoalSelected {
            gate_id: GateId::new("gate-1").expect("valid id"),
        }];
        assert!(review_package(FRANCHISE, OBJECTIVE, DRIVE, &kicked).is_ok());
        // So is a completed drive.
        assert!(packaged(&[drive_completed()])
            .drive_id
            .starts_with("drive-"));
    }

    #[test]
    fn the_same_journal_yields_the_same_package() {
        let journal = vec![
            substituted("p-1", Some("p-2")),
            admitted("quick-slant"),
            completed("quick-slant", 400_000, 640_000),
            tokens(5_000),
            evidence(),
            evidence(),
            cers_recovered("art-1"),
            drive_completed(),
        ];
        assert_eq!(packaged(&journal), packaged(&journal));
        let review = packaged(&journal);
        assert_eq!(review.personnel_used, vec!["p-2"]);
        assert_eq!(review.tactics_called, vec!["quick-slant"]);
        assert_eq!(review.substitutions, vec![("p-1".into(), "p-2".into())]);
        assert_eq!(review.inference_spend_tokens, 5_000);
        assert_eq!(review.tool_spend_calls, 0);
        assert_eq!(review.evidence_count, 2);
        assert_eq!(review.recovered_knowledge, vec!["art-1"]);
        assert_eq!(review.opening_ytg_milli, 400_000);
        assert_eq!(review.closing_ytg_milli, 640_000);
    }

    #[test]
    fn tactic_deltas_are_after_minus_before_as_signed_milli() {
        let journal = vec![
            admitted("deep-post"),
            completed("deep-post", 1_200_000, 800_000),
            admitted("quick-slant"),
            completed("quick-slant", 500_000, 1_600_000),
            drive_completed(),
        ];
        let review = packaged(&journal);
        assert_eq!(
            review.tactic_ytg_deltas,
            vec![
                ("deep-post".to_owned(), -400_000),
                ("quick-slant".to_owned(), 1_100_000),
            ]
        );
    }

    #[test]
    fn personnel_findings_fire_only_without_a_positive_delta_and_only_when_paired() {
        // One substitution brings p-2 on; one tactic ran; it lost yardage.
        let losing = vec![
            substituted("p-1", Some("p-2")),
            admitted("deep-post"),
            completed("deep-post", 900_000, 700_000),
            drive_completed(),
        ];
        assert_eq!(
            evaluate(&packaged(&losing)).personnel_adjustments,
            vec![PersonnelRecommendation {
                personnel: "p-2".to_owned(),
                reason_code: REASON_CONSUMED_SPEND_WITHOUT_YTG.to_owned(),
            }]
        );

        // The same drive with a positive delta keeps the personnel.
        let winning = vec![
            substituted("p-1", Some("p-2")),
            admitted("deep-post"),
            completed("deep-post", 700_000, 900_000),
            drive_completed(),
        ];
        assert!(evaluate(&packaged(&winning))
            .personnel_adjustments
            .is_empty());

        // Documented limitation: when the lengths differ the positional
        // personnel→tactic pairing is undefined, so personnel findings are
        // skipped even with a negative delta on the board.
        let unpaired = vec![
            substituted("p-1", Some("p-2")),
            admitted("deep-post"),
            admitted("screen"),
            completed("deep-post", 900_000, 700_000),
            drive_completed(),
        ];
        assert!(evaluate(&packaged(&unpaired))
            .personnel_adjustments
            .is_empty());
    }

    #[test]
    fn negative_and_outlier_deltas_produce_tactical_findings_in_order() {
        let journal = vec![
            admitted("deep-post"),
            completed("deep-post", 900_000, 600_000),
            admitted("hail-mary"),
            completed("hail-mary", 0, 2_500_000),
            admitted("screen"),
            completed("screen", 100_000, 100_500),
            drive_completed(),
        ];
        assert_eq!(
            evaluate(&packaged(&journal)).tactical_findings,
            vec![
                TacticalFinding {
                    tactic: "deep-post".to_owned(),
                    finding: FINDING_NEGATIVE_YTG.to_owned(),
                    delta_ytg_milli: -300_000,
                },
                TacticalFinding {
                    tactic: "hail-mary".to_owned(),
                    finding: FINDING_OUTLIER_GAIN_REVIEW.to_owned(),
                    delta_ytg_milli: 2_500_000,
                },
            ]
        );
    }

    #[test]
    fn full_spend_at_zero_ytg_is_a_salary_finding() {
        let spinning = vec![ytg(1_000_000, 1_000_000), tokens(5_000), drive_completed()];
        assert_eq!(
            evaluate(&packaged(&spinning)).salary_findings,
            vec![SalaryFinding {
                finding: FINDING_FULL_SPEND_ZERO_YTG.to_owned(),
            }]
        );

        // Yardage moved: no salary finding.
        let productive = vec![ytg(1_000_000, 1_400_000), tokens(5_000), drive_completed()];
        assert!(evaluate(&packaged(&productive)).salary_findings.is_empty());

        // No spend, no salary finding.
        let thrifty = vec![ytg(1_000_000, 1_000_000), drive_completed()];
        assert!(evaluate(&packaged(&thrifty)).salary_findings.is_empty());
    }

    #[test]
    fn repeated_identical_failures_nominate_deterministic_tooling() {
        let flaky = vec![
            tool_failed(ToolFailureKind::Timeout),
            tool_failed(ToolFailureKind::Timeout),
            drive_completed(),
        ];
        assert_eq!(
            evaluate(&packaged(&flaky)).infrastructure_opportunities,
            vec![
                InfrastructureOpportunity {
                    pattern: PATTERN_REPEATED_FAILURES_NO_CERS.to_owned(),
                    suggestion: SUGGESTION_ROUTE_FAILURES_THROUGH_CERS.to_owned(),
                },
                InfrastructureOpportunity {
                    pattern: PATTERN_REPEATED_IDENTICAL_FAILURE.to_owned(),
                    suggestion: SUGGESTION_DETERMINISTIC_TOOLING.to_owned(),
                },
            ]
        );

        // A CERS recovery clears the no-cers pattern, not the tooling one.
        let recovered = vec![
            tool_failed(ToolFailureKind::Timeout),
            tool_failed(ToolFailureKind::Timeout),
            cers_recovered("art-1"),
            drive_completed(),
        ];
        let opportunities = evaluate(&packaged(&recovered)).infrastructure_opportunities;
        assert_eq!(opportunities.len(), 1);
        assert_eq!(opportunities[0].pattern, PATTERN_REPEATED_IDENTICAL_FAILURE);

        // Distinct failure modes escape the identical-failure pattern but
        // still carry the no-cers one.
        let mixed = vec![
            tool_failed(ToolFailureKind::Timeout),
            tool_failed(ToolFailureKind::RateLimit),
            drive_completed(),
        ];
        let opportunities = evaluate(&packaged(&mixed)).infrastructure_opportunities;
        assert_eq!(opportunities.len(), 1);
        assert_eq!(opportunities[0].pattern, PATTERN_REPEATED_FAILURES_NO_CERS);
    }

    #[test]
    fn credential_needles_are_redacted_in_place_and_counted() {
        let journal = vec![admitted("bearer xyz sweep"), drive_completed()];
        let mut review = packaged(&journal);
        assert_eq!(review.tactics_called, vec!["bearer xyz sweep"]);
        assert_eq!(redact(&mut review), 1);
        assert_eq!(review.tactics_called, vec![REDACTED]);
        // Idempotent: a redacted package redacts nothing further.
        assert_eq!(redact(&mut review), 0);
    }

    #[test]
    fn the_review_consumes_a_shared_journal_and_cannot_rewrite_the_drive() {
        // Structural pin: `review_package` takes `&[DriveEvent]` — a shared
        // reference the compiler guarantees cannot append, reorder, or
        // rewrite the journal — and `PostDriveReview` is plain data (ids,
        // counters, vectors) with no journal handle, so this module exposes
        // no method taking drive authority. Dreamseq MAY analyse; it SHALL
        // NOT reopen or rewrite the completed drive. Pinned structurally:
        // the same slice is reviewed twice, the slice is untouched, and the
        // packages agree.
        let journal = vec![
            admitted("quick-slant"),
            completed("quick-slant", 100_000, 300_000),
            drive_completed(),
        ];
        let first = review_package(FRANCHISE, OBJECTIVE, DRIVE, &journal).expect("terminal");
        let second = review_package(FRANCHISE, OBJECTIVE, DRIVE, &journal).expect("terminal");
        assert_eq!(first, second);
        assert_eq!(journal.len(), 3);
        assert_eq!(journal[0].name(), "tactic_admitted");
        assert_eq!(journal[2].name(), "drive_completed");
        assert_eq!(first.drive_id, DRIVE);
    }
}
