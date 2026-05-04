//! Environment-agnostic engine trait and implementations.
//!
//! This crate defines [`Environment`], a synchronous trait that wraps
//! any turn-based environment behind a uniform interface. Each game
//! implements this trait to provide:
//!
//! - Environment lifecycle (create, apply actions, check terminal state)
//! - Player-filtered state views (fog of war)
//! - Legal action enumeration
//! - Rankings derivation
//!
//! # Feature flags
//!
//! Each environment is gated behind its own feature (`coup`, `vibe_check`,
//! `wordle`, `tic_tac_toe`, `connect_four`, `red_button`, `poker`); all are
//! enabled by default.

#[cfg(feature = "connect_four")]
pub mod connect_four;
#[cfg(feature = "coup")]
pub mod coup;
#[cfg(feature = "poker")]
pub mod poker;
#[cfg(feature = "red_button")]
pub mod red_button;
pub mod registry;
#[cfg(feature = "tic_tac_toe")]
pub mod tic_tac_toe;
#[cfg(feature = "vibe_check")]
pub mod vibe_check;
#[cfg(feature = "wordle")]
pub mod wordle;

pub use eval_runtime::TurnInfo;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors produced by [`Environment`] operations.
#[derive(Debug, Error)]
pub enum EnvironmentError {
    /// The environment could not be created with the given parameters.
    #[error("invalid setup: {0}")]
    InvalidSetup(String),

    /// The submitted action is not legal in the current state.
    #[error("invalid action: {0}")]
    InvalidAction(String),

    /// The specified player does not exist in this environment.
    #[error("unknown player: {0}")]
    UnknownPlayer(String),

    /// The environment has already ended; no further actions are accepted.
    #[error("environment already terminated")]
    AlreadyTerminated,

    /// A serialization or deserialization error occurred.
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// An internal environment error not covered by other variants.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Result alias for [`EnvironmentError`].
pub type Result<T> = std::result::Result<T, EnvironmentError>;

/// Final ranking entry for a player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRanking {
    /// Player identifier (environment-specific string).
    pub player_id: String,

    /// 1-based rank (1 = winner).
    pub rank: u32,
}

/// Controls what event data an agent owner can see after a match ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostMatchVisibility {
    /// Competitive: owner sees only their own agent's events.
    OwnOnly,
    /// Educational: owner sees all events after the match ends.
    Full,
}

/// Environment-agnostic trait for environments.
///
/// Implementations wrap a concrete environment engine (e.g. `CoupGame`) and
/// expose it through a uniform JSON-based interface. All state and
/// action payloads are serialized as [`serde_json::Value`] so the
/// harness and tool executor remain environment-agnostic.
///
/// The trait is **not** async because environments are pure-logic,
/// in-process computations with no I/O.
pub trait Environment: Send + Sync {
    /// Unique identifier for this environment type (e.g. `"coup"`).
    fn environment_type(&self) -> &str;

    /// Human-readable display name (e.g. `"Coup"`).
    fn display_name(&self) -> &str;

    /// Minimum number of players supported.
    fn min_players(&self) -> usize;

    /// Maximum number of players supported.
    fn max_players(&self) -> usize;

    /// Return the current state filtered for the given player.
    ///
    /// Hidden information belonging to other players must be redacted.
    fn state_for_player(&self, player_id: &str) -> Result<serde_json::Value>;

    /// Return the full (unfiltered) state. Use only for spectators/admin.
    fn full_state(&self) -> Result<serde_json::Value>;

    /// Return information about the current turn.
    fn turn_info(&self) -> Result<TurnInfo>;

    /// Return the legal actions available to a player as a JSON array.
    fn legal_actions(&self, player_id: &str) -> Result<serde_json::Value>;

    /// Apply an action submitted by a player.
    ///
    /// `action` is a JSON value whose schema is environment-specific.
    /// Returns the outcome as a JSON value.
    fn apply_action(
        &mut self,
        player_id: &str,
        action: &serde_json::Value,
    ) -> Result<serde_json::Value>;

    /// Whether the environment has reached a terminal state.
    fn is_terminal(&self) -> bool;

    /// Return final rankings when the environment is terminal.
    ///
    /// Returns `None` if the environment is still in progress.
    fn rankings(&self) -> Option<Vec<PlayerRanking>>;

    /// Return the rules/instructions for this environment as markdown text.
    fn rules_markdown(&self) -> &str;

    /// Return a list of player IDs in this environment.
    fn player_ids(&self) -> Vec<String>;
}

/// Configuration for creating a new environment instance.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentConfig {
    /// Number of players.
    pub player_count: usize,

    /// RNG seed for deterministic replay.
    pub seed: u64,

    /// Optional environment-specific configuration as JSON.
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,

    /// Optional match identifier (used by environments that need a stable ID).
    #[serde(default)]
    pub match_id: Option<String>,

    /// Optional explicit player IDs. When absent, IDs are generated as
    /// stringified `0..player_count` by convention. Built-in numeric games parse
    /// these strings at adapter boundaries.
    #[serde(default)]
    pub player_ids: Option<Vec<String>>,

    /// Optional player display-name map (`player_id` -> name).
    #[serde(default)]
    pub player_names: Option<HashMap<String, String>>,
}
