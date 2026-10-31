//! The SVG emitter: NFL-playbook aesthetics over the play specs.
//!
//! One card is a chalk-green field with yard lines, hash marks, a marked
//! line of scrimmage, and end zones labelled PLAYBOOK and EVIDENCE. Offense
//! draws O's (the quarterback a circled square), defence draws X's; routes
//! follow the coaching tree — solid animated pass routes with arrowheads,
//! thick runs with a ball marker, short block strikes, dashed pre-snap
//! motion, hatched coverage, and heavy pulsing rush arrows. Every glyph
//! carries its dynamic label, and every card footer is a 3form event line.
//! The emitted markup is deterministic: the same spec always renders the
//! same bytes.

use crate::spec::{Glyph, PlaySpec, Role, RouteKind, Side};

/// Render one play card as a standalone `<svg>` fragment.
pub fn render_card(spec: &PlaySpec, id: &str) -> String {
    let mut svg = String::with_capacity(8_000);
    svg.push_str(&format!(
        "<svg id=\"card-{id}\" viewBox=\"0 0 120 124\" xmlns=\"http://www.w3.org/2000/svg\" \
         role=\"img\" aria-label=\"{} play card\">\n",
        xml_escape(&spec.title)
    ));
    svg.push_str(&format!(
        "<title>{} — {}</title>\n",
        xml_escape(&spec.title),
        xml_escape(&spec.coaching_point)
    ));
    svg.push_str(&field_group());
    svg.push_str(&routes_group(spec, id));
    svg.push_str(&slots_group(spec));
    svg.push_str(&title_group(spec));
    svg.push_str(&footer_group(spec));
    svg.push_str("</svg>\n");
    svg
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The chalk-green field: turf, yard lines every 10 yards, hash marks, the
/// line of scrimmage, and the two labelled end zones.
fn field_group() -> String {
    let mut field = String::new();
    field.push_str("<g class=\"field\">\n");
    field
        .push_str("<rect x=\"4\" y=\"4\" width=\"112\" height=\"116\" rx=\"3\" class=\"turf\"/>\n");
    field.push_str("<rect x=\"4\" y=\"4\" width=\"112\" height=\"14\" class=\"endzone\"/>\n");
    field.push_str("<rect x=\"4\" y=\"106\" width=\"112\" height=\"14\" class=\"endzone\"/>\n");
    field.push_str(
        "<text x=\"60\" y=\"13\" text-anchor=\"middle\" class=\"endzone-text\">PLAYBOOK</text>\n",
    );
    field.push_str(
        "<text x=\"60\" y=\"116\" text-anchor=\"middle\" class=\"endzone-text\">EVIDENCE</text>\n",
    );
    for y in [24, 34, 44, 64, 74, 84, 94] {
        field.push_str(&format!(
            "<line x1=\"6\" y1=\"{y}\" x2=\"114\" y2=\"{y}\" class=\"yard-line\"/>\n"
        ));
    }
    for y in [19, 29, 39, 49, 59, 69, 79, 89, 99] {
        for x in [36, 84] {
            field.push_str(&format!(
                "<line x1=\"{x}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\" class=\"hash\"/>\n",
                x + 4
            ));
        }
    }
    field.push_str("<line x1=\"6\" y1=\"50\" x2=\"114\" y2=\"50\" class=\"los\"/>\n");
    field.push_str("<text x=\"9\" y=\"48\" class=\"los-text\">LOS</text>\n");
    field.push_str("</g>\n");
    field
}

fn slots_group(spec: &PlaySpec) -> String {
    let mut group = String::new();
    group.push_str("<g class=\"personnel\">\n");
    for (index, slot) in spec.slots.iter().enumerate() {
        let (x, y) = (6.0 + f64::from(slot.x), 4.0 + f64::from(slot.y));
        let label_x = x + 4.0;
        let label_y = y - 2.5;
        match slot.glyph {
            Glyph::Offense => {
                group.push_str(&format!(
                    "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"2.6\" class=\"glyph-o animated-player\" \
                     style=\"animation-delay:{}ms\"/>\n",
                    index * 120
                ));
            }
            Glyph::Quarterback => {
                group.push_str(&format!(
                    "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"5\" height=\"5\" rx=\"1.4\" \
                     class=\"glyph-qb animated-player\" transform=\"rotate(45 {x:.1} {y:.1})\"/>\n"
                ));
            }
            Glyph::Defense => {
                let (x1, x2) = (x - 2.4, x + 2.4);
                let (y1, y2) = (y - 2.4, y + 2.4);
                group.push_str(&format!(
                    "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" \
                     class=\"glyph-x animated-player\" style=\"animation-delay:{}ms\"/>\n",
                    index * 120
                ));
                group.push_str(&format!(
                    "<line x1=\"{x2:.1}\" y1=\"{y1:.1}\" x2=\"{x1:.1}\" y2=\"{y2:.1}\" \
                     class=\"glyph-x animated-player\" style=\"animation-delay:{}ms\"/>\n",
                    index * 120
                ));
            }
        }
        group.push_str(&format!(
            "<text x=\"{label_x:.1}\" y=\"{label_y:.1}\" class=\"glyph-label\">{}</text>\n",
            xml_escape(&slot.label)
        ));
        if slot.role == Role::Runner {
            group.push_str(&format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"1.4\" ry=\"0.9\" class=\"ball\"/>\n",
                x + 1.0,
                y + 3.4
            ));
        }
    }
    group.push_str("</g>\n");
    group
}

fn routes_group(spec: &PlaySpec, id: &str) -> String {
    let mut group = String::new();
    group.push_str("<g class=\"routes\">\n");
    for (index, route) in spec.routes.iter().enumerate() {
        let Some(origin) = spec.slots.get(route.from_slot) else {
            continue;
        };
        let mut path = format!(
            "M {} {}",
            6.0 + f64::from(origin.x),
            4.0 + f64::from(origin.y)
        );
        for (x, y) in &route.points {
            path.push_str(&format!(
                " L {} {}",
                6.0 + f64::from(*x),
                4.0 + f64::from(*y)
            ));
        }
        let class = match route.kind {
            RouteKind::Pass => "route-pass",
            RouteKind::Run => "route-run",
            RouteKind::Block => "route-block",
            RouteKind::Motion => "route-motion",
            RouteKind::Coverage => "route-coverage",
            RouteKind::Rush => "route-rush",
        };
        group.push_str(&format!(
            "<path id=\"{id}-r{index}\" d=\"{path}\" class=\"{class} animated-route\" \
             style=\"animation-delay:{}ms\"/>\n",
            index * 200
        ));
        if matches!(route.kind, RouteKind::Pass | RouteKind::Rush) {
            if let Some((x, y)) = route.points.last() {
                let (ax, ay) = (6.0 + f64::from(*x), 4.0 + f64::from(*y));
                group.push_str(&format!(
                    "<circle cx=\"{ax:.1}\" cy=\"{ay:.1}\" r=\"0.9\" class=\"route-arrow\"/>\n"
                ));
            }
        }
    }
    group.push_str("</g>\n");
    group
}

fn title_group(spec: &PlaySpec) -> String {
    let class_chip = match spec.class {
        crate::spec::CardClass::Formation => "FORMATION",
        crate::spec::CardClass::Play => "PLAY",
        crate::spec::CardClass::Adjustment => "ADJUSTMENT",
        crate::spec::CardClass::Doctrine => "DOCTRINE",
        crate::spec::CardClass::Defence => "DEFENCE",
    };
    let side_chip = match spec.side {
        Side::Offense => "OFFENSE",
        Side::Defense => "DEFENSE",
    };
    format!(
        "<g class=\"title-group\">\n\
         <text x=\"8\" y=\"22\" class=\"card-title\">{}</text>\n\
         <text x=\"112\" y=\"22\" text-anchor=\"end\" class=\"class-chip\">{class_chip}</text>\n\
         <text x=\"112\" y=\"27\" text-anchor=\"end\" class=\"side-chip\">{side_chip}</text>\n\
         </g>\n",
        xml_escape(&spec.title)
    )
}

fn footer_group(spec: &PlaySpec) -> String {
    let mut footer = String::new();
    footer.push_str("<g class=\"footer-group\">\n");
    footer.push_str(&format!(
        "<text x=\"8\" y=\"115\" class=\"coaching-point\">{}</text>\n",
        xml_escape(&wrap_text(&spec.coaching_point, 64).0)
    ));
    if let Some(second) = &wrap_text(&spec.coaching_point, 64).1 {
        footer.push_str(&format!(
            "<text x=\"8\" y=\"118.4\" class=\"coaching-point\">{}</text>\n",
            xml_escape(second)
        ));
    }
    footer.push_str(&format!(
        "<text x=\"8\" y=\"111.5\" class=\"threeform-line\">{}</text>\n",
        xml_escape(&spec.threeform_line())
    ));
    footer.push_str("</g>\n");
    footer
}

/// Word-wrap a coaching point at `width` characters into (first, optional
/// second) lines. Deterministic: greedy fill on spaces.
fn wrap_text(text: &str, width: usize) -> (String, Option<String>) {
    if text.chars().count() <= width {
        return (text.to_string(), None);
    }
    let mut first = String::new();
    let mut rest = String::new();
    let mut in_first = true;
    for word in text.split_whitespace() {
        if in_first && first.chars().count() + word.chars().count() + 1 > width {
            in_first = false;
        }
        if in_first {
            if !first.is_empty() {
                first.push(' ');
            }
            first.push_str(word);
        } else {
            if !rest.is_empty() {
                rest.push(' ');
            }
            rest.push_str(word);
        }
    }
    (first, if rest.is_empty() { None } else { Some(rest) })
}
