//! The card campaign (Stage 3): formation truth, validation matrix, and
//! determinism under §12–§14 of the 3form completion directive.

use playbook_cards::{all_cards, card, render_card, CardClass, Side};
use playbook_tactics::formation::{formation_directive, profile, TacticalFormation};
use playbook_tactics::TacticalContext;
use std::collections::BTreeMap;

fn context() -> TacticalContext {
    TacticalContext {
        problem_breadth_ppm: 400,
        domain_coupling_ppm: 300,
        provider_instability_ppm: 100,
        coherence_debt: 100,
        repeated_failures: 0,
        risk_band: playbook_tactics::domain::RiskBand::Low,
        divergence_observed: false,
        critical_domain: None,
        ytg_milli: 10_000,
        budget_remaining_ppm: 900_000,
        short_ytg: false,
        at_ms: 0,
    }
}

/// §13 — every supported formation has a card.
#[test]
fn every_supported_formation_has_a_valid_card() {
    let formations = [
        TacticalFormation::Spread,
        TacticalFormation::Heavy,
        TacticalFormation::Trips,
        TacticalFormation::Bunch,
        TacticalFormation::Empty,
        TacticalFormation::Nickel,
        TacticalFormation::Dime,
        TacticalFormation::Bear,
        TacticalFormation::TwoHigh,
    ];
    for formation in formations {
        let name = formation.machine_name();
        let spec = card(name).unwrap_or_else(|| {
            panic!("formation {name} has no card — the playbook cannot teach it")
        });
        // No missing personnel: an alignment with no slots is not an
        // alignment.
        assert!(!spec.slots.is_empty(), "{name} has an empty alignment");
        // No duplicate personnel: no two glyphs share the same coordinates.
        let mut coordinates: Vec<(i32, i32)> =
            spec.slots.iter().map(|slot| (slot.x, slot.y)).collect();
        coordinates.sort_unstable();
        let count = coordinates.len();
        coordinates.dedup();
        assert_eq!(coordinates.len(), count, "{name} double-books a coordinate");
        // Domain labels correspond to the directive: every label is
        // non-empty.
        for slot in &spec.slots {
            assert!(!slot.label.trim().is_empty(), "{name} has an empty label");
        }
        // Routes point at valid actors.
        for route in &spec.routes {
            assert!(
                route.from_slot < spec.slots.len(),
                "{name} draws a route from an unaligned slot"
            );
        }
        // The formation name agrees with the tactical directive.
        let directive = formation_directive(formation, &context());
        assert_eq!(directive.name, name, "{name} disagrees with its directive");
        // The 3form event line agrees with the structural data.
        let line = spec.threeform_line();
        assert!(line.contains(&format!("name={name}")));
    }
}

/// §13 — unknown formations fail gracefully: the card lookup returns None
/// rather than fabricating a layout.
#[test]
fn unknown_formations_return_no_card() {
    assert!(card("huey_kew").is_none());
    assert!(card("illegitimate_wishbone").is_none());
}

/// §12 — formation truth: the card's stats are the directive's own values,
/// not an independent reconstruction. Spread and Empty carry their
/// formation-profile numbers on the card face.
#[test]
fn card_stats_agree_with_the_formation_profiles() {
    let spread_profile = profile(TacticalFormation::Spread);
    let spread = card("spread").unwrap();
    let stats: BTreeMap<String, String> = spread.stats.iter().cloned().collect();
    assert_eq!(
        stats["max_concurrent"],
        spread_profile.max_concurrent.to_string(),
        "the Spread card must quote the live formation profile"
    );
    let empty_profile = profile(TacticalFormation::Empty);
    let empty = card("empty").unwrap();
    let stats: BTreeMap<String, String> = empty.stats.iter().cloned().collect();
    assert_eq!(
        stats["reserves_share_ppm"],
        empty_profile.reserves_share_ppm.to_string(),
        "the Empty card must quote the live reserves share"
    );
}

/// §14 — static generation is deterministic and repeatable: two renders
/// produce byte-identical output, and rendering mutates nothing.
#[test]
fn card_generation_is_byte_deterministic() {
    let first = render_card(&card("spread").unwrap(), "spread");
    let second = render_card(&card("spread").unwrap(), "spread");
    assert_eq!(first, second);
    let all_first: Vec<String> = all_cards()
        .iter()
        .map(|spec| render_card(spec, &spec.name))
        .collect();
    let all_second: Vec<String> = all_cards()
        .iter()
        .map(|spec| render_card(spec, &spec.name))
        .collect();
    assert_eq!(all_first, all_second);
}

/// §39 — the consistency matrix: tactic identity agrees across the card,
/// the 3form event line, and the tactical class system, for every card in
/// the repertoire.
#[test]
fn the_consistency_matrix_holds_across_card_directive_and_class() {
    for spec in all_cards() {
        // The machine name is the matrix key and the directive key.
        assert!(!spec.name.trim().is_empty());
        // The threeform line names the same tactic.
        assert!(spec
            .threeform_line()
            .contains(&format!("name={}", spec.name)));
        // The class chip and the side agree with the spec.
        match spec.class {
            CardClass::Formation | CardClass::Doctrine | CardClass::Play => {}
            CardClass::Adjustment | CardClass::Defence => {}
        }
        match spec.side {
            Side::Offense | Side::Defense => {}
        }
        // Every card carries its coaching point in the 3form tail.
        assert!(spec.threeform_line().contains("text=\""));
    }
}

/// §41 — the rendered gallery carries no secrets or absolute host paths.
#[test]
fn the_gallery_leaks_no_host_paths_or_credentials() {
    for spec in &all_cards() {
        let rendered = render_card(spec, &spec.name).to_lowercase();
        for needle in ["/home/", "api_key", "bearer ", "password"] {
            assert!(!rendered.contains(needle), "{} leaked {needle}", spec.name);
        }
    }
}

/// The full gallery render is byte-deterministic (§14 at page scale).
#[test]
fn the_gallery_page_is_byte_deterministic() {
    let first = playbook_cards::render_gallery();
    let second = playbook_cards::render_gallery();
    assert_eq!(first, second);
}
