//! The 3form HUD event grammar for the command line.
//!
//! One event is one line: `event=<kind> key=value ...`, optionally closed by
//! a free-text pair `text="..."`. Keys are ASCII alphanumerics and underscore;
//! values carry no bare spaces — anything with spaces (and every `text` pair,
//! matching the playbook-cards `threeform_line`) is quoted. Lines with these
//! guarantees feed the 3form HUD directly, the same vocabulary the cards
//! gallery already speaks.

#![forbid(unsafe_code)]

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
}
