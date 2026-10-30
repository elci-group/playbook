//! Manmark: problem-domain ownership (FC-5).
//!
//! Every persistent problem domain has exactly one defender. A mark records
//! who owns the domain, how bad it is, which deterministic inspection
//! trigger watches it, and whether remediation is instinctive (the host
//! runs a fixed procedure; no inference is consumed) or strategic (a named
//! personnel plays the coverage).
//!
//! Enforcement is instinctive-first: an instinctive mark that breaches is
//! handled immediately by the deterministic procedure and never escalates.
//! A strategic mark breaching at High or Critical severity proposes a
//! [`ManmarkRequest`] through the blitz escalation seam — a proposal only;
//! the registry never authorises expenditure. The host flips a breached
//! mark into [`MarkState::Escalating`] once policy authorises the blitz.
//!
//! Strategy graduates into instinct: after enough consecutive successful
//! strategic remediations, [`ManmarkRegistry::graduate`] demotes the mark's
//! remediation to an extracted deterministic procedure.
//!
//! Inspection is deterministic. The observed summary of a piece of evidence
//! is the canonical JSON string of its sealed body (`serde_json::to_string`
//! of the `serde_json::Value` — compact, key-sorted, byte-stable for a
//! given workspace build). The mark trips exactly when the evidence class
//! equals the trigger's class and the summary contains the trigger's
//! subject fragment (an empty fragment matches every body of the right
//! class). Inspection timestamps are monotonic: a later observation never
//! rewinds `last_inspected_ms`.

use serde::{Deserialize, Serialize};

use playbook_blitz::escalation::ManmarkRequest;
use playbook_blitz::Severity;
use playbook_core::{EventId, Evidence, EvidenceClass, EvidenceId, PersonnelId, PlaybookError};

/// The life cycle of one owned domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkState {
    /// The defender holds coverage.
    Contained,
    /// The trigger tripped; the defender is beaten.
    Exposed,
    /// A breach was escalated and a blitz owns recovery.
    Escalating,
}

/// How the domain is defended. Instinctive consumes no inference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Remediation {
    /// The host runs this deterministic procedure on a breach.
    Instinctive { procedure: String },
    /// This named personnel plays the coverage.
    Strategic { owner: PersonnelId },
}

/// A deterministic inspection trigger: evidence of this class whose summary
/// contains the subject fragment trips the mark.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionTrigger {
    pub evidence_class: EvidenceClass,
    pub subject_contains: String,
}

/// The result of one inspection against one mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkOutcome {
    /// The trigger did not trip.
    Untriggered,
    /// The trigger tripped; the mark is exposed (or stays exposed).
    Breached,
    /// A clean inspection recontained an exposed mark.
    Recontained,
}

/// One owned problem domain: who defends it, how bad it is, what the
/// deterministic inspection trigger is, and which remediation class it uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mark {
    pub domain: String,
    pub owner: PersonnelId,
    pub severity: Severity,
    pub state: MarkState,
    pub remediation: Remediation,
    pub trigger: InspectionTrigger,
    /// Evidence sealed against this mark; breaches append their evidence.
    pub evidence_refs: Vec<EvidenceId>,
    /// Monotonic last-inspection timestamp in milliseconds.
    pub last_inspected_ms: u64,
    /// Consecutive successful remediations since the last breach or
    /// recontainment. A breach resets them.
    pub consecutive_successes: u32,
}

/// The problem-domain ownership runtime. The registry keeps its marks
/// sorted by domain so persistence and inspection are order-independent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManmarkRegistry {
    marks: Vec<Mark>,
}

fn invalid(message: &str) -> PlaybookError {
    PlaybookError::Evidence(message.into())
}

impl ManmarkRegistry {
    /// An empty registry.
    pub fn empty() -> Self {
        Self { marks: Vec::new() }
    }

    /// Claim ownership of a domain. Refuses a duplicate domain and keeps
    /// the registry sorted domain-ascending.
    pub fn assign(&mut self, mark: Mark) -> Result<(), PlaybookError> {
        if mark.domain.trim().is_empty() {
            return Err(invalid("manmark domain requires a subject"));
        }
        match self
            .marks
            .binary_search_by(|existing| existing.domain.as_str().cmp(mark.domain.as_str()))
        {
            Ok(_) => Err(invalid("domain is already marked")),
            Err(at) => {
                self.marks.insert(at, mark);
                Ok(())
            }
        }
    }

    /// Every owned mark, domain-ascending.
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// The mark owning one domain, or none.
    pub fn mark(&self, domain: &str) -> Option<&Mark> {
        self.marks.iter().find(|mark| mark.domain == domain)
    }

    fn mark_mut(&mut self, domain: &str) -> Result<&mut Mark, PlaybookError> {
        self.marks
            .iter_mut()
            .find(|mark| mark.domain == domain)
            .ok_or_else(|| invalid("unknown manmark domain"))
    }

    /// Inspect one observed piece of evidence against the mark's trigger.
    /// The deterministic summary of the evidence is the canonical JSON
    /// string of its sealed body.
    ///
    /// A tripping evidence seals its id into the mark, resets the success
    /// streak, and moves a `Contained` mark to `Exposed` (an `Exposed` or
    /// `Escalating` mark stays put and is reported breached again). A clean
    /// inspection of an `Exposed` or `Escalating` mark recontains it and
    /// starts a fresh success streak at zero.
    pub fn inspect(
        &mut self,
        domain: &str,
        evidence: &Evidence,
    ) -> Result<MarkOutcome, PlaybookError> {
        let mark = self.mark_mut(domain)?;
        let summary = serde_json::to_string(&evidence.body).map_err(PlaybookError::json)?;
        mark.last_inspected_ms = mark.last_inspected_ms.max(evidence.observed_at_ms);
        let tripped = evidence.class == mark.trigger.evidence_class
            && summary.contains(mark.trigger.subject_contains.as_str());
        if !tripped {
            return Ok(
                if matches!(mark.state, MarkState::Exposed | MarkState::Escalating) {
                    mark.state = MarkState::Contained;
                    mark.consecutive_successes = 0;
                    MarkOutcome::Recontained
                } else {
                    MarkOutcome::Untriggered
                },
            );
        }
        if !mark.evidence_refs.contains(&evidence.id) {
            mark.evidence_refs.push(evidence.id.clone());
        }
        mark.consecutive_successes = 0;
        if mark.state == MarkState::Contained {
            mark.state = MarkState::Exposed;
        }
        Ok(MarkOutcome::Breached)
    }

    /// Instinctive-first enforcement. An `Instinctive` mark never escalates:
    /// the host runs its deterministic procedure and no inference is
    /// consumed (`Ok(None)`). A `Strategic` mark escalates only at High or
    /// Critical severity, proposing a blitz request through the seam;
    /// anything lower stays in-drive (`Ok(None)`). An unknown domain is an
    /// error, as is a domain that cannot name an originating event.
    pub fn escalation_request(
        &self,
        domain: &str,
    ) -> Result<Option<ManmarkRequest>, PlaybookError> {
        let mark = self
            .mark(domain)
            .ok_or_else(|| invalid("unknown manmark domain"))?;
        let Remediation::Strategic { .. } = &mark.remediation else {
            return Ok(None);
        };
        if !matches!(mark.severity, Severity::High | Severity::Critical) {
            return Ok(None);
        }
        Ok(Some(ManmarkRequest {
            threat: domain.to_string(),
            severity: mark.severity,
            evidence_refs: mark.evidence_refs.clone(),
            originating_event: EventId::new(format!("mark-{domain}"))?,
        }))
    }

    /// The strategic→instinctive graduation path. After `threshold`
    /// consecutive successful strategic remediations, the mark proposes an
    /// instinctive procedure extracted from its domain
    /// (`"graduated: {domain}"`), demotes its remediation, and restarts its
    /// success streak at zero. Returns `Some(procedure)` on graduation,
    /// `None` while the streak sits below the threshold or the mark already
    /// plays instinctively.
    pub fn graduate(
        &mut self,
        domain: &str,
        threshold: u32,
    ) -> Result<Option<String>, PlaybookError> {
        let mark = self.mark_mut(domain)?;
        if mark.consecutive_successes < threshold {
            return Ok(None);
        }
        let Remediation::Strategic { .. } = &mark.remediation else {
            return Ok(None);
        };
        let procedure = format!("graduated: {domain}");
        mark.remediation = Remediation::Instinctive {
            procedure: procedure.clone(),
        };
        mark.consecutive_successes = 0;
        Ok(Some(procedure))
    }

    /// Record a successful remediation, extending the consecutive success
    /// streak that graduation counts.
    pub fn record_success(&mut self, domain: &str) -> Result<(), PlaybookError> {
        let mark = self.mark_mut(domain)?;
        mark.consecutive_successes = mark.consecutive_successes.saturating_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::EvidenceClass;
    use serde_json::json;

    fn mark(domain: &str, owner: &str, remediation: Remediation) -> Mark {
        Mark {
            domain: domain.into(),
            owner: PersonnelId::new(owner).unwrap(),
            severity: Severity::Medium,
            state: MarkState::Contained,
            remediation,
            trigger: InspectionTrigger {
                evidence_class: EvidenceClass::TestReport,
                subject_contains: "dependency drift".into(),
            },
            evidence_refs: Vec::new(),
            last_inspected_ms: 0,
            consecutive_successes: 0,
        }
    }

    fn strategic(domain: &str, owner: &str, severity: Severity) -> Mark {
        Mark {
            severity,
            ..mark(
                domain,
                owner,
                Remediation::Strategic {
                    owner: PersonnelId::new(owner).unwrap(),
                },
            )
        }
    }

    fn evidence(class: EvidenceClass, subject: &str, at_ms: u64) -> Evidence {
        Evidence::seal(
            class,
            json!({ "subject": subject, "unit": "lockfile" }),
            at_ms,
        )
        .unwrap()
    }

    #[test]
    fn assign_keeps_domain_order_and_refuses_duplicates() {
        let mut registry = ManmarkRegistry::empty();
        registry
            .assign(mark(
                "flaky-suite",
                "tool",
                Remediation::Instinctive {
                    procedure: "rerun".into(),
                },
            ))
            .unwrap();
        registry
            .assign(mark(
                "supply-chain",
                "sentinel",
                Remediation::Instinctive {
                    procedure: "pin".into(),
                },
            ))
            .unwrap();
        registry
            .assign(mark(
                "dep-drift",
                "tool",
                Remediation::Instinctive {
                    procedure: "pin".into(),
                },
            ))
            .unwrap();
        let domains: Vec<&str> = registry.marks().iter().map(|m| m.domain.as_str()).collect();
        assert_eq!(domains, vec!["dep-drift", "flaky-suite", "supply-chain"]);
        assert_eq!(
            registry.assign(mark(
                "dep-drift",
                "tool",
                Remediation::Instinctive {
                    procedure: "pin".into()
                }
            )),
            Err(invalid("domain is already marked"))
        );
        assert!(registry.mark("dep-drift").is_some());
        assert!(registry.mark("unknown").is_none());
        assert!(registry
            .assign(mark(
                "  ",
                "tool",
                Remediation::Instinctive {
                    procedure: "x".into()
                }
            ))
            .is_err());
    }

    #[test]
    fn inspect_stays_untriggered_without_class_and_subject_match() {
        let mut registry = ManmarkRegistry::empty();
        registry
            .assign(mark(
                "dep-drift",
                "tool",
                Remediation::Instinctive {
                    procedure: "pin".into(),
                },
            ))
            .unwrap();
        // Wrong class: untriggered even with the subject present.
        let wrong_class = evidence(EvidenceClass::CompilerResult, "dependency drift", 10);
        assert_eq!(
            registry.inspect("dep-drift", &wrong_class).unwrap(),
            MarkOutcome::Untriggered
        );
        // Right class, wrong subject: untriggered.
        let wrong_subject = evidence(EvidenceClass::TestReport, "all green", 20);
        assert_eq!(
            registry.inspect("dep-drift", &wrong_subject).unwrap(),
            MarkOutcome::Untriggered
        );
        let held = registry.mark("dep-drift").unwrap();
        assert_eq!(held.state, MarkState::Contained);
        assert_eq!(held.last_inspected_ms, 20);
        assert!(held.evidence_refs.is_empty());
    }

    #[test]
    fn a_breach_exposes_the_mark_and_seals_the_evidence() {
        let mut registry = ManmarkRegistry::empty();
        registry
            .assign(strategic("dep-drift", "sentinel", Severity::High))
            .unwrap();
        let breach = evidence(
            EvidenceClass::TestReport,
            "dependency drift in lockfile",
            30,
        );
        assert_eq!(
            registry.inspect("dep-drift", &breach).unwrap(),
            MarkOutcome::Breached
        );
        let exposed = registry.mark("dep-drift").unwrap();
        assert_eq!(exposed.state, MarkState::Exposed);
        assert_eq!(exposed.last_inspected_ms, 30);
        assert_eq!(exposed.evidence_refs, vec![breach.id.clone()]);
        // A second breach stays breached and does not duplicate the ref.
        assert_eq!(
            registry.inspect("dep-drift", &breach).unwrap(),
            MarkOutcome::Breached
        );
        assert_eq!(registry.mark("dep-drift").unwrap().evidence_refs.len(), 1);
    }

    #[test]
    fn a_clean_inspection_recontains_an_exposed_mark() {
        let mut registry = ManmarkRegistry::empty();
        registry
            .assign(mark(
                "dep-drift",
                "tool",
                Remediation::Instinctive {
                    procedure: "pin".into(),
                },
            ))
            .unwrap();
        registry
            .inspect(
                "dep-drift",
                &evidence(EvidenceClass::TestReport, "dependency drift", 10),
            )
            .unwrap();
        registry.record_success("dep-drift").unwrap();
        registry.record_success("dep-drift").unwrap();
        assert_eq!(registry.mark("dep-drift").unwrap().consecutive_successes, 2);
        // The remediation holds: a clean inspection recontains and restarts
        // the streak.
        let clean = evidence(EvidenceClass::TestReport, "pinned and green", 20);
        assert_eq!(
            registry.inspect("dep-drift", &clean).unwrap(),
            MarkOutcome::Recontained
        );
        let held = registry.mark("dep-drift").unwrap();
        assert_eq!(held.state, MarkState::Contained);
        assert_eq!(held.consecutive_successes, 0);
        assert_eq!(held.last_inspected_ms, 20);
    }

    #[test]
    fn instinctive_marks_never_escalate() {
        let mut registry = ManmarkRegistry::empty();
        registry
            .assign(strategic("covered", "sentinel", Severity::Low))
            .unwrap();
        registry
            .assign(mark(
                "dep-drift",
                "tool",
                Remediation::Instinctive {
                    procedure: "pin".into(),
                },
            ))
            .unwrap();
        // Below the escalation floor: in-drive coverage.
        assert_eq!(registry.escalation_request("covered").unwrap(), None);
        // A breach of an instinctive mark is immediate procedure, not
        // inference.
        registry
            .inspect(
                "dep-drift",
                &evidence(EvidenceClass::TestReport, "dependency drift", 10),
            )
            .unwrap();
        assert_eq!(registry.escalation_request("dep-drift").unwrap(), None);
        assert_eq!(
            registry.escalation_request("unknown"),
            Err(invalid("unknown manmark domain"))
        );
    }

    #[test]
    fn strategic_marks_escalate_at_high_severity_and_above() {
        let mut registry = ManmarkRegistry::empty();
        registry
            .assign(strategic("dep-drift", "sentinel", Severity::High))
            .unwrap();
        let breach = evidence(EvidenceClass::TestReport, "dependency drift", 10);
        registry.inspect("dep-drift", &breach).unwrap();
        let request = registry.escalation_request("dep-drift").unwrap().unwrap();
        assert_eq!(request.threat, "dep-drift");
        assert_eq!(request.severity, Severity::High);
        assert_eq!(request.evidence_refs, vec![breach.id]);
        assert_eq!(
            request.originating_event,
            EventId::new("mark-dep-drift").unwrap()
        );
        // Critical escalates the same way.
        let mut registry = ManmarkRegistry::empty();
        registry
            .assign(strategic("outage", "captain", Severity::Critical))
            .unwrap();
        assert!(registry.escalation_request("outage").unwrap().is_some());
    }

    #[test]
    fn graduation_demotes_remediation_after_threshold() {
        let mut registry = ManmarkRegistry::empty();
        registry
            .assign(strategic("dep-drift", "sentinel", Severity::Medium))
            .unwrap();
        for _ in 0..2 {
            registry.record_success("dep-drift").unwrap();
        }
        // Below the threshold: still strategic.
        assert_eq!(registry.graduate("dep-drift", 3).unwrap(), None);
        assert!(matches!(
            registry.mark("dep-drift").unwrap().remediation,
            Remediation::Strategic { .. }
        ));
        registry.record_success("dep-drift").unwrap();
        let procedure = registry.graduate("dep-drift", 3).unwrap().unwrap();
        assert_eq!(procedure, "graduated: dep-drift");
        let graduated = registry.mark("dep-drift").unwrap();
        assert_eq!(
            graduated.remediation,
            Remediation::Instinctive {
                procedure: procedure.clone()
            }
        );
        assert_eq!(graduated.consecutive_successes, 0);
        // Already instinctive: nothing left to graduate.
        assert_eq!(registry.graduate("dep-drift", 1).unwrap(), None);
    }

    #[test]
    fn inspect_of_unknown_domain_is_an_error() {
        let mut registry = ManmarkRegistry::empty();
        let observed = evidence(EvidenceClass::TestReport, "dependency drift", 10);
        assert_eq!(
            registry.inspect("dep-drift", &observed),
            Err(invalid("unknown manmark domain"))
        );
        assert_eq!(
            registry.record_success("dep-drift"),
            Err(invalid("unknown manmark domain"))
        );
        assert!(registry.graduate("dep-drift", 1).is_err());
    }

    #[test]
    fn registry_round_trips_through_json() {
        let mut registry = ManmarkRegistry::empty();
        registry
            .assign(strategic("dep-drift", "sentinel", Severity::High))
            .unwrap();
        registry
            .assign(mark(
                "supply-chain",
                "tool",
                Remediation::Instinctive {
                    procedure: "pin".into(),
                },
            ))
            .unwrap();
        registry
            .inspect(
                "dep-drift",
                &evidence(EvidenceClass::TestReport, "dependency drift", 42),
            )
            .unwrap();
        registry.record_success("supply-chain").unwrap();
        let serialized = serde_json::to_string(&registry).unwrap();
        let recovered: ManmarkRegistry = serde_json::from_str(&serialized).unwrap();
        assert_eq!(recovered, registry);
        assert_eq!(recovered.marks().len(), 2);
    }
}
