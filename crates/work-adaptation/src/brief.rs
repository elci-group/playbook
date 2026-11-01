//! Brief normalisation (§9.1): the brief owns the wrapper mismatch when
//! the model's answer was substantively right but the contract rejected
//! it on presentation.
//!
//! INVARIANT (§9.1): the normaliser ADDS clarity — it appends explicit,
//! machine-checkable output rules derived from validator failures. It
//! NEVER removes, rewrites, weakens or drops any requirement of the
//! original brief. The original text is always carried through verbatim;
//! the repaired brief is a strict superset of the original's demands, so
//! the validator is never bypassed and standards are never lowered.

/// Normalise an *observed* answer before semantic comparison: strip
/// markdown fences, strip surrounding quotes, trim whitespace.
pub fn normalise_observed_answer(answer: &str) -> String {
    let mut s = answer.trim();
    // Strip markdown fences (```...``` with an optional language tag).
    if let Some(rest) = s.strip_prefix("```") {
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric());
        let rest = rest.strip_prefix('\n').unwrap_or(rest);
        if let Some(body) = rest.strip_suffix("```") {
            s = body;
        } else {
            s = rest;
        }
    }
    let s = s.trim();
    // Strip one balanced pair of surrounding quotes.
    let s = s
        .strip_prefix('"')
        .and_then(|body| body.strip_suffix('"'))
        .filter(|body| !body.is_empty())
        .or_else(|| {
            s.strip_prefix('\'')
                .and_then(|body| body.strip_suffix('\''))
                .filter(|body| !body.is_empty())
        })
        .unwrap_or(s);
    s.trim().to_string()
}

/// Whether a semantic requirement is met by a (already normalised)
/// answer: case-insensitive substring containment.
pub fn semantic_requirement_met(requirement: &str, normalised: &str) -> bool {
    let req = requirement.trim();
    if req.is_empty() {
        return true;
    }
    normalised.to_lowercase().contains(&req.to_lowercase())
}

/// Produce a REPAIRED brief: the original verbatim, plus explicit
/// machine-checkable output rules derived from the validator failures.
///
/// Invariant: never removes or weakens a requirement (§9.1). See the
/// module documentation.
pub fn normalise_brief(original: &str, validator_failures: &[String]) -> String {
    if validator_failures.is_empty() {
        return original.trim().to_string();
    }
    let mut out = String::with_capacity(original.len() + 128);
    out.push_str(original.trim_end());
    out.push_str("\n\nOutput contract repair (requirements unchanged, presentation made machine-checkable):\n");
    for failure in validator_failures {
        let failure = failure.trim();
        if failure.is_empty() {
            continue;
        }
        if let Some(missing) = failure
            .strip_prefix("missing required content:")
            .map(str::trim)
        {
            out.push_str(&format!(
                "- Your output MUST contain the exact string: {missing}\n"
            ));
        } else {
            out.push_str(&format!(
                "- Your output MUST satisfy this validator exactly: {failure}\n"
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markdown_fences() {
        assert_eq!(normalise_observed_answer("```\nhello\n```"), "hello");
        assert_eq!(
            normalise_observed_answer("```rust\nfn x() {}\n```"),
            "fn x() {}"
        );
        assert_eq!(normalise_observed_answer("```\nhello"), "hello");
    }

    #[test]
    fn strips_surrounding_quotes_and_trims() {
        assert_eq!(normalise_observed_answer("  \"answer\"  "), "answer");
        assert_eq!(normalise_observed_answer("'answer'"), "answer");
        assert_eq!(normalise_observed_answer("plain"), "plain");
    }

    #[test]
    fn quote_strip_is_pairwise() {
        // A lone leading quote is not a pair — keep it.
        assert_eq!(normalise_observed_answer("\"answer"), "\"answer");
    }

    #[test]
    fn semantic_requirement_case_insensitive_contains() {
        assert!(semantic_requirement_met(
            "Total: 42",
            "the total: 42 is final"
        ));
        assert!(!semantic_requirement_met("Total: 42", "total was 41"));
        assert!(semantic_requirement_met("", "anything"));
    }

    #[test]
    fn repaired_brief_preserves_original_verbatim() {
        let original = "List the three rivers.\nDo not use markdown fences.";
        let repaired = normalise_brief(original, &["missing required content: Nile".into()]);
        assert!(repaired.starts_with(original));
    }

    #[test]
    fn repaired_brief_adds_rules_for_each_failure() {
        let repaired = normalise_brief(
            "Do the thing.",
            &[
                "missing required content: TOTAL_LINE".into(),
                "output exceeded 100 lines".into(),
            ],
        );
        assert!(repaired.contains("Your output MUST contain the exact string: TOTAL_LINE"));
        assert!(repaired.contains(
            "Your output MUST satisfy this validator exactly: output exceeded 100 lines"
        ));
    }

    #[test]
    fn no_failure_leaves_brief_trimmed_only() {
        assert_eq!(normalise_brief("  be good  ", &[]), "be good");
    }

    #[test]
    fn requirements_preserved_no_requirement_dropped() {
        let original = "R1: alpha\nR2: beta\nR3: gamma";
        let repaired = normalise_brief(original, &["missing required content: beta".into()]);
        for req in ["R1: alpha", "R2: beta", "R3: gamma"] {
            assert!(repaired.contains(req), "dropped: {req}");
        }
    }
}
