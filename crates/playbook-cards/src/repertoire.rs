//! The play repertoire: every card in the playbook, as data.
//!
//! These are the X-and-O templates. Offense draws O's (the quarterback is a
//! rotated square), defence draws X's; routes follow the coaching-tree
//! conventions (solid pass routes with arrowheads, thick runs, dashed motion,
//! hatched coverage, heavy pulsing rush). The dynamic labels name what the
//! card's personnel actually own in Playbook's vocabulary — domains, scopes,
//! corroboration policies — pulled from the tactical repertoire's real
//! values, so the cards teach the system, not just the football.

use crate::spec::slot::{defense, offense, quarterback, receiver};
use crate::spec::{CardClass, PlaySpec, Role, Route, RouteKind, Side, Slot};

fn qb() -> Slot {
    quarterback(50, 42)
}

/// The offensive card set plus the defensive cards, in playbook order.
pub fn all_cards() -> Vec<PlaySpec> {
    vec![
        spread(),
        trips(),
        bunch(),
        empty(),
        heavy(),
        mesh(),
        flood(),
        levels(),
        verticals(),
        quick_game(),
        motion(),
        audible(),
        max_protect(),
        prevent(),
        make_them_drive(),
        blitz(),
        spy(),
        bracket(),
        bear(),
        two_high(),
    ]
}

/// Look up one card by machine name.
pub fn card(name: &str) -> Option<PlaySpec> {
    all_cards().into_iter().find(|spec| spec.name == name)
}

fn spread() -> PlaySpec {
    PlaySpec {
        name: "spread".into(),
        title: "SPREAD".into(),
        side: Side::Offense,
        class: CardClass::Formation,
        coaching_point: "broad parallel ownership — one sole owner per domain, minimal overlap"
            .into(),
        slots: vec![
            qb(),
            receiver(15, 52, "domain-a"),
            receiver(32, 55, "domain-b"),
            receiver(68, 55, "domain-c"),
            receiver(85, 52, "domain-d"),
            offense(50, 40, Role::Runner),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(15, 58), (12, 75), (10, 92)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 2,
                points: vec![(32, 62), (35, 80), (33, 94)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 3,
                points: vec![(68, 62), (65, 80), (67, 94)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 4,
                points: vec![(85, 58), (88, 75), (90, 92)],
                kind: RouteKind::Pass,
            },
        ],
        stats: vec![
            ("max_concurrent".into(), "8".into()),
            ("overlap_ppm".into(), "50".into()),
            ("independence".into(), "sole_domain".into()),
        ],
    }
}

fn trips() -> PlaySpec {
    PlaySpec {
        name: "trips".into(),
        title: "TRIPS".into(),
        side: Side::Offense,
        class: CardClass::Formation,
        coaching_point: "numerical superiority on the critical domain — three owners, one blocker"
            .into(),
        slots: vec![
            qb(),
            receiver(78, 52, "critical"),
            receiver(84, 58, "critical"),
            receiver(90, 54, "critical"),
            receiver(18, 52, "coverage"),
            offense(50, 40, Role::Runner),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(78, 58), (74, 78), (76, 94)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 2,
                points: vec![(84, 64), (84, 82), (84, 94)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 3,
                points: vec![(90, 60), (94, 76), (92, 92)],
                kind: RouteKind::Pass,
            },
        ],
        stats: vec![
            ("concentration".into(), "3_on_critical".into()),
            ("remainder".into(), "global".into()),
        ],
    }
}

fn bunch() -> PlaySpec {
    PlaySpec {
        name: "bunch".into(),
        title: "BUNCH".into(),
        side: Side::Offense,
        class: CardClass::Formation,
        coaching_point: "cooperative micro-team — shared context, bounded huddle access".into(),
        slots: vec![
            qb(),
            receiver(72, 52, "shared"),
            receiver(79, 57, "shared"),
            receiver(76, 63, "shared"),
            receiver(22, 52, "isolated"),
            offense(50, 40, Role::Runner),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(70, 58), (60, 74), (50, 88)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 2,
                points: vec![(79, 63), (74, 80), (68, 92)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 3,
                points: vec![(76, 69), (82, 82), (86, 92)],
                kind: RouteKind::Pass,
            },
        ],
        stats: vec![
            ("overlap_ppm".into(), "900".into()),
            ("huddle_access".into(), "full".into()),
        ],
    }
}

fn empty() -> PlaySpec {
    PlaySpec {
        name: "empty".into(),
        title: "EMPTY".into(),
        side: Side::Offense,
        class: CardClass::Formation,
        coaching_point: "every seat on execution — no reserve, no protection".into(),
        slots: vec![
            qb(),
            receiver(12, 52, "exec"),
            receiver(30, 56, "exec"),
            receiver(48, 58, "exec"),
            receiver(66, 56, "exec"),
            receiver(86, 52, "exec"),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(12, 58), (10, 76), (12, 92)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 3,
                points: vec![(48, 64), (48, 82), (48, 94)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 5,
                points: vec![(86, 58), (88, 76), (86, 92)],
                kind: RouteKind::Pass,
            },
        ],
        stats: vec![
            ("reserves_share_ppm".into(), "0".into()),
            ("counter".into(), "max_protect".into()),
        ],
    }
}

fn heavy() -> PlaySpec {
    PlaySpec {
        name: "heavy".into(),
        title: "HEAVY".into(),
        side: Side::Offense,
        class: CardClass::Formation,
        coaching_point: "one primary executor wrapped in deterministic validation".into(),
        slots: vec![
            qb(),
            offense(50, 34, Role::Runner),
            offense(40, 48, Role::Blocker),
            offense(50, 48, Role::Blocker),
            offense(60, 48, Role::Blocker),
            offense(30, 44, Role::Validator),
            offense(70, 44, Role::Validator),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(50, 28), (46, 18), (52, 8)],
                kind: RouteKind::Run,
            },
            Route {
                from_slot: 2,
                points: vec![(40, 43), (38, 55), (42, 60)],
                kind: RouteKind::Block,
            },
            Route {
                from_slot: 4,
                points: vec![(60, 43), (62, 55), (58, 60)],
                kind: RouteKind::Block,
            },
        ],
        stats: vec![
            ("executors".into(), "1".into()),
            ("protection_delta_ppm".into(), "800".into()),
        ],
    }
}

fn mesh() -> PlaySpec {
    PlaySpec {
        name: "mesh".into(),
        title: "MESH".into(),
        side: Side::Offense,
        class: CardClass::Play,
        coaching_point: "crossing traversals expose contradictory assumptions — the crossing point is the reconciliation target".into(),
        slots: vec![
            qb(),
            receiver(20, 52, "api→impl"),
            receiver(80, 52, "tests→impl"),
            offense(50, 40, Role::Runner),
        ],
        routes: vec![
            Route { from_slot: 1, points: vec![(22, 56), (40, 66), (58, 66), (76, 56)], kind: RouteKind::Pass },
            Route { from_slot: 2, points: vec![(78, 56), (60, 60), (42, 60), (24, 56)], kind: RouteKind::Pass },
        ],
        stats: vec![
            ("reconciliation_target".into(), "crossing".into()),
            ("information_delta_ppm".into(), "800".into()),
        ],
    }
}

fn flood() -> PlaySpec {
    PlaySpec {
        name: "flood".into(),
        title: "FLOOD".into(),
        side: Side::Offense,
        class: CardClass::Play,
        coaching_point:
            "attack one domain at every abstraction depth — find the layer that holds the blocker"
                .into(),
        slots: vec![
            qb(),
            receiver(70, 52, "architecture"),
            receiver(76, 58, "module"),
            receiver(82, 64, "function"),
            receiver(88, 70, "test"),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(70, 58), (68, 76), (68, 94)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 2,
                points: vec![(76, 64), (78, 78), (80, 94)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 3,
                points: vec![(82, 70), (88, 80), (94, 90)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 4,
                points: vec![(88, 76), (94, 84), (98, 92)],
                kind: RouteKind::Pass,
            },
        ],
        stats: vec![
            ("depths".into(), "5".into()),
            ("domain".into(), "single".into()),
        ],
    }
}

fn levels() -> PlaySpec {
    PlaySpec {
        name: "levels".into(),
        title: "LEVELS".into(),
        side: Side::Offense,
        class: CardClass::Play,
        coaching_point: "diagnostic depth — the same question at progressively finer granularity"
            .into(),
        slots: vec![
            qb(),
            receiver(35, 56, "system"),
            receiver(45, 64, "crate"),
            receiver(55, 72, "function"),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(35, 62), (35, 76), (35, 90)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 2,
                points: vec![(45, 70), (45, 82), (45, 94)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 3,
                points: vec![(55, 78), (55, 88), (55, 96)],
                kind: RouteKind::Pass,
            },
        ],
        stats: vec![
            ("granularity".into(), "progressive".into()),
            ("mode".into(), "diagnostic".into()),
        ],
    }
}

fn verticals() -> PlaySpec {
    PlaySpec {
        name: "verticals".into(),
        title: "VERTICALS".into(),
        side: Side::Offense,
        class: CardClass::Play,
        coaching_point: "high-upside architectural strikes — carries increased budget risk".into(),
        slots: vec![
            qb(),
            receiver(25, 52, "redesign-a"),
            receiver(50, 54, "redesign-b"),
            receiver(75, 52, "redesign-c"),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(25, 58), (25, 78), (25, 96)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 2,
                points: vec![(50, 60), (50, 80), (50, 96)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 3,
                points: vec![(75, 58), (75, 78), (75, 96)],
                kind: RouteKind::Pass,
            },
        ],
        stats: vec![
            ("budget_risk".into(), "elevated".into()),
            ("upside".into(), "collapse_ytg".into()),
        ],
    }
}

fn quick_game() -> PlaySpec {
    PlaySpec {
        name: "quick_game".into(),
        title: "QUICK GAME".into(),
        side: Side::Offense,
        class: CardClass::Play,
        coaching_point:
            "cheap low-latency personnel on bounded actions with deterministic validation".into(),
        slots: vec![
            qb(),
            receiver(30, 52, "rename"),
            receiver(70, 52, "lint-fix"),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(30, 56), (26, 66), (22, 72)],
                kind: RouteKind::Pass,
            },
            Route {
                from_slot: 2,
                points: vec![(70, 56), (74, 66), (78, 72)],
                kind: RouteKind::Pass,
            },
        ],
        stats: vec![
            ("latency".into(), "low".into()),
            ("validation".into(), "deterministic".into()),
        ],
    }
}

fn motion() -> PlaySpec {
    PlaySpec {
        name: "motion".into(),
        title: "MOTION".into(),
        side: Side::Offense,
        class: CardClass::Adjustment,
        coaching_point:
            "probe before the snap — reveal coupling, availability, bottlenecks; diagnostic only"
                .into(),
        slots: vec![
            qb(),
            receiver(35, 52, "probe:api"),
            receiver(65, 52, "probe:tests"),
        ],
        routes: vec![Route {
            from_slot: 1,
            points: vec![(35, 52), (48, 52), (60, 52)],
            kind: RouteKind::Motion,
        }],
        stats: vec![
            ("max_tokens".into(), "500".into()),
            ("max_duration_ms".into(), "2000".into()),
        ],
    }
}

fn audible() -> PlaySpec {
    PlaySpec {
        name: "audible".into(),
        title: "AUDIBLE".into(),
        side: Side::Offense,
        class: CardClass::Adjustment,
        coaching_point:
            "replace the called play on observed state — before irreversible execution, never after"
                .into(),
        slots: vec![
            qb(),
            receiver(50, 58, "called:spread"),
            receiver(50, 44, "audible:bunch"),
        ],
        routes: vec![Route {
            from_slot: 2,
            points: vec![(50, 50), (50, 54)],
            kind: RouteKind::Motion,
        }],
        stats: vec![
            ("window".into(), "pre_dispatch".into()),
            ("after_snap".into(), "keep_plan".into()),
        ],
    }
}

fn max_protect() -> PlaySpec {
    PlaySpec {
        name: "max_protect".into(),
        title: "MAX PROTECT".into(),
        side: Side::Offense,
        class: CardClass::Doctrine,
        coaching_point:
            "verification, rollback, state protection and observation around one executor".into(),
        slots: vec![
            qb(),
            offense(50, 46, Role::Blocker),
            offense(38, 46, Role::Blocker),
            offense(28, 40, Role::Validator),
            offense(62, 40, Role::Validator),
            offense(50, 30, Role::Runner),
            defense(20, 60, Role::Observer, "OBS"),
        ],
        routes: vec![Route {
            from_slot: 6,
            points: vec![(24, 64), (30, 72), (36, 78)],
            kind: RouteKind::Coverage,
        }],
        stats: vec![
            ("validators".into(), "2+".into()),
            ("observers".into(), "1+".into()),
        ],
    }
}

fn prevent() -> PlaySpec {
    PlaySpec {
        name: "prevent".into(),
        title: "PREVENT".into(),
        side: Side::Defense,
        class: CardClass::Doctrine,
        coaching_point: "sacrifice short gains — block every large unverified change".into(),
        slots: vec![
            defense(30, 70, Role::Safety, "S"),
            defense(50, 74, Role::Safety, "S"),
            defense(70, 70, Role::Safety, "S"),
            defense(50, 56, Role::Linebacker, "CAP"),
        ],
        routes: vec![Route {
            from_slot: 3,
            points: vec![(50, 52), (50, 62)],
            kind: RouteKind::Coverage,
        }],
        stats: vec![
            ("max_files".into(), "3".into()),
            ("max_lines".into(), "100".into()),
        ],
    }
}

fn make_them_drive() -> PlaySpec {
    PlaySpec {
        name: "make_them_drive".into(),
        title: "MAKE THEM DRIVE".into(),
        side: Side::Defense,
        class: CardClass::Doctrine,
        coaching_point: "many small validated stages — never one large patch".into(),
        slots: vec![
            defense(35, 66, Role::Corner, "ST1"),
            defense(50, 66, Role::Corner, "ST2"),
            defense(65, 66, Role::Corner, "ST3"),
            defense(50, 56, Role::Linebacker, "VAL"),
        ],
        routes: vec![],
        stats: vec![
            ("staged".into(), "true".into()),
            ("validation".into(), "per_stage".into()),
        ],
    }
}

fn blitz() -> PlaySpec {
    PlaySpec {
        name: "blitz".into(),
        title: "BLITZ".into(),
        side: Side::Defense,
        class: CardClass::Defence,
        coaching_point: "independent fan-out, first verified recovery wins, the rest cancel".into(),
        slots: vec![
            defense(25, 60, Role::Linebacker, "W1"),
            defense(42, 62, Role::Linebacker, "W2"),
            defense(58, 62, Role::Linebacker, "W3"),
            defense(75, 60, Role::Linebacker, "W4"),
            defense(50, 74, Role::Safety, "VER"),
        ],
        routes: vec![
            Route {
                from_slot: 0,
                points: vec![(25, 54), (28, 52), (34, 51)],
                kind: RouteKind::Rush,
            },
            Route {
                from_slot: 1,
                points: vec![(42, 56), (44, 52), (46, 51)],
                kind: RouteKind::Rush,
            },
            Route {
                from_slot: 2,
                points: vec![(58, 56), (56, 52), (54, 51)],
                kind: RouteKind::Rush,
            },
            Route {
                from_slot: 3,
                points: vec![(75, 54), (72, 52), (66, 51)],
                kind: RouteKind::Rush,
            },
        ],
        stats: vec![
            ("corroboration".into(), "first_verified".into()),
            ("cancel_after_win".into(), "true".into()),
        ],
    }
}

fn spy() -> PlaySpec {
    PlaySpec {
        name: "spy".into(),
        title: "SPY".into(),
        side: Side::Defense,
        class: CardClass::Defence,
        coaching_point: "an observer assigned to a high-risk runner — it watches, never intervenes"
            .into(),
        slots: vec![
            defense(50, 58, Role::Spy, "SPY"),
            offense(50, 44, Role::Runner),
            defense(30, 70, Role::Safety, "S"),
            defense(70, 70, Role::Safety, "S"),
        ],
        routes: vec![Route {
            from_slot: 0,
            points: vec![(50, 52), (50, 48)],
            kind: RouteKind::Coverage,
        }],
        stats: vec![
            ("intervene".into(), "recommend_only".into()),
            ("observe_only".into(), "true".into()),
        ],
    }
}

fn bracket() -> PlaySpec {
    PlaySpec {
        name: "bracket".into(),
        title: "BRACKET".into(),
        side: Side::Defense,
        class: CardClass::Defence,
        coaching_point:
            "two different controls over one critical domain — deterministic and generative".into(),
        slots: vec![
            offense(50, 60, Role::Receiver),
            defense(44, 70, Role::Corner, "DET"),
            defense(56, 70, Role::Corner, "GEN"),
        ],
        routes: vec![
            Route {
                from_slot: 1,
                points: vec![(46, 66), (47, 66)],
                kind: RouteKind::Coverage,
            },
            Route {
                from_slot: 2,
                points: vec![(54, 66), (53, 66)],
                kind: RouteKind::Coverage,
            },
        ],
        stats: vec![
            ("controls".into(), "2_kinds".into()),
            ("domain".into(), "critical".into()),
        ],
    }
}

fn bear() -> PlaySpec {
    PlaySpec {
        name: "bear".into(),
        title: "BEAR".into(),
        side: Side::Defense,
        class: CardClass::Defence,
        coaching_point: "deterministic controls directly over the critical shared core".into(),
        slots: vec![
            defense(42, 58, Role::Linebacker, "CORE"),
            defense(50, 58, Role::Linebacker, "CORE"),
            defense(58, 58, Role::Linebacker, "CORE"),
            defense(30, 70, Role::Corner, "CB"),
            defense(70, 70, Role::Corner, "CB"),
        ],
        routes: vec![],
        stats: vec![
            ("coverage".into(), "central_core".into()),
            ("remediation".into(), "instinctive".into()),
        ],
    }
}

fn two_high() -> PlaySpec {
    PlaySpec {
        name: "two_high".into(),
        title: "TWO-HIGH".into(),
        side: Side::Defense,
        class: CardClass::Defence,
        coaching_point: "two independent reserves held deep for divergent threat classes".into(),
        slots: vec![
            defense(30, 78, Role::Safety, "RES:A"),
            defense(70, 78, Role::Safety, "RES:B"),
            defense(40, 58, Role::Corner, "CB"),
            defense(60, 58, Role::Corner, "CB"),
        ],
        routes: vec![],
        stats: vec![
            ("reserves_share_ppm".into(), "600".into()),
            ("threat_classes".into(), "2".into()),
        ],
    }
}
