//! Play-card specifications: the X-and-O alignments behind every card.
//!
//! Each play is a [`PlaySpec`]: a name from the tactical repertoire, the
//! side of the ball, the alignment (every placed glyph with its role and
//! label), and the routes drawn from it. Coordinates are on a 100×100 grid:
//! `x` runs left-to-right across the field, `y` runs bottom-to-top with the
//! line of scrimmage at y = 50, the offense's own end zone below and the
//! defence's end zone above. Everything is data — the SVG emitter and the
//! gallery are generic over these specs, so a new play is a new spec, never
//! new rendering code.
//!
//! Dynamic labelling: alignment labels and the stats footer are filled from
//! the live repertoire data — formation profiles for the formation cards,
//! route geometry for the diagnostic cards, the corroboration policy for
//! Blitz — so a card always describes the tactic as the crate actually
//! defines it.

use serde::{Deserialize, Serialize};

/// Which side of the ball a play card shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Offense,
    Defense,
}

/// The tactical class chip shown on the card face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardClass {
    Formation,
    Play,
    Adjustment,
    Doctrine,
    Defence,
}

/// The glyph drawn for one aligned player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Glyph {
    /// The offense: a circle.
    Offense,
    /// The defence: an X.
    Defense,
    /// The quarterback: a rotated square.
    Quarterback,
}

/// The job of one aligned player, rendered as the glyph's small label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Quarterback,
    Lineman,
    Receiver,
    Blocker,
    Runner,
    Spy,
    Safety,
    Corner,
    Linebacker,
    Observer,
    Validator,
}

impl Role {
    /// The short chip printed beside the glyph.
    pub fn chip(self) -> &'static str {
        match self {
            Self::Quarterback => "QB",
            Self::Lineman => "OL",
            Self::Receiver => "WR",
            Self::Blocker => "BLK",
            Self::Runner => "RB",
            Self::Spy => "SPY",
            Self::Safety => "S",
            Self::Corner => "CB",
            Self::Linebacker => "LB",
            Self::Observer => "OBS",
            Self::Validator => "VAL",
        }
    }
}

/// One placed glyph on the card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub x: i32,
    pub y: i32,
    pub glyph: Glyph,
    pub role: Role,
    /// The dynamic label: normally the role chip, sometimes a named owner
    /// (SPY, OBS) or a domain (the Bear front's marked core).
    pub label: String,
}

/// A route drawn from a slot index: the polyline the run or pass travels,
/// with the line style the coaching tree uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// Index into the spec's slots.
    pub from_slot: usize,
    pub points: Vec<(i32, i32)>,
    pub kind: RouteKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    /// A pass route: solid with an arrowhead, animated marching.
    Pass,
    /// A run: thick solid with a ball marker.
    Run,
    /// A block: a short perpendicular strike.
    Block,
    /// Pre-snap motion: dashed, animated slide.
    Motion,
    /// A coverage assignment: dashed zone hatching.
    Coverage,
    /// An escalation arrow: heavy, animated pulse (the Blitz rush).
    Rush,
}

/// One play card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaySpec {
    /// The machine name — the conflict-matrix key.
    pub name: String,
    /// The card-face title.
    pub title: String,
    pub side: Side,
    pub class: CardClass,
    /// The one-line coaching point.
    pub coaching_point: String,
    pub slots: Vec<Slot>,
    pub routes: Vec<Route>,
    /// The 3form stats footer: `key=value` pairs in the HUD vocabulary.
    pub stats: Vec<(String, String)>,
}

impl PlaySpec {
    /// The 3form event line for this card, in the HUD's key=value grammar:
    /// `event=play name=spread class=formation ... text="coaching point"`.
    pub fn threeform_line(&self) -> String {
        let mut line = format!("event=play name={}", self.name);
        let class = match self.class {
            CardClass::Formation => "formation",
            CardClass::Play => "play",
            CardClass::Adjustment => "adjustment",
            CardClass::Doctrine => "doctrine",
            CardClass::Defence => "defence",
        };
        line.push_str(&format!(" class={class}"));
        line.push_str(&format!(
            " side={}",
            match self.side {
                Side::Offense => "offense",
                Side::Defense => "defence",
            }
        ));
        for (key, value) in &self.stats {
            if key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                line.push_str(&format!(" {key}={value}"));
            }
        }
        line.push_str(&format!(" text=\"{}\"", self.coaching_point));
        line
    }
}

/// Helper constructors so the specs read like a coach's tablet.
pub mod slot {
    use super::{Glyph, Role, Slot};

    pub fn offense(x: i32, y: i32, role: Role) -> Slot {
        Slot {
            x,
            y,
            glyph: Glyph::Offense,
            role,
            label: role.chip().to_string(),
        }
    }

    pub fn quarterback(x: i32, y: i32) -> Slot {
        Slot {
            x,
            y,
            glyph: Glyph::Quarterback,
            role: Role::Quarterback,
            label: Role::Quarterback.chip().to_string(),
        }
    }

    pub fn defense(x: i32, y: i32, role: Role, label: &str) -> Slot {
        Slot {
            x,
            y,
            glyph: Glyph::Defense,
            role,
            label: label.to_string(),
        }
    }

    /// An offensive receiver with a dynamic domain label (what this card's
    /// personnel actually owns).
    pub fn receiver(x: i32, y: i32, label: &str) -> Slot {
        Slot {
            x,
            y,
            glyph: Glyph::Offense,
            role: Role::Receiver,
            label: label.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threeform_lines_follow_the_hud_grammar() {
        let spec = PlaySpec {
            name: "spread".into(),
            title: "SPREAD".into(),
            side: Side::Offense,
            class: CardClass::Formation,
            coaching_point: "force the defence to cover the whole field".into(),
            slots: Vec::new(),
            routes: Vec::new(),
            stats: vec![
                ("max_concurrent".into(), "8".into()),
                ("overlap_ppm".into(), "50".into()),
            ],
        };
        let line = spec.threeform_line();
        assert_eq!(
            line,
            "event=play name=spread class=formation side=offense \
             max_concurrent=8 overlap_ppm=50 \
             text=\"force the defence to cover the whole field\""
        );
        assert!(line.starts_with("event=play "));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn unsafe_stat_keys_are_dropped_from_the_line() {
        let spec = PlaySpec {
            name: "spy".into(),
            title: "SPY".into(),
            side: Side::Defense,
            class: CardClass::Defence,
            coaching_point: "watch the runner".into(),
            slots: Vec::new(),
            routes: Vec::new(),
            stats: vec![("not a key!".into(), "value".into())],
        };
        let line = spec.threeform_line();
        assert!(!line.contains("not a key!"));
        assert!(line.ends_with("text=\"watch the runner\""));
    }

    #[test]
    fn role_chips_are_the_short_labels() {
        assert_eq!(Role::Quarterback.chip(), "QB");
        assert_eq!(Role::Observer.chip(), "OBS");
    }
}
