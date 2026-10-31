//! Offline, deterministic portfolio construction. No execution-plane dependency.
//!
//! Inputs describe closed seasons and deliberately lossy workload classes. No
//! repository, live task, drive state, tool handle or execution callback is accepted.
#![forbid(unsafe_code)]

use playbook_core::{sha256_hex, to_canonical_bytes, PersonnelId, PlaybookError, Ppm, PPM_SCALE};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskClass {
    RepositoryAnalysis,
    CodeRepair,
    TechnicalResearch,
    LongContextSynthesis,
    Reconnaissance,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolClass {
    Compiler,
    TestRunner,
    Search,
    Browser,
    Shell,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Currency {
    Gbp,
    Usd,
    Eur,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpponentProfile {
    pub task_class: TaskClass,
    pub weight_ppm: u32,
    pub novelty_ppm: u32,
    pub context_tokens: u64,
    pub tool_intensity_ppm: u32,
    pub reasoning_depth_ppm: u32,
    pub coding_requirement_ppm: u32,
    pub required_tools: Vec<ToolClass>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalPerformance {
    pub task_class: TaskClass,
    pub through_season: u64,
    pub samples: u32,
    pub success_ppm: u32,
    pub divcon_upset_ppm: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub personnel_id: PersonnelId,
    pub provider: String,
    /// Shared upstream infrastructure and rate-limit pools, not just brand names.
    pub failure_domains: BTreeSet<String>,
    pub cost_micros_per_workload: u64,
    pub quota_workloads: u64,
    pub latency_ms: u64,
    pub reliability_ppm: u32,
    pub price_volatility_ppm: u32,
    pub context_tokens: u64,
    pub reasoning_ppm: u32,
    pub coding_ppm: u32,
    pub tool_use_ppm: u32,
    pub tools: BTreeSet<ToolClass>,
    pub offline: bool,
    pub history: Vec<HistoricalPerformance>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortfolioInput {
    pub effective_season: u64,
    pub evidence_through_season: u64,
    pub currency: Currency,
    pub budget_micros_per_workload: u64,
    pub maximum_members: usize,
    pub minimum_depth: usize,
    pub minimum_providers: usize,
    pub minimum_reliability_ppm: u32,
    pub minimum_samples: u32,
    pub maximum_latency_ms: u64,
    pub require_offline_fallback: bool,
    pub workloads: Vec<OpponentProfile>,
    pub candidates: Vec<Candidate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    Reliability,
    Latency,
    Quota,
    Cost,
    NoHistoricalCoverage,
    MarginalRosterValue,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusion {
    pub personnel_id: PersonnelId,
    pub reason: ExclusionReason,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RosterEntry {
    pub personnel_id: PersonnelId,
    pub provider: String,
    pub failure_domains: BTreeSet<String>,
    pub eligible_formations: Vec<TaskClass>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RosterPlan {
    pub effective_season: u64,
    pub evidence_through_season: u64,
    pub input_digest: String,
    pub currency: Currency,
    pub projected_cost_micros_per_workload: u64,
    pub coverage_ppm: u32,
    pub redundancy_ppm: u32,
    pub diversification_ppm: u32,
    pub minimum_depth: usize,
    pub minimum_providers: usize,
    pub members: Vec<RosterEntry>,
    pub eligible_formations: Vec<TaskClass>,
    pub ineligible: Vec<Exclusion>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RosterContract {
    pub magic: String,
    pub digest: String,
    pub plan: RosterPlan,
}

fn invalid(message: &str) -> PlaybookError {
    PlaybookError::Evidence(message.into())
}
fn ppm(value: u32) -> Result<(), PlaybookError> {
    Ppm::new(value).map(|_| ())
}

impl RosterContract {
    pub fn verify(&self) -> Result<(), PlaybookError> {
        if self.magic != "ROSTER/1" || self.digest != sha256_hex(&to_canonical_bytes(&self.plan)?) {
            return Err(PlaybookError::DigestMismatch);
        }
        let plan = &self.plan;
        if plan.members.len() > 16
            || plan.ineligible.len() > 16
            || plan.eligible_formations.is_empty()
            || plan.eligible_formations.len() > 5
            || plan.effective_season <= plan.evidence_through_season
            || plan.members.is_empty()
            || plan.minimum_depth == 0
            || plan.minimum_providers == 0
        {
            return Err(invalid("roster must use evidence from earlier seasons"));
        }
        for value in [
            plan.coverage_ppm,
            plan.redundancy_ppm,
            plan.diversification_ppm,
        ] {
            ppm(value)?;
        }
        let mut ids = BTreeSet::new();
        let mut providers = BTreeSet::new();
        for member in &plan.members {
            PersonnelId::new(member.personnel_id.as_str())?;
            PersonnelId::new(&member.provider)?;
            providers.insert(&member.provider);
            if member.failure_domains.is_empty() || member.failure_domains.len() > 16 {
                return Err(invalid("failure domains are required"));
            }
            for domain in &member.failure_domains {
                PersonnelId::new(domain)?;
            }
            if member.eligible_formations.len() > 5
                || !ids.insert(member.personnel_id.as_str())
                || member.eligible_formations.is_empty()
                || member
                    .eligible_formations
                    .iter()
                    .any(|f| !plan.eligible_formations.contains(f))
            {
                return Err(invalid(
                    "roster has duplicate membership or invalid formation coverage",
                ));
            }
        }
        if providers.len() < plan.minimum_providers {
            return Err(invalid("roster does not meet provider diversification"));
        }
        for formation in &plan.eligible_formations {
            if plan
                .members
                .iter()
                .filter(|m| m.eligible_formations.contains(formation))
                .count()
                < plan.minimum_depth
            {
                return Err(invalid("roster does not meet its depth contract"));
            }
        }
        Ok(())
    }
}

fn validate(input: &PortfolioInput) -> Result<(), PlaybookError> {
    if input.effective_season <= input.evidence_through_season
        || input.workloads.is_empty()
        || input.workloads.len() > 5
        || input.candidates.is_empty()
        || input.candidates.len() > 16
        || input.maximum_members == 0
        || input.maximum_members > 16
        || input.minimum_depth == 0
        || input.minimum_depth > input.maximum_members
        || input.minimum_providers == 0
        || input.minimum_providers > input.maximum_members
        || input.minimum_samples == 0
        || input.budget_micros_per_workload == 0
        || input.maximum_latency_ms == 0
    {
        return Err(invalid("invalid portfolio bounds or concurrent evidence"));
    }
    ppm(input.minimum_reliability_ppm)?;
    let mut formations = BTreeSet::new();
    let mut total_weight = 0u64;
    for profile in &input.workloads {
        if profile.required_tools.len() > 5 {
            return Err(invalid("too many required tool classes"));
        }
        if !formations.insert(profile.task_class) || profile.weight_ppm == 0 {
            return Err(invalid("duplicate or unweighted workload class"));
        }
        for value in [
            profile.weight_ppm,
            profile.novelty_ppm,
            profile.tool_intensity_ppm,
            profile.reasoning_depth_ppm,
            profile.coding_requirement_ppm,
        ] {
            ppm(value)?;
        }
        total_weight += u64::from(profile.weight_ppm);
    }
    if total_weight != u64::from(PPM_SCALE) {
        return Err(invalid("workload weights must sum to one million"));
    }
    let mut ids = BTreeSet::new();
    for candidate in &input.candidates {
        PersonnelId::new(candidate.personnel_id.as_str())?;
        if !ids.insert(&candidate.personnel_id)
            || candidate.failure_domains.is_empty()
            || candidate.failure_domains.len() > 16
        {
            return Err(invalid(
                "candidate ids must be unique and failure domains explicit",
            ));
        }
        PersonnelId::new(&candidate.provider)?;
        for domain in &candidate.failure_domains {
            PersonnelId::new(domain)?;
        }
        for value in [
            candidate.reliability_ppm,
            candidate.price_volatility_ppm,
            candidate.reasoning_ppm,
            candidate.coding_ppm,
            candidate.tool_use_ppm,
        ] {
            ppm(value)?;
        }
        let mut history_classes = BTreeSet::new();
        for history in &candidate.history {
            if history.through_season > input.evidence_through_season
                || !history_classes.insert(history.task_class)
            {
                return Err(invalid("history is concurrent, future or duplicated"));
            }
            ppm(history.success_ppm)?;
            ppm(history.divcon_upset_ppm)?;
        }
    }
    Ok(())
}

fn fit(candidate: &Candidate, profile: &OpponentProfile, min_samples: u32) -> u32 {
    let Some(history) = candidate
        .history
        .iter()
        .find(|h| h.task_class == profile.task_class && h.samples >= min_samples)
    else {
        return 0;
    };
    if candidate.context_tokens < profile.context_tokens
        || profile
            .required_tools
            .iter()
            .any(|tool| !candidate.tools.contains(tool))
    {
        return 0;
    }
    let ratio = |have: u32, need: u32| {
        if need == 0 {
            u64::from(PPM_SCALE)
        } else {
            (u64::from(have) * u64::from(PPM_SCALE) / u64::from(need)).min(u64::from(PPM_SCALE))
        }
    };
    let adequacy = ratio(candidate.reasoning_ppm, profile.reasoning_depth_ppm)
        .min(ratio(candidate.coding_ppm, profile.coding_requirement_ppm))
        .min(ratio(candidate.tool_use_ppm, profile.tool_intensity_ppm));
    let empirical = (u64::from(history.success_ppm) * 9 + u64::from(history.divcon_upset_ppm)) / 10;
    (empirical * u64::from(candidate.reliability_ppm) / u64::from(PPM_SCALE) * adequacy
        / u64::from(PPM_SCALE)
        * u64::from(PPM_SCALE - profile.novelty_ppm / 4)
        / u64::from(PPM_SCALE)) as u32
}

fn exclusion(candidate: &Candidate, input: &PortfolioInput) -> Option<ExclusionReason> {
    if candidate.reliability_ppm < input.minimum_reliability_ppm {
        Some(ExclusionReason::Reliability)
    } else if candidate.latency_ms > input.maximum_latency_ms {
        Some(ExclusionReason::Latency)
    } else if candidate.quota_workloads == 0 {
        Some(ExclusionReason::Quota)
    } else if candidate.cost_micros_per_workload > input.budget_micros_per_workload {
        Some(ExclusionReason::Cost)
    } else if input
        .workloads
        .iter()
        .all(|p| fit(candidate, p, input.minimum_samples) == 0)
    {
        Some(ExclusionReason::NoHistoricalCoverage)
    } else {
        None
    }
}

struct EvaluatedPortfolio {
    score: i64,
    cost: u64,
    selected: Vec<usize>,
    coverage: u32,
    redundancy: u32,
    diversification: u32,
}

/// Exhaustively compare portfolios of at most 16 candidates. Scores are
/// counterfactual proxies, not measured completion probabilities.
pub fn construct(input: &PortfolioInput) -> Result<RosterContract, PlaybookError> {
    validate(input)?;
    let mut candidates: Vec<_> = input
        .candidates
        .iter()
        .filter(|c| exclusion(c, input).is_none())
        .collect();
    candidates.sort_by(|a, b| a.personnel_id.cmp(&b.personnel_id));
    let mut best: Option<EvaluatedPortfolio> = None;
    for mask in 1u32..(1u32 << candidates.len()) {
        if mask.count_ones() as usize > input.maximum_members {
            continue;
        }
        let selected: Vec<usize> = (0..candidates.len())
            .filter(|i| mask & (1 << i) != 0)
            .collect();
        let Some(cost) = selected.iter().try_fold(0u64, |sum, i| {
            sum.checked_add(candidates[*i].cost_micros_per_workload)
        }) else {
            continue;
        };
        if cost > input.budget_micros_per_workload {
            continue;
        }
        let providers: BTreeSet<_> = selected.iter().map(|i| &candidates[*i].provider).collect();
        if providers.len() < input.minimum_providers {
            continue;
        }
        let mut coverage = 0u64;
        let mut redundancy = 0u64;
        let mut feasible = true;
        for profile in &input.workloads {
            let mut scores: Vec<_> = selected
                .iter()
                .map(|i| (fit(candidates[*i], profile, input.minimum_samples), *i))
                .filter(|(score, _)| *score > 0)
                .collect();
            scores.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            if scores.len() < input.minimum_depth
                || (input.require_offline_fallback
                    && !scores.iter().any(|(_, i)| candidates[*i].offline))
            {
                feasible = false;
                break;
            }
            coverage +=
                u64::from(scores[0].0) * u64::from(profile.weight_ppm) / u64::from(PPM_SCALE);
            let depth_score: u64 = scores
                .iter()
                .take(input.minimum_depth)
                .map(|(score, _)| u64::from(*score))
                .sum::<u64>()
                / input.minimum_depth as u64;
            redundancy += depth_score * u64::from(profile.weight_ppm) / u64::from(PPM_SCALE);
        }
        if !feasible {
            continue;
        }
        let mut independent = 0u64;
        let mut pairs = 0u64;
        for (at, i) in selected.iter().enumerate() {
            for j in &selected[at + 1..] {
                pairs += 1;
                if candidates[*i].provider != candidates[*j].provider
                    && candidates[*i]
                        .failure_domains
                        .is_disjoint(&candidates[*j].failure_domains)
                {
                    independent += 1;
                }
            }
        }
        let diversification = (independent * u64::from(PPM_SCALE))
            .checked_div(pairs)
            .unwrap_or(0);
        let volatility: u64 = selected
            .iter()
            .map(|i| u64::from(candidates[*i].price_volatility_ppm))
            .sum::<u64>()
            / selected.len() as u64;
        let cost_ratio = (u128::from(cost) * u128::from(PPM_SCALE)
            / u128::from(input.budget_micros_per_workload)) as u64;
        let score = (coverage * 60 + redundancy * 25 + diversification * 15) as i64
            - (cost_ratio * 5 + volatility * 5) as i64;
        let better = best.as_ref().is_none_or(|old| {
            score > old.score
                || (score == old.score
                    && (cost < old.cost
                        || (cost == old.cost
                            && (selected.len() < old.selected.len()
                                || (selected.len() == old.selected.len()
                                    && selected < old.selected)))))
        });
        if better {
            best = Some(EvaluatedPortfolio {
                score,
                cost,
                selected,
                coverage: coverage as u32,
                redundancy: redundancy as u32,
                diversification: diversification as u32,
            });
        }
    }
    let EvaluatedPortfolio {
        cost,
        selected,
        coverage,
        redundancy,
        diversification,
        ..
    } = best.ok_or_else(|| {
        invalid("no portfolio satisfies budget, coverage, depth and diversity constraints")
    })?;
    let members: Vec<_> = selected
        .iter()
        .map(|i| RosterEntry {
            personnel_id: candidates[*i].personnel_id.clone(),
            provider: candidates[*i].provider.clone(),
            failure_domains: candidates[*i].failure_domains.clone(),
            eligible_formations: input
                .workloads
                .iter()
                .filter(|p| fit(candidates[*i], p, input.minimum_samples) > 0)
                .map(|p| p.task_class)
                .collect(),
        })
        .collect();
    let ineligible = input
        .candidates
        .iter()
        .filter(|c| !members.iter().any(|m| m.personnel_id == c.personnel_id))
        .map(|c| Exclusion {
            personnel_id: c.personnel_id.clone(),
            reason: exclusion(c, input).unwrap_or(ExclusionReason::MarginalRosterValue),
        })
        .collect();
    let plan = RosterPlan {
        effective_season: input.effective_season,
        evidence_through_season: input.evidence_through_season,
        input_digest: sha256_hex(&to_canonical_bytes(input)?),
        currency: input.currency,
        projected_cost_micros_per_workload: cost,
        coverage_ppm: coverage,
        redundancy_ppm: redundancy,
        diversification_ppm: diversification,
        minimum_depth: input.minimum_depth,
        minimum_providers: input.minimum_providers,
        members,
        eligible_formations: input.workloads.iter().map(|p| p.task_class).collect(),
        ineligible,
    };
    let contract = RosterContract {
        magic: "ROSTER/1".into(),
        digest: sha256_hex(&to_canonical_bytes(&plan)?),
        plan,
    };
    contract.verify()?;
    Ok(contract)
}
