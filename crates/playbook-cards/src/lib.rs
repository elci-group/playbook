//! The NFL-playbook visualiser.
//!
//! Every tactic in the repertoire gets a play card: an X-and-O alignment on
//! a chalk-green field with yard lines, a marked line of scrimmage, and
//! PLAYBOOK/EVIDENCE end zones. Routes follow the coaching tree — animated
//! pass routes, thick runs, dashed pre-snap motion, hatched coverage, and
//! heavy pulsing rush arrows for the Blitz — and every glyph carries its
//! dynamic label: the domain it owns, the scope it holds, the corroboration
//! policy it plays under. Each card's footer is a 3form event line, so the
//! gallery doubles as a HUD feed.
//!
//! The cards are data (`spec`, `repertoire`), the rendering is generic
//! (`svg`, `html`), and everything is deterministic: the same repertoire
//! always renders the same bytes.

#![forbid(unsafe_code)]

pub mod gallery;
pub mod html;
pub mod repertoire;
pub mod spec;
pub mod svg;

pub use gallery::{render_gallery, threeform_lines};
pub use repertoire::{all_cards, card};
pub use spec::{slot, CardClass, Glyph, PlaySpec, Role, Route, RouteKind, Side, Slot};
pub use svg::render_card;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_render_deterministically() {
        let first = render_card(&card("spread").unwrap(), "spread");
        let second = render_card(&card("spread").unwrap(), "spread");
        assert_eq!(first, second);
        assert!(first.contains("<svg"));
        assert!(first.contains("class=\"glyph-o"));
        assert!(first.contains("class=\"glyph-qb"));
        assert!(
            first.contains("animated-route"),
            "routes animate via the page stylesheet"
        );
        assert!(first.contains("event=play name=spread"));
    }

    #[test]
    fn defense_cards_draw_x_glyphs() {
        let blitz = render_card(&card("blitz").unwrap(), "blitz");
        assert!(blitz.contains("class=\"glyph-x"));
        assert!(blitz.contains("route-rush"));
        assert!(blitz.contains("corroboration=first_verified"));
    }

    #[test]
    fn every_card_carries_a_threeform_footer() {
        for spec in all_cards() {
            let rendered = render_card(&spec, &spec.name);
            assert!(
                rendered.contains("event=play"),
                "{} is missing its 3form footer",
                spec.name
            );
            let first_word = spec
                .coaching_point
                .split_whitespace()
                .next()
                .unwrap_or_default();
            assert!(
                rendered.contains(first_word),
                "{} is missing its coaching point",
                spec.name
            );
        }
    }
}
