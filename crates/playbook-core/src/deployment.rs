//! Deployment status: the roster dimension answering *on which side of
//! the ball a player should normally be deployed* — a separate axis from
//! health ("can this player be selected?"). One-way status is never
//! weakness: it can mean *too strategically valuable or too constrained
//! to spend indiscriminately*.
//!
//! Offensive snaps consume inference to advance the state of the work
//! toward a deliver gate: plan → implement → repair → synthesize → decide
//! → deliver. Defensive snaps consume inference principally to prevent
//! loss: review → verify → attack → inspect → test → audit → reject.
//! Every inference call therefore carries a side-of-ball classification
//! before roster selection, and selection becomes: given the franchise
//! doctrine, game state, roster health and remaining resources, which
//! *eligible* player takes *this* snap?

use serde::{Deserialize, Serialize};

/// On which side of the ball a snap deploys the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideOfBall {
    Offense,
    Defense,
}

impl SideOfBall {
    pub fn key(self) -> &'static str {
        match self {
            Self::Offense => "offense",
            Self::Defense => "defense",
        }
    }
}

/// The registered deployment status of a roster player.
///
/// ONE-WAY OFFENSE: reserved primarily for progression, synthesis,
/// decisive execution — typically a powerful but quota-scarce frontier
/// model. ONE-WAY DEFENSE: reserved primarily for critique, verification,
/// interception of bad plans — a strong evaluator with constrained usage.
/// TWO-WAY: can sustainably participate on either side — the general-purpose
/// starter, intrinsically valuable for its *optionality* even when not the
/// absolute strongest. SPECIAL TEAMS: a narrow specialist (tool-use, vision,
/// huge-context) deployed situationally rather than as a normal side player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    OneWayOffense,
    OneWayDefense,
    TwoWay,
    SpecialTeams,
}

impl DeploymentStatus {
    pub fn key(self) -> &'static str {
        match self {
            Self::OneWayOffense => "one_way_offense",
            Self::OneWayDefense => "one_way_defense",
            Self::TwoWay => "two_way",
            Self::SpecialTeams => "special_teams",
        }
    }

    /// Whether the player may take snaps on the given side at all under
    /// their registered status. Two-way players qualify on both sides —
    /// the source of their optionality value; one-way players on exactly
    /// their own.
    pub fn may_play(self, side: SideOfBall) -> bool {
        match self {
            Self::TwoWay => true,
            Self::SpecialTeams => false, // situational deployment, never a normal snap
            Self::OneWayOffense => side == SideOfBall::Offense,
            Self::OneWayDefense => side == SideOfBall::Defense,
        }
    }
}

/// The intended character of one inference snap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapIntent {
    Plan,
    Implement,
    Repair,
    Synthesize,
    Decide,
    Deliver,
    Review,
    Verify,
    Attack,
    Inspect,
    Test,
    Audit,
    Reject,
}

impl SnapIntent {
    /// The progressive (work-advancing) utility of this snap.
    pub fn progressive_utility(self) -> u32 {
        match self {
            Self::Plan | Self::Implement | Self::Repair => 70,
            Self::Synthesize | Self::Decide | Self::Deliver => 90,
            _ => 10,
        }
    }

    /// The protective (loss-preventing) utility of this snap.
    pub fn protective_utility(self) -> u32 {
        match self {
            Self::Review | Self::Inspect => 60,
            Self::Verify | Self::Test => 80,
            Self::Attack | Self::Audit => 90,
            Self::Reject => 70,
            _ => 10,
        }
    }

    /// Classify the snap by expected utility: progressive utility greater
    /// than protective utility → OFFENSE; protective greater than
    /// progressive → DEFENSE. No tie exists in the schedule above.
    pub fn side(self) -> SideOfBall {
        if self.progressive_utility() > self.protective_utility() {
            SideOfBall::Offense
        } else {
            SideOfBall::Defense
        }
    }
}

/// A player's present physical/workload state — the *health* axis.
/// Recovery levels contract deployment without rewriting identity (the
/// registered status stands; the *effective* status is derived).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fitness {
    Fit,
    /// Recovering: R0 fully contracted → R3 nearly restored.
    Recovery(u8),
    Out,
}

impl Fitness {
    pub fn key(self) -> String {
        match self {
            Self::Fit => "fit".into(),
            Self::Recovery(level) => format!("recovery_r{level}"),
            Self::Out => "out".into(),
        }
    }

    /// Whether the player may be selected for a snap at all.
    pub fn selectable(self) -> bool {
        !matches!(self, Self::Out)
    }
}

/// Eligibility is separate from health: a fit player can still be
/// ineligible (suspended by policy, contractually excluded).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Eligibility {
    Eligible,
    Suspended,
}

/// Workload character: how the player may be spent across the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Workload {
    /// High-volume sustainable use.
    Starter,
    /// Short, violent bursts only.
    Restricted,
    /// Situational.
    Situation,
}

/// The effective deployment of a player for the current game situation —
/// derived from registered status, recovery state, franchise policy and
/// game situation. Recovery contracts the *effective* role; it never
/// rewrites the player's registered roster identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveDeployment {
    pub registered: DeploymentStatus,
    /// What the player may actually do this game.
    pub effective: DeploymentStatus,
    pub side_restriction: Option<SideOfBall>,
    pub rationale: &'static str,
}

/// Derive the effective deployment from the registered status, fitness
/// and the franchise doctrine's red-zone restriction level (how deep into
/// recovery a player must be before they may take offensive snaps).
pub fn effective_deployment(
    registered: DeploymentStatus,
    fitness: Fitness,
    red_zone_threshold: u8,
) -> EffectiveDeployment {
    if !fitness.selectable() {
        return EffectiveDeployment {
            registered,
            effective: DeploymentStatus::SpecialTeams, // cannot play normal snaps
            side_restriction: Some(SideOfBall::Offense),
            rationale: "out: may not take snaps",
        };
    }
    match fitness {
        Fitness::Fit => EffectiveDeployment {
            registered,
            effective: registered,
            side_restriction: None,
            rationale: "fit: full registered deployment available",
        },
        Fitness::Recovery(level) => {
            // Recovery contracts effective deployment, but one-way players
            // retain their registered side — only two-way contracts.
            let (effective, side) = match registered {
                DeploymentStatus::TwoWay => {
                    if level >= red_zone_threshold {
                        (DeploymentStatus::OneWayOffense, Some(SideOfBall::Offense))
                    } else {
                        (DeploymentStatus::OneWayDefense, Some(SideOfBall::Defense))
                    }
                }
                DeploymentStatus::OneWayOffense => {
                    (DeploymentStatus::OneWayOffense, Some(SideOfBall::Offense))
                }
                DeploymentStatus::OneWayDefense => {
                    (DeploymentStatus::OneWayDefense, Some(SideOfBall::Defense))
                }
                DeploymentStatus::SpecialTeams => (DeploymentStatus::SpecialTeams, None),
            };
            EffectiveDeployment {
                registered,
                effective,
                side_restriction: side,
                rationale: if level >= red_zone_threshold {
                    "recovery deep enough for red-zone offense only"
                } else {
                    "recovery: contracted to defensive deployment until restored"
                },
            }
        }
        Fitness::Out => unreachable!("handled by selectable"),
    }
}

/// The combined player card: every roster dimension in one projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerCard {
    pub player: String,
    pub provider: String,
    pub position: String,
    pub deployment: DeploymentStatus,
    pub fitness: Fitness,
    pub eligibility: Eligibility,
    pub workload: Workload,
}

impl PlayerCard {
    /// Whether the card may be selected for a snap on the given side:
    /// eligible, selectable, and permitted by both registered status and
    /// the recovery-derived effective status.
    pub fn may_take_snap(&self, side: SideOfBall, red_zone_threshold: u8) -> bool {
        if !matches!(self.eligibility, Eligibility::Eligible) || !self.fitness.selectable() {
            return false;
        }
        if !self.deployment.may_play(side) {
            return false;
        }
        let effective = effective_deployment(self.deployment, self.fitness, red_zone_threshold);
        effective.effective.may_play(side)
    }

    /// The one-line card rendering (the depth chart's cell).
    pub fn render(&self) -> String {
        format!(
            "{}/{}\n  position    {}\n  deployment  {}\n  health      {}\n  eligibility {}\n  workload    {}",
            self.player,
            self.provider,
            self.position,
            self.deployment.key(),
            self.fitness.key(),
            match self.eligibility {
                Eligibility::Eligible => "eligible",
                Eligibility::Suspended => "suspended",
            },
            match self.workload {
                Workload::Starter => "high-volume starter",
                Workload::Restricted => "burst only",
                Workload::Situation => "situational",
            },
        )
    }
}

/// The franchise's strategic doctrine: how inference is allocated between
/// offense and defense.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FranchiseDoctrine {
    /// Share of snaps (in percent) allocated to offense.
    pub offensive_share: u32,
    pub validation_intensity: u32,
    /// The recovery level at or above which red-zone offense is permitted.
    pub red_zone_threshold: u8,
}

impl FranchiseDoctrine {
    /// Defensive fortress: 30% offense, heavy validation, long defensive
    /// periods by sustainable two-way players with short violent bursts
    /// from premium one-way offense.
    pub fn defensive_fortress() -> Self {
        Self {
            offensive_share: 30,
            validation_intensity: 90,
            red_zone_threshold: 2,
        }
    }

    /// Aggressive defense + methodical offense: premium models defend to
    /// kill bad approaches early; cheap two-way models progress.
    pub fn aggressive_defense() -> Self {
        Self {
            offensive_share: 40,
            validation_intensity: 85,
            red_zone_threshold: 3,
        }
    }

    /// Air raid: 75% offense, lightweight defensive validation between plays.
    pub fn air_raid() -> Self {
        Self {
            offensive_share: 75,
            validation_intensity: 40,
            red_zone_threshold: 1,
        }
    }

    /// Ball control: two-way players alternate both sides; predictable cost.
    pub fn ball_control() -> Self {
        Self {
            offensive_share: 50,
            validation_intensity: 60,
            red_zone_threshold: 2,
        }
    }

    pub fn defensive_share(self) -> u32 {
        100 - self.offensive_share
    }
}

/// The selection pipeline: snap classified → side → eligible roster
/// subset → fitness filter → tactical selection. Returns the eligible
/// pool for the snap — the subset the tactical optimizer evaluates,
/// instead of every available route.
pub fn eligible_pool<'a>(
    roster: &'a [PlayerCard],
    intent: SnapIntent,
    doctrine: &FranchiseDoctrine,
) -> Vec<&'a PlayerCard> {
    let side = intent.side();
    roster
        .iter()
        .filter(|card| card.may_take_snap(side, doctrine.red_zone_threshold))
        .collect()
}

/// Roster utility: the multiplicative value of a player to the franchise.
/// Two-way players gain optionality value through the deployment
/// permission term — they reduce substitution and replanning costs.
pub fn roster_utility(card: &PlayerCard, doctrine: &FranchiseDoctrine) -> u64 {
    let task_fitness: u64 = match card.workload {
        Workload::Starter => 100,
        Workload::Restricted => 90,
        Workload::Situation => 50,
    };
    let availability: u64 = if card.fitness.selectable() { 100 } else { 0 };
    let deployment_permission: u64 = match card.deployment {
        DeploymentStatus::TwoWay => 100, // optionality: both candidate pools
        DeploymentStatus::OneWayOffense => {
            if doctrine.offensive_share >= 50 {
                80
            } else {
                60
            }
        }
        DeploymentStatus::OneWayDefense => {
            if doctrine.defensive_share() >= 50 {
                80
            } else {
                60
            }
        }
        DeploymentStatus::SpecialTeams => 30,
    };
    let sustainability: u64 = match card.workload {
        Workload::Starter => 100,
        Workload::Restricted => 40,
        Workload::Situation => 60,
    };
    let tactical_value: u64 = match card.position.as_str() {
        "qb" | "offensive_kicker" => 110,
        _ => 100,
    };
    task_fitness
        .saturating_mul(availability)
        .saturating_mul(deployment_permission)
        .saturating_mul(sustainability)
        .saturating_mul(tactical_value)
}

/// The depth chart: the roster projected by position under a doctrine —
/// one utility-ranked line per position group.
pub fn depth_chart(
    roster: &[PlayerCard],
    doctrine: &FranchiseDoctrine,
) -> Vec<(String, Vec<(String, u64)>)> {
    let mut groups: std::collections::BTreeMap<String, Vec<(String, u64)>> =
        std::collections::BTreeMap::new();
    for card in roster {
        groups
            .entry(card.position.clone())
            .or_default()
            .push((card.player.clone(), roster_utility(card, doctrine)));
    }
    let mut chart: Vec<(String, Vec<(String, u64)>)> = groups.into_iter().collect();
    for (_, line) in chart.iter_mut() {
        line.sort_by_key(|k| std::cmp::Reverse(k.1));
    }
    chart
}
