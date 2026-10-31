//! The Theosis capability-freshness seam: reconciling the installed Playbook
//! ecosystem tooling against the authoritative Git-backed versions.
//!
//! Football: the equipment manager. The kit is checked before the match —
//! every pad, boot, and glove is compared against the kit list the club
//! actually issued ([`preflight`], §14) — and replacements happen at
//! half-time in a maintenance window, never mid-play. And nobody downgrades
//! the kit: if the club's issued list is older than what the player already
//! wears, the player keeps wearing it ([`classify`] returns `Current`).
//!
//! The constitutional line, enforced by the types: Theosis is not a package
//! manager and not a live tactical component. It classifies and decides; it
//! never mutates the machine on its own authority. [`decide`] turns one
//! classification into a Playbook policy outcome — continue, refuse the
//! affected capability, or schedule the update inside an explicit
//! maintenance window — and [`update_permitted`] is the whole mid-play rule:
//! no maintenance window, no update. A failed build leaves the previous
//! version in place (`UpdateFailed` is set by the build step, never by the
//! classifier), and a post-install version is verified independently before
//! the drive trusts the capability again.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One tool's installed-versus-authoritative version record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolVersion {
    /// Tool name, e.g. `mesut`, `theosis`, `baby`.
    pub tool: String,
    /// Installed version string, as the drive observed it.
    pub installed: String,
    /// Authoritative remote version string, as Git reports it.
    pub remote: String,
    /// Pinned remote commit when known.
    pub commit: Option<String>,
}

/// Compare two dotted version strings numerically component-by-component.
/// Each string is split on `.` and every component is parsed as `u64`;
/// missing components count as `0`. A component that does not parse as
/// `u64` (e.g. a pre-release tag such as `0-rc1`) contributes `0` and is
/// otherwise skipped for ordering purposes: pre-release tags do not affect
/// the order. Comparison is numeric, not lexicographic, so `1.2.10` sorts
/// after `1.2.9`.
pub fn compare_versions(left: &str, right: &str) -> Ordering {
    let mut left_parts = left.split('.');
    let mut right_parts = right.split('.');
    loop {
        let left_component = left_parts.next();
        let right_component = right_parts.next();
        if left_component.is_none() && right_component.is_none() {
            return Ordering::Equal;
        }
        let left_value = left_component
            .and_then(|part| part.parse::<u64>().ok())
            .unwrap_or(0);
        let right_value = right_component
            .and_then(|part| part.parse::<u64>().ok())
            .unwrap_or(0);
        if left_value != right_value {
            return left_value.cmp(&right_value);
        }
    }
}

/// How one tool's freshness was judged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionClassification {
    /// Installed satisfies the authoritative version and every declared
    /// compatibility minimum.
    Current,
    /// The remote is newer, but the installed version still satisfies the
    /// declared compatibility; the update waits for a maintenance window.
    UpdateAvailable,
    /// The installed version is below a required minimum: the capability
    /// must be refreshed before the drive trusts it.
    UpdateRequired,
    /// The installed or remote version is unknown (empty), so no trust
    /// decision can be made.
    Unavailable,
    /// A maintenance-window build failed; the old version is preserved.
    /// Set by the build step, never by [`classify`].
    UpdateFailed,
}

/// Classify one tool against the authoritative remote.
///
/// `Unavailable` when the installed or remote string is empty. Otherwise:
/// remote < installed → `Current` — automatic downgrade is forbidden, an
/// older remote NEVER triggers action; remote == installed → `Current`;
/// remote > installed → `UpdateAvailable`. The classifier never returns
/// `UpdateFailed` or `UpdateRequired`: `UpdateFailed` is set by the build
/// step after a failed maintenance-window build, and `UpdateRequired`
/// needs a declared minimum, which lives in [`CompatibilityPolicy`] —
/// see [`classify_with_policy`].
pub fn classify(tool: &ToolVersion) -> VersionClassification {
    if tool.installed.is_empty() || tool.remote.is_empty() {
        return VersionClassification::Unavailable;
    }
    match compare_versions(&tool.remote, &tool.installed) {
        Ordering::Less | Ordering::Equal => VersionClassification::Current,
        Ordering::Greater => VersionClassification::UpdateAvailable,
    }
}

/// The compatibility policy the drive declares for its evidence adapters:
/// the minimum `major.minor` each tool must have installed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityPolicy {
    /// Minimum `(major, minor)` required per tool name. A tool absent from
    /// the map has no declared minimum.
    pub minimum_required: BTreeMap<String, (u64, u64)>,
}

/// Classify one tool against the remote *and* the drive's compatibility
/// policy. Same semantics as [`classify`], except: a tool whose **installed**
/// version is below its required minimum classifies `UpdateRequired` (the
/// capability must be refreshed before the drive trusts it) even though the
/// update would merely be "available" otherwise.
pub fn classify_with_policy(
    tool: &ToolVersion,
    policy: &CompatibilityPolicy,
) -> VersionClassification {
    let classification = classify(tool);
    if classification != VersionClassification::UpdateAvailable {
        return classification;
    }
    let Some(&(major, minor)) = policy.minimum_required.get(&tool.tool) else {
        return classification;
    };
    let minimum = format!("{major}.{minor}");
    if compare_versions(&tool.installed, &minimum) == Ordering::Less {
        VersionClassification::UpdateRequired
    } else {
        classification
    }
}

/// The §14 bounded preflight for a Franchise campaign: every dependency,
/// classified, in a deterministic tool-name-ascending order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightReport {
    /// One entry per input tool, each with its classification. Sorted
    /// ascending by tool name; every tool that went in is classified.
    pub dependencies: Vec<(ToolVersion, VersionClassification)>,
}

/// Run the preflight: classify every tool under `policy` and sort the
/// report ascending by tool name. Deterministic — the same tool set always
/// produces the same report.
pub fn preflight(tools: &[ToolVersion], policy: &CompatibilityPolicy) -> PreflightReport {
    let mut dependencies: Vec<(ToolVersion, VersionClassification)> = tools
        .iter()
        .map(|tool| {
            let classification = classify_with_policy(tool, policy);
            (tool.clone(), classification)
        })
        .collect();
    dependencies.sort_by(|left, right| left.0.tool.cmp(&right.0.tool));
    PreflightReport { dependencies }
}

/// Playbook's policy outcome for one dependency before a campaign.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightDecision {
    /// The dependency is trusted; the play proceeds.
    Continue,
    /// The dependency cannot be trusted; the affected capability is refused.
    /// `capability` names the tool the drive depends on.
    RefuseCapability {
        /// The tool whose capability is refused.
        capability: String,
    },
    /// The dependency needs a refresh; the update is scheduled for the
    /// maintenance window, never mid-play.
    UpdateInMaintenanceWindow,
}

/// Decide Playbook policy per dependency (§14 examples):
///
/// - `Current` → `Continue`.
/// - `UpdateAvailable` → `Continue`: the current version satisfies declared
///   compatibility; the update waits for a maintenance window.
/// - `UpdateRequired` with the maintenance window open →
///   `UpdateInMaintenanceWindow`.
/// - `UpdateRequired` without a maintenance window →
///   `RefuseCapability { capability: tool }` — required evidence adapter
///   unavailable, refuse the affected capability.
/// - `UpdateFailed` or `Unavailable` → `RefuseCapability`.
///
/// The decision is per dependency: pass the tool name in.
pub fn decide(
    classification: VersionClassification,
    tool: &str,
    maintenance_window_open: bool,
) -> PreflightDecision {
    match classification {
        VersionClassification::Current | VersionClassification::UpdateAvailable => {
            PreflightDecision::Continue
        }
        VersionClassification::UpdateRequired if maintenance_window_open => {
            PreflightDecision::UpdateInMaintenanceWindow
        }
        VersionClassification::UpdateRequired => PreflightDecision::RefuseCapability {
            capability: tool.to_string(),
        },
        VersionClassification::UpdateFailed | VersionClassification::Unavailable => {
            PreflightDecision::RefuseCapability {
                capability: tool.to_string(),
            }
        }
    }
}

/// The maintenance-window rule (§14): Theosis never updates tools mid-play.
/// Updates happen only while the maintenance window is open.
pub const fn update_permitted(maintenance_window_open: bool) -> bool {
    maintenance_window_open
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, installed: &str, remote: &str) -> ToolVersion {
        ToolVersion {
            tool: name.to_string(),
            installed: installed.to_string(),
            remote: remote.to_string(),
            commit: None,
        }
    }

    fn policy(entries: &[(&str, (u64, u64))]) -> CompatibilityPolicy {
        CompatibilityPolicy {
            minimum_required: entries
                .iter()
                .map(|(name, minimum)| ((*name).to_string(), *minimum))
                .collect(),
        }
    }

    #[test]
    fn compare_versions_orders_dotted_components_numerically() {
        assert_eq!(compare_versions("2.1.0", "2.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("2.1.5", "2.1.4"), Ordering::Greater);
        assert_eq!(compare_versions("2.2.0", "2.1.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.2.10", "1.2.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.9", "1.1.0"), Ordering::Less);
    }

    #[test]
    fn missing_and_non_numeric_components_count_as_zero() {
        assert_eq!(compare_versions("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.2", "1.2.1"), Ordering::Less);
        assert_eq!(compare_versions("1.2.0.0", "1.2"), Ordering::Equal);
        // Pre-release tags do not affect the order.
        assert_eq!(compare_versions("3.0.0-rc1", "3.0.0"), Ordering::Equal);
    }

    #[test]
    fn classify_reaches_its_three_documented_outcomes() {
        // Empty installed or remote → Unavailable.
        assert_eq!(
            classify(&tool("mesut", "", "1.4.0")),
            VersionClassification::Unavailable
        );
        assert_eq!(
            classify(&tool("mesut", "1.3.0", "")),
            VersionClassification::Unavailable
        );
        // Remote equal or older → Current: an older remote NEVER triggers
        // action, because automatic downgrade is forbidden.
        assert_eq!(
            classify(&tool("mesut", "1.4.0", "1.4.0")),
            VersionClassification::Current
        );
        assert_eq!(
            classify(&tool("mesut", "1.5.1", "1.4.9")),
            VersionClassification::Current
        );
        // Remote newer → UpdateAvailable.
        assert_eq!(
            classify(&tool("mesut", "1.3.0", "1.4.0")),
            VersionClassification::UpdateAvailable
        );
        // `UpdateFailed` is set by the BUILD step, not by the classifier,
        // so it is deliberately not exercised here.
    }

    #[test]
    fn classify_with_policy_escalates_below_minimum_to_update_required() {
        let policy = policy(&[("mesut", (1, 4))]);
        // Installed 1.3.2 is below the required 1.4: escalation.
        assert_eq!(
            classify_with_policy(&tool("mesut", "1.3.2", "1.6.0"), &policy),
            VersionClassification::UpdateRequired
        );
        // Installed at the minimum stays a plain available update.
        assert_eq!(
            classify_with_policy(&tool("mesut", "1.4.0", "1.6.0"), &policy),
            VersionClassification::UpdateAvailable
        );
        // A tool with no declared minimum is never escalated.
        assert_eq!(
            classify_with_policy(&tool("baby", "0.1.0", "9.0.0"), &policy),
            VersionClassification::UpdateAvailable
        );
    }

    #[test]
    fn preflight_sorts_dependencies_by_tool_name_and_classifies_each() {
        let tools = vec![
            tool("theosis", "0.9.0", "0.9.0"),
            tool("baby", "", "1.0.0"),
            tool("mesut", "1.3.0", "1.5.0"),
        ];
        let empty_policy = CompatibilityPolicy::default();
        let report = preflight(&tools, &empty_policy);
        let names: Vec<&str> = report
            .dependencies
            .iter()
            .map(|(tool, _)| tool.tool.as_str())
            .collect();
        assert_eq!(names, vec!["baby", "mesut", "theosis"]);
        assert_eq!(report.dependencies.len(), tools.len());
        assert!(report
            .dependencies
            .iter()
            .all(|(_, classification)| *classification != VersionClassification::UpdateFailed));
        assert_eq!(report.dependencies[0].1, VersionClassification::Unavailable);
        assert_eq!(
            report.dependencies[1].1,
            VersionClassification::UpdateAvailable
        );
        assert_eq!(report.dependencies[2].1, VersionClassification::Current);
    }

    #[test]
    fn preflight_escalates_below_minimum_versions_in_report_order() {
        let tools = vec![tool("mesut", "1.2.0", "1.6.0")];
        let report = preflight(&tools, &policy(&[("mesut", (1, 4))]));
        assert_eq!(
            report.dependencies[0].1,
            VersionClassification::UpdateRequired
        );
    }

    #[test]
    fn decide_continues_on_current_and_merely_available_versions() {
        assert_eq!(
            decide(VersionClassification::Current, "mesut", false),
            PreflightDecision::Continue
        );
        // An available update never blocks the play and never happens
        // outside a maintenance window; it just waits for half-time.
        assert_eq!(
            decide(VersionClassification::UpdateAvailable, "mesut", false),
            PreflightDecision::Continue
        );
        assert_eq!(
            decide(VersionClassification::UpdateAvailable, "mesut", true),
            PreflightDecision::Continue
        );
    }

    #[test]
    fn decide_routes_update_required_through_the_maintenance_window() {
        assert_eq!(
            decide(VersionClassification::UpdateRequired, "mesut", true),
            PreflightDecision::UpdateInMaintenanceWindow
        );
        assert_eq!(
            decide(VersionClassification::UpdateRequired, "mesut", false),
            PreflightDecision::RefuseCapability {
                capability: "mesut".to_string()
            }
        );
    }

    #[test]
    fn decide_refuses_unavailable_and_failed_capabilities() {
        assert_eq!(
            decide(VersionClassification::Unavailable, "baby", true),
            PreflightDecision::RefuseCapability {
                capability: "baby".to_string()
            }
        );
        assert_eq!(
            decide(VersionClassification::UpdateFailed, "baby", false),
            PreflightDecision::RefuseCapability {
                capability: "baby".to_string()
            }
        );
    }

    #[test]
    fn updates_only_ever_happen_inside_a_maintenance_window() {
        assert!(!update_permitted(false));
        assert!(update_permitted(true));
    }
}
