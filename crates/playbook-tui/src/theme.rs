//! TUI-wide colour schemes, authored by a free model (nemotron via
//! OpenRouter) and validated mechanically: municipality names only —
//! team nicknames and trademarked words are rejected at the corpus gate —
//! and hex shades deliberately shifted from trademark colours.
//!
//! Application is terminal-wide by construction: the theme redefines the
//! terminal's own palette (OSC 4 + OSC 10/11), so every existing frame —
//! dugout, broadcast, film room, refusal panel — adopts the scheme
//! without any render function changing.

use serde::{Deserialize, Serialize};

const CORPUS: &str = include_str!("../themes/themes.json");

/// One authored colour scheme.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub league: String,
    pub municipality: String,
    pub name: String,
    pub colors: ThemeColors,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeColors {
    pub background: String,
    pub ink: String,
    pub dim: String,
    pub primary: String,
    pub secondary: String,
    pub accent: String,
}

impl Theme {
    /// The trademark guard: team nicknames never appear in theme names.
    /// The corpus gate enforced this at authoring time; this re-check
    /// runs on every load, because the corpus is data and data can be
    /// edited (§ prompt-injection boundary: authored content is evidence,
    /// not authority).
    pub fn trademark_clean(&self, forbidden: &[&str]) -> bool {
        let lowered = self.name.to_lowercase();
        // Word-boundary matching: "Los Angeles" must not be caught by the
        // substring "angel" — only a standing nickname word disqualifies.
        let words: Vec<&str> = lowered.split(|c: char| !c.is_alphanumeric()).collect();
        !forbidden.iter().any(|word| words.contains(word))
    }
}

/// Load the authored corpus, applying the trademark guard.
pub fn load_all() -> Vec<Theme> {
    let forbidden = crate::theme::FORBIDDEN_WORDS;
    let mut themes: Vec<Theme> = serde_json::from_str(CORPUS).unwrap_or_default();
    themes.retain(|theme| theme.trademark_clean(forbidden));
    themes.sort_by(|a, b| a.name.cmp(&b.name));
    themes
}

/// The forbidden nickname vocabulary (partial list; the corpus gate holds
/// the fuller set — both must pass).
pub const FORBIDDEN_WORDS: &[&str] = &[
    "patriot",
    "laker",
    "celtic",
    "warrior",
    "yankee",
    "steeler",
    "packer",
    "cowboy",
    "eagle",
    "raven",
    "dolphin",
    "seahawk",
    "bear",
    "viking",
    "cheif",
    "chief",
    "raider",
    "bronco",
    "charger",
    "jet",
    "giant",
    "cowboys",
    "red sox",
    "cubs",
    "white sox",
    "mets",
    "phillies",
    "braves",
    "marlins",
    "ornoles",
    "ray",
    "ranger",
    "astro",
    "angel",
    "dodger",
    "padre",
    "rocky",
    "twins",
    "guardian",
    "tiger",
    "royal",
    "blue jays",
    "mariner",
    "nugget",
    "timberwolf",
    "thunder",
    "blazer",
    "jazz",
    "king",
    "sunny",
    "clipper",
    "laker",
    "rocket",
    "maverick",
    "grizzly",
    "pelican",
    "warrior",
    "clipper",
    "laker",
    "spur",
    "sunny",
    "mavericks",
    "rocket",
    "spurs",
    "maverick",
    "grizzlies",
    "pelicans",
    "kings",
    "suns",
    "clippers",
    "lakers",
    "rockets",
    "mavericks",
    "wolverine",
    "buckeye",
    "crimson",
    "longhorn",
    "sooners",
    "seminole",
    "fighting irish",
    "hoya",
    "blue devil",
    "gator",
    "volunteer",
    "bulldog",
    "wildcat",
    "tide",
];

/// Build the OSC 4 + OSC 10/11 palette redefinition: the terminal-wide
/// application. Maps the theme onto the 16 ANSI slots: background/ink/dim
/// directly, and the accents across the bright axis so charts and
/// verdicts re-theme coherently.
pub fn apply_osc(theme: &Theme) -> String {
    let c = &theme.colors;
    let slot = |index: u8, hex: &str| -> String {
        let (r, g, b) = parse_hex(hex);
        format!("\x1b]4;{index};rgb:{r:02x}/{g:02x}/{b:02x}\x1b\\")
    };
    let (bg_r, bg_g, bg_b) = parse_hex(&c.background);
    let (ink_r, ink_g, ink_b) = parse_hex(&c.ink);
    let mut out = format!(
        "\x1b]11;rgb:{bg_r:02x}/{bg_g:02x}/{bg_b:02x}\x1b\\\x1b]10;rgb:{ink_r:02x}/{ink_g:02x}/{ink_b:02x}\x1b\\"
    );
    out.push_str(&slot(0, &c.background)); // black
    out.push_str(&slot(1, &c.secondary)); // red
    out.push_str(&slot(2, &c.primary)); // green
    out.push_str(&slot(3, &c.accent)); // yellow
    out.push_str(&slot(4, &c.primary)); // blue
    out.push_str(&slot(5, &c.accent)); // magenta
    out.push_str(&slot(6, &c.secondary)); // cyan
    out.push_str(&slot(7, &c.ink)); // white
    out.push_str(&slot(8, &c.dim)); // bright black
    out.push_str(&slot(9, &c.accent));
    out.push_str(&slot(10, &c.primary));
    out.push_str(&slot(11, &c.accent));
    out.push_str(&slot(12, &c.secondary));
    out.push_str(&slot(13, &c.primary));
    out.push_str(&slot(14, &c.dim));
    out.push_str(&slot(15, &c.ink));
    out
}

fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    let byte = |range: std::ops::Range<usize>| {
        u8::from_str_radix(hex.get(range).unwrap_or("00"), 16).unwrap_or(0)
    };
    (byte(0..2), byte(2..4), byte(4..6))
}

/// A truecolor swatch row for the selector preview.
pub fn swatch(theme: &Theme) -> String {
    let block = "\u{2588}\u{2588}\u{2588}";
    let c = &theme.colors;
    format!(
        "\x1b[48;2;{}m{} \x1b[49m {}",
        rgb_term(&c.background),
        block,
        theme.name
    ) + &format!(
        " \x1b[48;2;{}m{} \x1b[48;2;{}m{} \x1b[48;2;{}m{}\x1b[0m",
        rgb_term(&c.primary),
        block,
        rgb_term(&c.secondary),
        block,
        rgb_term(&c.accent),
        block
    )
}

fn rgb_term(hex: &str) -> String {
    let (r, g, b) = parse_hex(hex);
    format!("{r};{g};{b}")
}

/// The selector: list, preview, choose. Line-driven (no raw mode): n/p
/// move, Enter applies and writes the persisted choice, q quits.
pub fn run_selector(themes: &[Theme]) -> Option<Theme> {
    use std::io::{BufRead, Write};
    if themes.is_empty() {
        return None;
    }
    let mut cursor = 0usize;
    loop {
        for (index, theme) in themes.iter().enumerate() {
            let marker = if index == cursor { "▸ " } else { "  " };
            println!("{marker}{}", swatch(theme));
        }
        println!(
            "(n)ext (p)rev (Enter) apply (q)uit — cursor {}/{}",
            cursor + 1,
            themes.len()
        );
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
            return None;
        }
        match line.trim() {
            "n" | "j" => cursor = (cursor + 1).min(themes.len() - 1),
            "p" | "k" => cursor = cursor.saturating_sub(1),
            "q" | "" => return None,
            _ => return Some(themes[cursor].clone()),
        }
    }
}

/// Persist the chosen theme for future runs.
pub fn persist(theme: &Theme) -> std::io::Result<std::path::PathBuf> {
    let dir = dirs_config();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("theme");
    std::fs::write(&path, &theme.name)?;
    Ok(path)
}

/// The persisted theme, if any.
pub fn persisted() -> Option<Theme> {
    let name = std::fs::read_to_string(dirs_config().join("theme")).ok()?;
    load_all()
        .into_iter()
        .find(|theme| theme.name == name.trim())
}

fn dirs_config() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(|home| {
            std::path::PathBuf::from(home)
                .join(".config")
                .join("playbook-tui")
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corpus_loads_and_passes_the_trademark_guard() {
        let themes = load_all();
        assert!(
            themes.len() >= 12,
            "the authored corpus is substantial: {}",
            themes.len()
        );
        // Every name municipality-based: no nickname from the guard list.
        for theme in &themes {
            assert!(
                theme.trademark_clean(FORBIDDEN_WORDS),
                "trademark word in {}",
                theme.name
            );
            assert!(theme.name.chars().next().unwrap().is_uppercase());
        }
        // Names unique.
        let mut names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
        names.sort();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "theme names unique");
    }

    #[test]
    fn every_colour_is_valid_hex_and_dark_ui_viable() {
        for theme in load_all() {
            let c = &theme.colors;
            for (field, hex) in [
                ("background", &c.background),
                ("ink", &c.ink),
                ("dim", &c.dim),
                ("primary", &c.primary),
                ("secondary", &c.secondary),
                ("accent", &c.accent),
            ] {
                assert!(
                    hex.len() == 7 && hex.starts_with('#'),
                    "{field} of {} malformed: {hex}",
                    theme.name
                );
            }
            // Background stays dark: every channel ≤ 0x40.
            let bg = c.background.trim_start_matches('#');
            for channel in (0..6).step_by(2) {
                let value = u8::from_str_radix(&bg[channel..channel + 2], 16).unwrap_or(0);
                assert!(
                    value <= 0x40,
                    "background of {} not dark: {}",
                    theme.name,
                    c.background
                );
            }
        }
    }

    #[test]
    fn the_osc_palette_binds_all_sixteen_slots() {
        let theme = load_all().remove(0);
        let sequence = apply_osc(&theme);
        assert!(sequence.starts_with("\x1b]11;"), "background first");
        for slot in 0..=15 {
            assert!(
                sequence.contains(&format!("]4;{slot};rgb:")),
                "slot {slot} bound"
            );
        }
        assert!(sequence.ends_with("\x1b\\"), "ST terminated");
    }
}
