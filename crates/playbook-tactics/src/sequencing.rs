//! Multi-play tactical memory: constraint plays and play sequencing.
//!
//! Football translation → computational behaviour:
//!
//! - **Constraint Play (§6.23)**: a coordinator that keeps calling one
//!   dominant tactic against a defence that has already answered it is
//!   overcommitted. The counter is a *constraint*: a complementary tactic
//!   designed to exploit the PREDICTABLE failure response of the dominant
//!   strategy. `Spread` ×N collapses into coherence debt → concentrate
//!   cooperatively with `bunch`. Explosive `verticals` leave fragile state
//!   → catastrophic-risk avoidance with `prevent`. Parallel `flood` churns
//!   integrations and `quick_game` advances only shallowly → small
//!   validated stages with `make_them_drive`. Concentrated `heavy` stalls
//!   single-threaded → broaden with `spread`.
//! - **Play Sequencing (§6.24)**: earlier plays create the expectations
//!   later plays exploit. The [`TacticalHistory`] is the tactical memory;
//!   it records named outcomes in insertion order and answers deterministic
//!   questions — trailing runs, integer parts-per-million success rates,
//!   time monotonicity — that [`recommend_next`] folds into the next call.
//!
//! Everything here is deterministic, float-free, and saturating: the fold
//! reads only recorded outcomes, never judgement, and the same history
//! always recommends the same next call.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// One graded outcome in the tactical memory. Evidence grades the play;
/// the record never rewrites it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryRow {
    /// The machine name of the called tactic, e.g. "spread".
    pub tactic: String,
    /// Whether the evidence graded the play a success.
    pub success: bool,
    /// Yards to go before the play, in milli.
    pub ytg_before_milli: u32,
    /// Yards to go after the play, in milli.
    pub ytg_after_milli: u32,
    /// The monotonic timestamp the sequencing fold orders plays by, in ms.
    pub at_ms: u64,
}

/// The multi-play tactical memory: every graded outcome, oldest first.
/// Sequencing reads only this record; appending is its sole mutation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalHistory {
    records: Vec<HistoryRow>,
}

impl TacticalHistory {
    /// An empty memory: nothing has been called yet.
    pub fn empty() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Append one graded outcome. Insertion order is preserved: the memory
    /// never reorders or rewrites what was recorded.
    pub fn record(&mut self, row: HistoryRow) {
        self.records.push(row);
    }

    /// The last `window` records, oldest first. A `window` of 0 is an empty
    /// slice; a `window` beyond the memory's length is the whole memory.
    pub fn recent(&self, window: usize) -> &[HistoryRow] {
        let start = self.records.len().saturating_sub(window);
        &self.records[start..]
    }

    /// How many outcomes the memory holds.
    pub fn total(&self) -> u32 {
        u32::try_from(self.records.len()).unwrap_or(u32::MAX)
    }

    /// How many of the most recent records are the named tactic: the length
    /// of the trailing run. Zero when the memory is empty or the last play
    /// called something else.
    pub fn consecutive(&self, tactic: &str) -> u32 {
        let run = self
            .records
            .iter()
            .rev()
            .take_while(|row| row.tactic == tactic)
            .count();
        u32::try_from(run).unwrap_or(u32::MAX)
    }

    /// The success rate of the named tactic over its last `window`
    /// occurrences, 0..=1_000_000 ppm by pure integer math. `None` when the
    /// tactic has no occurrence inside the window (a `window` of 0 counts
    /// as none).
    pub fn success_rate_ppm(&self, tactic: &str, window: usize) -> Option<u32> {
        let mut occurrences = 0u64;
        let mut successes = 0u64;
        for row in self
            .records
            .iter()
            .rev()
            .filter(|row| row.tactic == tactic)
            .take(window)
        {
            occurrences = occurrences.saturating_add(1);
            if row.success {
                successes = successes.saturating_add(1);
            }
        }
        if occurrences == 0 {
            return None;
        }
        let ppm = successes.saturating_mul(1_000_000) / occurrences;
        // successes <= occurrences, so ppm is already within 0..=1_000_000.
        Some(u32::try_from(ppm).unwrap_or(1_000_000))
    }

    /// Whether the memory is ordered non-decreasingly in `at_ms` — the
    /// invariant the sequencing fold requires. Vacuously true for empty and
    /// single-record memories; equal timestamps count as ordered.
    pub fn monotonic_time(&self) -> bool {
        self.records
            .windows(2)
            .all(|pair| pair[0].at_ms <= pair[1].at_ms)
    }
}

/// A detected overcommitment to one strategy: which tactic dominated, how
/// long its run at the tail of the window was, and how far its recent
/// success has decayed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overreaction {
    /// The machine name of the overcommitted tactic.
    pub tactic: String,
    /// The run length of that tactic at the tail of the window, as
    /// `TacticalHistory::consecutive` counts it.
    pub repeats: u32,
    /// The success rate of that tactic across its occurrences inside the
    /// window, 0..=1_000_000 ppm — strictly below half by construction.
    pub recent_success_rate_ppm: u32,
}

/// Overreaction detection (§6.24): when a dominant tactic has been repeated
/// and its outcomes turn sour, the playbook is overcommitted to one
/// strategy. The deterministic rule over the last `window` records,
/// examined most recent first:
///
/// 1. `threshold_repeats` must be at least 1, and the window must contain
///    at least that many records; otherwise nothing fires.
/// 2. The FIRST `threshold_repeats` records examined — the most recent
///    ones, the trailing run that `consecutive` counts — must all name the
///    same tactic: the dominant one. `repeats` reports the full run length
///    of it at the tail of the window.
/// 3. The success rate of that tactic across its occurrences inside the
///    window must have dropped below 500_000 ppm (less than half of its
///    occurrences succeeding).
///
/// The same history, window, and threshold always decide the same way.
pub fn detect_overreaction(
    history: &TacticalHistory,
    window: usize,
    threshold_repeats: usize,
) -> Option<Overreaction> {
    if threshold_repeats == 0 {
        return None;
    }
    let slice = history.recent(window);
    if slice.len() < threshold_repeats {
        return None;
    }
    // The window is non-empty, so it has a most recent record.
    let latest = &slice[slice.len() - 1];
    let tactic = latest.tactic.clone();
    let tail_run = slice
        .iter()
        .rev()
        .take_while(|row| row.tactic == tactic)
        .count();
    if tail_run < threshold_repeats {
        return None;
    }
    let mut occurrences = 0u64;
    let mut successes = 0u64;
    for row in slice.iter().filter(|row| row.tactic == tactic) {
        occurrences = occurrences.saturating_add(1);
        if row.success {
            successes = successes.saturating_add(1);
        }
    }
    let ppm = successes.saturating_mul(1_000_000) / occurrences;
    let ppm = u32::try_from(ppm).unwrap_or(1_000_000);
    if ppm >= 500_000 {
        return None;
    }
    Some(Overreaction {
        tactic,
        repeats: u32::try_from(tail_run).unwrap_or(u32::MAX),
        recent_success_rate_ppm: ppm,
    })
}

/// The frozen constraint table (§6.23): each dominant tactic maps to the
/// complementary tactic that exploits its predictable failure response.
///
/// - `"spread"` → `"bunch"`: breadth overreaction → coherence collapse →
///   concentrate cooperatively.
/// - `"verticals"` → `"prevent"`: explosive overreaction → fragile state →
///   catastrophic-risk avoidance.
/// - `"flood"` → `"make_them_drive"`: parallel overreaction → integration
///   churn → small validated stages.
/// - `"quick_game"` → `"make_them_drive"`: tempo overreaction → shallow
///   progress → staged validation.
/// - `"heavy"` → `"spread"`: concentration overreaction → single-threaded
///   stall → broaden.
///
/// Anything else → `"play_sequencing"`: the default counter is sequencing
/// itself — change the call and track the outcome.
pub fn constraint_for(overreaction: &Overreaction) -> &'static str {
    match overreaction.tactic.as_str() {
        "spread" => "bunch",
        "verticals" => "prevent",
        "flood" => "make_them_drive",
        "quick_game" => "make_them_drive",
        "heavy" => "spread",
        _ => "play_sequencing",
    }
}

/// The sequencing recommendation (§6.23 + §6.24): if an overreaction has
/// been detected for the tactic the coordinator plans to call again, return
/// its constraint counter instead; otherwise return the planned tactic
/// unchanged. Counters are `'static` doctrine names; a kept plan is an
/// owned copy, since the caller's name borrows no slice of this module.
pub fn recommend_next(
    history: &TacticalHistory,
    planned: &str,
    window: usize,
    threshold_repeats: usize,
) -> Cow<'static, str> {
    match detect_overreaction(history, window, threshold_repeats) {
        // The planned call is the overcommitted one: sequence its constraint.
        Some(overreaction) if overreaction.tactic == planned => {
            Cow::Borrowed(constraint_for(&overreaction))
        }
        // No overreaction, or the plan already moves off the dominant tactic.
        _ => Cow::Owned(planned.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(tactic: &str, success: bool, at_ms: u64) -> HistoryRow {
        HistoryRow {
            tactic: tactic.into(),
            success,
            ytg_before_milli: 10_000,
            ytg_after_milli: if success { 6_000 } else { 10_000 },
            at_ms,
        }
    }

    fn history(calls: &[(&str, bool)]) -> TacticalHistory {
        let mut history = TacticalHistory::empty();
        for (index, (tactic, success)) in calls.iter().enumerate() {
            history.record(row(tactic, *success, index as u64 * 1_000));
        }
        history
    }

    #[test]
    fn history_keeps_insertion_order_and_slices_recent_windows() {
        let log = history(&[("spread", true), ("heavy", false), ("flood", true)]);
        assert_eq!(log.total(), 3);
        assert_eq!(log.recent(2).len(), 2);
        // Oldest first inside the window.
        assert_eq!(log.recent(2)[0].tactic, "heavy");
        assert_eq!(log.recent(2)[1].tactic, "flood");
        // Window 0 is an empty slice; an oversized window is the whole memory.
        assert_eq!(log.recent(0).len(), 0);
        assert_eq!(log.recent(50).len(), 3);
        let mut appended = log.clone();
        appended.record(row("bunch", true, 9_000));
        assert_eq!(appended.total(), 4);
        assert_eq!(appended.recent(1)[0].tactic, "bunch");
    }

    #[test]
    fn consecutive_counts_the_trailing_run_of_one_tactic() {
        // Insertion order is oldest first, so the mission's "Spread, Spread,
        // Spread, Heavy" narrative reads: heavy first, then spread as the
        // three most recent calls.
        let log = history(&[
            ("heavy", true),
            ("spread", false),
            ("spread", false),
            ("spread", false),
        ]);
        assert_eq!(log.consecutive("spread"), 3);
        assert_eq!(log.consecutive("heavy"), 0);
        // A trailing heavy interrupts the spread run at one.
        let flipped = history(&[
            ("spread", true),
            ("spread", false),
            ("spread", true),
            ("heavy", true),
        ]);
        assert_eq!(flipped.consecutive("heavy"), 1);
        assert_eq!(flipped.consecutive("spread"), 0);
        assert_eq!(flipped.consecutive("bunch"), 0);
        assert_eq!(TacticalHistory::empty().consecutive("spread"), 0);
    }

    #[test]
    fn success_rate_ppm_is_exact_integer_math() {
        let log = history(&[
            ("spread", true),
            ("spread", false),
            ("spread", true),
            ("spread", false),
        ]);
        assert_eq!(log.success_rate_ppm("spread", 4), Some(500_000));
        let cold = history(&[("spread", false), ("spread", false)]);
        assert_eq!(cold.success_rate_ppm("spread", 4), Some(0));
        let hot = history(&[("spread", true), ("spread", true)]);
        assert_eq!(hot.success_rate_ppm("spread", 4), Some(1_000_000));
        // No occurrence of the tactic, or no window: no rate.
        assert_eq!(log.success_rate_ppm("bunch", 4), None);
        assert_eq!(log.success_rate_ppm("spread", 0), None);
        // The window counts the most recent occurrences of the tactic itself.
        let mixed = history(&[
            ("spread", true),
            ("spread", true),
            ("heavy", false),
            ("spread", false),
            ("spread", false),
        ]);
        assert_eq!(mixed.success_rate_ppm("spread", 2), Some(0));
        assert_eq!(mixed.success_rate_ppm("spread", 3), Some(333_333));
        assert_eq!(mixed.success_rate_ppm("spread", 4), Some(500_000));
    }

    #[test]
    fn time_is_monotonic_exactly_when_records_never_move_backwards() {
        assert!(TacticalHistory::empty().monotonic_time());
        let mut single = TacticalHistory::empty();
        single.record(row("spread", true, 5_000));
        assert!(single.monotonic_time());
        let ordered = history(&[("spread", true), ("heavy", false), ("flood", true)]);
        assert!(ordered.monotonic_time());
        // Equal timestamps are non-decreasing, not a violation.
        let mut tied = TacticalHistory::empty();
        tied.record(row("spread", true, 1_000));
        tied.record(row("heavy", true, 1_000));
        assert!(tied.monotonic_time());
        let mut rewound = history(&[("spread", true), ("heavy", false)]);
        rewound.record(row("flood", true, 500));
        assert!(!rewound.monotonic_time());
    }

    #[test]
    fn overreaction_fires_at_exactly_the_threshold_repeats() {
        // The window opens with three spreads whose rate decayed to 1 of 3.
        let log = history(&[
            ("heavy", true),
            ("spread", true),
            ("spread", false),
            ("spread", false),
        ]);
        let detected =
            detect_overreaction(&log, 4, 3).expect("three degraded spreads are an overreaction");
        assert_eq!(detected.tactic, "spread");
        assert_eq!(detected.repeats, 3);
        assert_eq!(detected.recent_success_rate_ppm, 333_333);
        // One repeat short of the demanded threshold: nothing fires.
        assert_eq!(detect_overreaction(&log, 4, 4), None);
        // A window too short to hold the demanded run: nothing fires.
        assert_eq!(detect_overreaction(&log, 2, 3), None);
    }

    #[test]
    fn overreaction_requires_a_degraded_rate_not_merely_repetition() {
        // Three spreads, two of them successful: 666_666 ppm is above half.
        let winning = history(&[("spread", true), ("spread", true), ("spread", false)]);
        assert_eq!(detect_overreaction(&winning, 3, 3), None);
        // Exactly half succeeding is not below half.
        let even = history(&[
            ("spread", true),
            ("spread", true),
            ("spread", false),
            ("spread", false),
        ]);
        assert_eq!(detect_overreaction(&even, 4, 3), None);
        // Degraded rate, but the head of the window is not one tactic.
        let broken_run = history(&[
            ("spread", false),
            ("heavy", false),
            ("spread", false),
            ("spread", false),
        ]);
        assert_eq!(detect_overreaction(&broken_run, 4, 3), None);
        // A zero threshold demands no repeats and detects nothing.
        assert_eq!(detect_overreaction(&even, 4, 0), None);
    }

    #[test]
    fn the_constraint_table_maps_every_doctrine_and_defaults_to_sequencing() {
        fn over(tactic: &str) -> Overreaction {
            Overreaction {
                tactic: tactic.into(),
                repeats: 3,
                recent_success_rate_ppm: 250_000,
            }
        }
        assert_eq!(constraint_for(&over("spread")), "bunch");
        assert_eq!(constraint_for(&over("verticals")), "prevent");
        assert_eq!(constraint_for(&over("flood")), "make_them_drive");
        assert_eq!(constraint_for(&over("quick_game")), "make_them_drive");
        assert_eq!(constraint_for(&over("heavy")), "spread");
        assert_eq!(constraint_for(&over("mesh")), "play_sequencing");
        assert_eq!(
            constraint_for(&over("unmapped_doctrine")),
            "play_sequencing"
        );
    }

    #[test]
    fn recommend_next_counters_the_overreaction_and_keeps_the_plan_otherwise() {
        let log = history(&[
            ("heavy", true),
            ("spread", true),
            ("spread", false),
            ("spread", false),
        ]);
        // The planned call is the overcommitted tactic: its constraint fires.
        assert_eq!(recommend_next(&log, "spread", 4, 3), "bunch");
        // The plan already moves off the dominant tactic: keep the plan.
        assert_eq!(recommend_next(&log, "flood", 4, 3), "flood");
        // No overreaction anywhere: keep the plan.
        let calm = history(&[("spread", true), ("flood", true)]);
        assert_eq!(recommend_next(&calm, "spread", 4, 3), "spread");
        // The counter for a mapped dominant tactic resolves through the table.
        let heavy = history(&[("heavy", false), ("heavy", false), ("heavy", false)]);
        assert_eq!(recommend_next(&heavy, "heavy", 3, 3), "spread");
    }

    #[test]
    fn history_round_trips_through_serde_and_refuses_unknown_fields() {
        let log = history(&[("spread", true), ("heavy", false)]);
        let json = serde_json::to_string(&log).expect("history serialises");
        let round: TacticalHistory = serde_json::from_str(&json).expect("history deserialises");
        assert_eq!(round, log);
        let crowded = r#"{"tactic":"spread","success":true,"ytg_before_milli":10000,"ytg_after_milli":6000,"at_ms":0,"surprise":1}"#;
        assert!(serde_json::from_str::<HistoryRow>(crowded).is_err());
    }

    #[test]
    fn the_same_history_always_recommends_the_same_next_call() {
        let log = history(&[
            ("heavy", true),
            ("spread", true),
            ("spread", false),
            ("spread", false),
        ]);
        assert_eq!(
            recommend_next(&log, "spread", 4, 3),
            recommend_next(&log, "spread", 4, 3)
        );
        assert_eq!(recommend_next(&log, "spread", 4, 3), "bunch");
        assert_eq!(
            detect_overreaction(&log, 4, 3),
            detect_overreaction(&log, 4, 3)
        );
        assert!(log.monotonic_time());
    }
}
