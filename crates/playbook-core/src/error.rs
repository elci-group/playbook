use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PlaybookError {
    #[error("identifier is empty or contains unsupported characters")]
    InvalidId,
    #[error("probability {0} is outside 0..=1000000")]
    Probability(u32),
    #[error("schema version {0} is not supported")]
    Schema(u32),
    #[error("evidence digest does not match its body")]
    DigestMismatch,
    #[error("waiver requires an authority and a reason")]
    WaiverAuthority,
    #[error("blind retry of {0} is prohibited until the field changes")]
    BlindRetry(String),
    #[error("repeated execution of {0} is stopped after authorization failure")]
    AuthorizationStop(String),
    #[error("side effect {0} is outside policy")]
    SideEffect(String),
    #[error("budget cannot increase without a new drive allocation")]
    BudgetIncrease,
    #[error("replay diverged: {0}")]
    ReplayDivergence(String),
    #[error("drive is already closed")]
    Closed,
    #[error("mandatory gates are still open")]
    Incomplete,
    #[error("idempotency key {0} must be reconciled before retry")]
    Reconcile(String),
    #[error("idempotency key {0} already committed")]
    DuplicateEffect(String),
    #[error("unknown gate {0}")]
    UnknownGate(String),
    #[error("evidence rejected: {0}")]
    Evidence(String),
    #[error("capability snapshot is stale")]
    StaleCapability,
    #[error("play {0} is not admissible: {1}")]
    Inadmissible(String, String),
    #[error("execution exceeded its budget ceiling")]
    BudgetCeiling,
    #[error("drive budget is exhausted")]
    BudgetExhausted,
    #[error("swarm play has no expected marginal gain")]
    SwarmUnjustified,
    #[error("preflight is required before this play")]
    PreflightRequired,
    #[error("serialized continuation leaked competitive metadata")]
    AmnesiaLeak,
    #[error("no admissible play")]
    NoPlay,
    #[error("fourth-down disposition does not match the doctrine")]
    DispositionMismatch,
    #[error("punt requires a fourth-down punt disposition")]
    PuntNotReady,
    #[error("json: {0}")]
    Json(String),
}

impl PlaybookError {
    pub fn json(err: serde_json::Error) -> Self {
        Self::Json(err.to_string())
    }
}
