//! The animated gallery: the whole playbook as one self-contained page.
//!
//! Dark-stadium chrome, chalk-green fields, and CSS-only animation — route
//! marching, player fade-in, rush pulse, motion slide — so the page animates
//! with no JavaScript at all. The header strip carries the 3form event lines
//! for every card, in collection order, ready to pipe at a 3form collector.

use crate::repertoire::all_cards;
use crate::svg::render_card;

/// Render the full animated playbook page.
pub fn render_gallery() -> String {
    let cards = all_cards();
    let mut page = String::with_capacity(64_000);
    page.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    page.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    page.push_str("<title>Playbook — the play card gallery</title>\n");
    page.push_str(&crate::html::styles());
    page.push_str("</head>\n<body>\n");
    page.push_str("<header class=\"stadium-header\">\n<h1>THE PLAYBOOK</h1>\n");
    page.push_str(
        "<p class=\"tagline\">every play is a deterministic directive — same roster, \
         different shape, measured result</p>\n</header>\n",
    );
    page.push_str("<section class=\"threeform-feed\" aria-label=\"3form event feed\">\n");
    page.push_str("<h2>3FORM FEED</h2>\n<pre>\n");
    for card in &cards {
        page.push_str(&format!("{}\n", card.threeform_line()));
    }
    page.push_str("</pre>\n</section>\n");
    page.push_str("<main class=\"card-wall\">\n");
    for card in &cards {
        page.push_str("<article class=\"play-card\">\n");
        page.push_str(&render_card(card, &card.name));
        page.push_str("</article>\n");
    }
    page.push_str("</main>\n");
    page.push_str(
        "<footer class=\"stadium-footer\">PLAYBOOK — deterministic control, \
         evidence-governed victory. Routes animate; the guarantees do not move.</footer>\n",
    );
    page.push_str("</body>\n</html>\n");
    page
}

/// The 3form event lines for the whole repertoire, one per card.
pub fn threeform_lines() -> Vec<String> {
    all_cards()
        .iter()
        .map(|card| card.threeform_line())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gallery_is_self_contained_and_animated() {
        let page = render_gallery();
        assert!(page.starts_with("<!DOCTYPE html>"));
        assert!(page.contains("THE PLAYBOOK"));
        assert!(page.contains("3FORM FEED"));
        assert!(page.contains("event=play name=spread"));
        assert!(page.contains("animated-route"));
        assert!(page.contains("animated-player"));
        assert!(page.contains("route-rush"));
        assert!(page.contains("EVIDENCE"));
        assert!(page.contains("PLAYBOOK"));
        assert!(page.contains("@keyframes route-march"));
        assert!(
            !page.contains("<script"),
            "no javascript: CSS-only animation"
        );
    }

    #[test]
    fn every_repertoire_tactic_has_a_card() {
        let cards = all_cards();
        assert!(
            cards.len() >= 20,
            "the playbook fields {} cards",
            cards.len()
        );
        for card in &cards {
            assert!(!card.slots.is_empty(), "{} has no alignment", card.name);
            assert!(!card.coaching_point.is_empty());
        }
        let mut names: Vec<&str> = cards.iter().map(|card| card.name.as_str()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate card names");
    }

    #[test]
    fn card_lookup_is_exact() {
        assert!(crate::repertoire::card("spread").is_some());
        assert!(crate::repertoire::card("blitz").is_some());
        assert!(crate::repertoire::card("huey_kew").is_none());
    }

    #[test]
    fn threeform_lines_cover_the_repertoire() {
        let lines = threeform_lines();
        assert_eq!(lines.len(), all_cards().len());
        assert!(lines.iter().all(|line| line.starts_with("event=play ")));
    }
}
