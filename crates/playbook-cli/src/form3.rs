//! The 3form HUD event grammar for the command line.
//!
//! One event is one line: `event=<kind> key=value ...`, optionally closed by
//! a free-text pair `text="..."`. Keys are ASCII alphanumerics and underscore;
//! values carry no bare spaces — anything with spaces (and every `text` pair,
//! matching the playbook-cards `threeform_line`) is quoted. Lines with these
//! guarantees feed the 3form HUD directly, the same vocabulary the cards
//! gallery already speaks.

#![forbid(unsafe_code)]

use ::form3::compat::Colorize;

/// Compose one 3form event line: `event=<kind> key=value ...`.
///
/// Pairs whose key is not ASCII alphanumeric-plus-underscore are dropped, as
/// in the playbook-cards grammar. Values with interior whitespace — and every
/// `text` pair — are quoted; interior whitespace collapses to single spaces
/// and double quotes become single quotes, so one event is always exactly one
/// parseable line.
pub fn event(kind: &str, pairs: &[(&str, String)]) -> String {
    let mut line = format!("event={kind}");
    for (key, value) in pairs {
        if !key_is_safe(key) {
            continue;
        }
        let free_text = *key == "text" || value.contains(' ') || value.contains('"');
        if free_text {
            let collapsed = value.split_whitespace().collect::<Vec<&str>>().join(" ");
            line.push_str(&format!(" {key}=\"{}\"", collapsed.replace('"', "'")));
        } else {
            line.push_str(&format!(" {key}={value}"));
        }
    }
    line
}

/// Render a boolean as the `true`/`false` token the HUD reads.
pub fn flag(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

/// The 3form painter for the human surface: colour, weight and emphasis
/// from the real `form3` library, resolved against stdout's colour support.
/// Piped output, `NO_COLOR` and `--basic` all strip to plain text — the
/// same facts, minus the paint.
pub struct Painter {
    color: bool,
}

impl Painter {
    /// Detect from stdout: a TTY with colour support paints; a pipe does
    /// not.
    pub fn detect() -> Self {
        Self {
            color: form3::term::TermInfo::detect().supports_color(),
        }
    }

    /// Never paints (tests, forced-plain contexts).
    #[allow(dead_code)]
    pub fn plain() -> Self {
        Self { color: false }
    }

    /// Whether this painter paints.
    pub fn colored(&self) -> bool {
        self.color
    }

    pub fn accent(&self, text: &str) -> String {
        self.paint(text, |t| t.bold())
    }

    fn paint(&self, text: &str, style: fn(&str) -> form3::compat::StyledText) -> String {
        if self.color {
            style(text).to_string()
        } else {
            text.to_string()
        }
    }

    /// The command headline: `▌ PLAYBOOK — <text>`.
    pub fn headline(&self, text: &str) -> String {
        let banner = if self.color {
            "▌ PLAYBOOK".bright_cyan().bold().to_string()
        } else {
            "▌ PLAYBOOK".to_string()
        };
        format!("{banner} — {}", self.accent(text))
    }

    /// A bold accent row label.
    pub fn accent_row(&self, text: &str) -> String {
        self.accent(text)
    }

    /// A passing verdict: green check.
    pub fn pass(&self, text: &str) -> String {
        self.paint(&format!("✓ {text}"), |t| t.bright_green())
    }

    /// A failing verdict: red cross.
    pub fn fail(&self, text: &str) -> String {
        self.paint(&format!("✗ {text}"), |t| t.bright_red())
    }

    /// A flagged-for-attention value: bright yellow.
    pub fn flag_value(&self, text: &str) -> String {
        self.paint(text, |t| t.bright_yellow())
    }

    /// A dimmed key label.
    pub fn key(&self, text: &str) -> String {
        self.paint(text, |t| t.dimmed())
    }

    /// One `key value` reading line with the key dimmed.
    pub fn kv(&self, key: &str, value: &str) -> String {
        format!("{} {}", self.key(&format!("{key}:")), value)
    }

    /// A boolean reading as a coloured verdict token.
    pub fn boolean(&self, ok: bool, label: &str) -> String {
        if ok {
            self.pass(label)
        } else {
            self.fail(label)
        }
    }
}

/// The stage bar: one row of progress cells, one per completed stage, in
/// the broadcast progress grammar (`█` filled, `░` remaining).
pub fn stage_bar(completed: usize, total: usize) -> String {
    if total == 0 {
        return String::new();
    }
    let width = total.clamp(10, 60);
    let filled = (completed.min(total) * width) / total;
    format!(
        "{}{}  {}/{}",
        "█".repeat(filled),
        "░".repeat(width - filled),
        completed,
        total
    )
}

/// True when the key fits the grammar: non-empty ASCII alphanumerics and
/// underscore.
fn key_is_safe(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_open_with_the_kind_and_carry_pairs() {
        let line = event(
            "mvp_complete",
            &[
                ("replay_match", flag(true).to_string()),
                ("disposition", "punt".to_string()),
            ],
        );
        assert_eq!(
            line,
            "event=mvp_complete replay_match=true disposition=punt"
        );
    }

    #[test]
    fn unsafe_keys_are_dropped_from_the_line() {
        let unsafe_key = "not a key!";
        let line = event("probe", &[(unsafe_key, "v".to_string())]);
        assert_eq!(line, "event=probe");
    }

    #[test]
    fn free_text_is_quoted_and_quote_free() {
        let line = event(
            "audit_reason",
            &[("text", "budget says \"punt\" now".to_string())],
        );
        assert_eq!(line, "event=audit_reason text=\"budget says 'punt' now\"");
    }

    #[test]
    fn spaced_values_are_quoted_and_whitespace_collapses() {
        let line = event(
            "tactical_status",
            &[("play", "integration  validation".to_string())],
        );
        assert_eq!(
            line,
            "event=tactical_status play=\"integration validation\""
        );
    }

    #[test]
    fn flags_render_as_tokens() {
        assert_eq!(flag(true), "true");
        assert_eq!(flag(false), "false");
    }

    #[test]
    fn the_plain_painter_emits_no_escape_codes() {
        let painter = Painter::plain();
        assert!(!painter.headline("ACCEPTANCE DRIVE").contains('\x1b'));
        assert!(!painter.pass("replay clean").contains('\x1b'));
        assert!(!painter.fail("replay diverged").contains('\x1b'));
        assert!(!painter.kv("disposition", "punt").contains('\x1b'));
        assert_eq!(painter.kv("disposition", "punt"), "disposition: punt");
        assert_eq!(painter.boolean(true, "replay clean"), "✓ replay clean");
        assert_eq!(
            painter.boolean(false, "replay diverged"),
            "✗ replay diverged"
        );
    }

    #[test]
    fn the_stage_bar_counts_completed_stages() {
        let bar = stage_bar(7, 10);
        assert!(bar.ends_with("7/10"));
        assert!(bar.contains('█'));
        assert!(bar.contains('░'));
        assert_eq!(stage_bar(0, 0), "");
    }
}
