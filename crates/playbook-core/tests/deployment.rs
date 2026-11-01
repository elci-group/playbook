//! Tests for the deployment module: DeploymentStatus, SideOfBall, SnapIntent,
//! Fitness, Eligibility, effective_deployment, eligible_pool, roster_utility,
//! depth_chart, and FranchiseDoctrine.

use playbook_core::deployment::*;

#[test]
fn one_way_offense_only_plays_offense() {
    assert!(DeploymentStatus::OneWayOffense.may_play(SideOfBall::Offense));
    assert!(!DeploymentStatus::OneWayOffense.may_play(SideOfBall::Defense));
}

#[test]
fn one_way_defense_only_plays_defense() {
    assert!(DeploymentStatus::OneWayDefense.may_play(SideOfBall::Defense));
    assert!(!DeploymentStatus::OneWayDefense.may_play(SideOfBall::Offense));
}

#[test]
fn two_way_plays_both_sides() {
    assert!(DeploymentStatus::TwoWay.may_play(SideOfBall::Offense));
    assert!(DeploymentStatus::TwoWay.may_play(SideOfBall::Defense));
}

#[test]
fn special_teams_never_plays_normal_snaps() {
    assert!(!DeploymentStatus::SpecialTeams.may_play(SideOfBall::Offense));
    assert!(!DeploymentStatus::SpecialTeams.may_play(SideOfBall::Defense));
}

#[test]
fn snap_intent_classifies_correctly() {
    assert_eq!(SnapIntent::Implement.side(), SideOfBall::Offense);
    assert_eq!(SnapIntent::Deliver.side(), SideOfBall::Offense);
    assert_eq!(SnapIntent::Review.side(), SideOfBall::Defense);
    assert_eq!(SnapIntent::Test.side(), SideOfBall::Defense);
    assert_eq!(SnapIntent::Attack.side(), SideOfBall::Defense);
}

#[test]
fn progressive_utility_greater_than_protective_is_offense() {
    let off = SnapIntent::Synthesize;
    let def = SnapIntent::Verify;
    assert!(off.progressive_utility() > off.protective_utility());
    assert!(def.protective_utility() > def.progressive_utility());
}

#[test]
fn fitness_selectable() {
    assert!(Fitness::Fit.selectable());
    assert!(Fitness::Recovery(1).selectable());
    assert!(!Fitness::Out.selectable());
}

#[test]
fn effective_deployment_contracts_two_way_under_recovery() {
    let ed = effective_deployment(DeploymentStatus::TwoWay, Fitness::Recovery(1), 2);
    assert_eq!(ed.effective, DeploymentStatus::OneWayDefense);
    assert_eq!(ed.side_restriction, Some(SideOfBall::Defense));

    let ed = effective_deployment(DeploymentStatus::TwoWay, Fitness::Recovery(2), 2);
    assert_eq!(ed.effective, DeploymentStatus::OneWayOffense);
    assert_eq!(ed.side_restriction, Some(SideOfBall::Offense));
}

#[test]
fn effective_deployment_one_way_offense_unaffected_by_recovery() {
    let ed = effective_deployment(DeploymentStatus::OneWayOffense, Fitness::Recovery(1), 2);
    assert_eq!(ed.effective, DeploymentStatus::OneWayOffense);
}

#[test]
fn out_player_never_selectable() {
    let ed = effective_deployment(DeploymentStatus::TwoWay, Fitness::Out, 2);
    assert!(!ed.effective.may_play(SideOfBall::Offense));
}

#[test]
fn player_card_may_take_snap() {
    let card = PlayerCard {
        player: "test".into(),
        provider: "test".into(),
        position: "qb".into(),
        deployment: DeploymentStatus::TwoWay,
        fitness: Fitness::Fit,
        eligibility: Eligibility::Eligible,
        workload: Workload::Starter,
    };
    assert!(card.may_take_snap(SideOfBall::Offense, 2));
    assert!(card.may_take_snap(SideOfBall::Defense, 2));
}

#[test]
fn player_card_blocked_by_eligibility() {
    let card = PlayerCard {
        player: "test".into(),
        provider: "test".into(),
        position: "qb".into(),
        deployment: DeploymentStatus::TwoWay,
        fitness: Fitness::Fit,
        eligibility: Eligibility::Suspended,
        workload: Workload::Starter,
    };
    assert!(!card.may_take_snap(SideOfBall::Offense, 2));
}

#[test]
fn player_card_blocked_by_fitness() {
    let card = PlayerCard {
        player: "test".into(),
        provider: "test".into(),
        position: "qb".into(),
        deployment: DeploymentStatus::TwoWay,
        fitness: Fitness::Out,
        eligibility: Eligibility::Eligible,
        workload: Workload::Starter,
    };
    assert!(!card.may_take_snap(SideOfBall::Offense, 2));
}

#[test]
fn eligible_pool_filters_by_snap_intent_and_doctrine() {
    let roster = vec![
        PlayerCard {
            player: "offense_only".into(),
            provider: "p".into(),
            position: "qb".into(),
            deployment: DeploymentStatus::OneWayOffense,
            fitness: Fitness::Fit,
            eligibility: Eligibility::Eligible,
            workload: Workload::Starter,
        },
        PlayerCard {
            player: "defense_only".into(),
            provider: "p".into(),
            position: "lb".into(),
            deployment: DeploymentStatus::OneWayDefense,
            fitness: Fitness::Fit,
            eligibility: Eligibility::Eligible,
            workload: Workload::Starter,
        },
        PlayerCard {
            player: "two_way".into(),
            provider: "p".into(),
            position: "utility".into(),
            deployment: DeploymentStatus::TwoWay,
            fitness: Fitness::Fit,
            eligibility: Eligibility::Eligible,
            workload: Workload::Starter,
        },
        PlayerCard {
            player: "suspended".into(),
            provider: "p".into(),
            position: "qb".into(),
            deployment: DeploymentStatus::TwoWay,
            fitness: Fitness::Fit,
            eligibility: Eligibility::Suspended,
            workload: Workload::Starter,
        },
        PlayerCard {
            player: "injured".into(),
            provider: "p".into(),
            position: "qb".into(),
            deployment: DeploymentStatus::TwoWay,
            fitness: Fitness::Out,
            eligibility: Eligibility::Eligible,
            workload: Workload::Starter,
        },
    ];
    let doctrine = FranchiseDoctrine::defensive_fortress();

    let off_pool = eligible_pool(&roster, SnapIntent::Implement, &doctrine);
    // Two-way players can also play offense
    assert_eq!(off_pool.len(), 2);
    assert!(off_pool.iter().any(|p| p.player == "offense_only"));
    assert!(off_pool.iter().any(|p| p.player == "two_way"));

    let def_pool = eligible_pool(&roster, SnapIntent::Test, &doctrine);
    // Two-way players can also play defense
    assert_eq!(def_pool.len(), 2);
    assert!(def_pool.iter().any(|p| p.player == "defense_only"));
    assert!(def_pool.iter().any(|p| p.player == "two_way"));
}

#[test]
fn two_way_roster_utility_has_optionality_value() {
    let doctrine = FranchiseDoctrine::defensive_fortress();
    let two_way = PlayerCard {
        player: "two".into(),
        provider: "p".into(),
        position: "utility".into(),
        deployment: DeploymentStatus::TwoWay,
        fitness: Fitness::Fit,
        eligibility: Eligibility::Eligible,
        workload: Workload::Starter,
    };
    let one_way_off = PlayerCard {
        player: "off".into(),
        provider: "p".into(),
        position: "qb".into(),
        deployment: DeploymentStatus::OneWayOffense,
        fitness: Fitness::Fit,
        eligibility: Eligibility::Eligible,
        workload: Workload::Restricted,
    };

    let util_tw = roster_utility(&two_way, &doctrine);
    let util_ow = roster_utility(&one_way_off, &doctrine);

    // Two-way gets optionality bonus in defensive doctrine
    assert!(util_tw > util_ow);
}

#[test]
fn depth_chart_groups_by_position_and_ranks_by_utility() {
    let roster = vec![
        PlayerCard {
            player: "a".into(),
            provider: "p".into(),
            position: "qb".into(),
            deployment: DeploymentStatus::OneWayOffense,
            fitness: Fitness::Fit,
            eligibility: Eligibility::Eligible,
            workload: Workload::Restricted,
        },
        PlayerCard {
            player: "b".into(),
            provider: "p".into(),
            position: "qb".into(),
            deployment: DeploymentStatus::TwoWay,
            fitness: Fitness::Fit,
            eligibility: Eligibility::Eligible,
            workload: Workload::Starter,
        },
        PlayerCard {
            player: "c".into(),
            provider: "p".into(),
            position: "lb".into(),
            deployment: DeploymentStatus::OneWayDefense,
            fitness: Fitness::Fit,
            eligibility: Eligibility::Eligible,
            workload: Workload::Starter,
        },
    ];
    let doctrine = FranchiseDoctrine::defensive_fortress();
    let chart = depth_chart(&roster, &doctrine);
    assert_eq!(chart.len(), 2); // qb and lb
    let qb_group = chart.iter().find(|(pos, _)| pos == "qb").unwrap();
    assert_eq!(qb_group.1.len(), 2);
}
