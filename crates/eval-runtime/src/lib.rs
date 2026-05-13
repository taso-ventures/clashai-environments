pub mod environment;

pub use environment::{
    EnvironmentAction, EnvironmentState, EnvironmentWinner, SequentialDecisionKind,
    SequentialPhase, SequentialState,
};

use serde::{Deserialize, Serialize};

/// Canonical information about the current turn state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnInfo {
    /// Current turn number (1-based).
    pub turn_number: u32,

    /// Name of the current phase (environment-specific).
    pub phase: String,

    /// Players whose input is required to advance.
    pub active_players: Vec<String>,

    /// Whether the environment has reached a terminal state.
    pub is_terminal: bool,

    /// Classification of the current decision: `"active"`, `"reactive"`, or `"forced"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_kind: Option<String>,

    /// Opaque revision tag for stale-state detection and replay fidelity
    /// (e.g. `"turn:5:phase:main"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_revision: Option<String>,

    /// Remaining step budget in milliseconds, when the caller wants to enforce
    /// a per-step deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_deadline_ms: Option<i64>,
}
