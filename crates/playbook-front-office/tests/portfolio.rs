use playbook_core::{PersonnelId, PlaybookError};
use playbook_front_office::*;

fn input() -> PortfolioInput {
    serde_json::from_str(include_str!("../../../fixtures/front_office.json")).unwrap()
}

#[test]
fn complementary_depth_beats_an_expensive_correlated_duplicate() {
    let input = input();
    let contract = construct(&input).unwrap();
    contract.verify().unwrap();
    assert_eq!(
        contract
            .plan
            .members
            .iter()
            .map(|m| m.personnel_id.as_str())
            .collect::<Vec<_>>(),
        ["cargo-toolchain", "coding-specialist"]
    );
    assert_eq!(contract.plan.projected_cost_micros_per_workload, 120);
    assert_eq!(contract.plan.diversification_ppm, 1_000_000);
    assert_eq!(
        contract.plan.ineligible[0].reason,
        ExclusionReason::MarginalRosterValue
    );
    let mut reversed = input;
    reversed.candidates.reverse();
    assert_eq!(
        construct(&reversed).unwrap().plan.members,
        contract.plan.members
    );
}

#[test]
fn hard_constraints_reject_portfolios_without_real_fallback_coverage() {
    let mut input = input();
    input.budget_micros_per_workload = 119;
    assert!(construct(&input).is_err());
    input.budget_micros_per_workload = 200;
    input.require_offline_fallback = true;
    assert!(construct(&input).is_ok());
    input.candidates.iter_mut().for_each(|c| c.offline = false);
    assert!(construct(&input).is_err());
    input.require_offline_fallback = false;
    input.candidates[2].tools.clear();
    input.candidates[1].tools.clear();
    assert!(construct(&input).is_err()); // Every formation requires depth two.
}

#[test]
fn same_season_results_and_live_task_payloads_are_rejected() {
    let mut input = input();
    input.candidates[0].history[0].through_season = input.effective_season;
    assert!(construct(&input).is_err());
    input.candidates[0].history[0].through_season = 1;
    input.evidence_through_season = input.effective_season;
    assert!(construct(&input).is_err());
    let mut raw = serde_json::to_value(input).unwrap();
    raw["repository"] = serde_json::json!("/home/sal/playbook");
    assert!(serde_json::from_value::<PortfolioInput>(raw).is_err());
    let profile = r#"{"task_class":"fix this exact bug","weight_ppm":1000000}"#;
    assert!(serde_json::from_str::<OpponentProfile>(profile).is_err());
}

#[test]
fn reliability_quota_context_and_history_limits_cannot_be_bypassed_by_self_reports() {
    let mut input = input();
    input.candidates[1].history.clear();
    input.candidates[2].reliability_ppm = 0;
    assert!(construct(&input).is_err());
    input.candidates[2].reliability_ppm = 990000;
    input.candidates[2].quota_workloads = 0;
    assert!(construct(&input).is_err());
    input.candidates[2].quota_workloads = 1;
    input.candidates[2].context_tokens = 100;
    assert!(construct(&input).is_err());
    input.candidates[2].context_tokens = 128000;
    input.candidates[2]
        .history
        .iter_mut()
        .for_each(|h| h.samples = 0);
    assert!(construct(&input).is_err());
}

#[test]
fn contract_tampering_and_overflowing_roster_costs_are_rejected() {
    let mut input = input();
    let mut contract = construct(&input).unwrap();
    contract.plan.members[0].personnel_id = PersonnelId::new("unselected").unwrap();
    assert_eq!(contract.verify(), Err(PlaybookError::DigestMismatch));
    input.budget_micros_per_workload = u64::MAX;
    input
        .candidates
        .iter_mut()
        .for_each(|c| c.cost_micros_per_workload = u64::MAX);
    assert!(construct(&input).is_err());
}
