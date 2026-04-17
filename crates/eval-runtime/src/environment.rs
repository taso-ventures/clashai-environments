//! Shared environment trait and phase types used across all game protocols.
//!
//! These definitions describe any sequential-turn game environment:
//! a [`EnvironmentState`] that the server exposes, an [`EnvironmentAction`]
//! players submit, and a [`SequentialPhase`] describing whose turn it is
//! and what kind of decision is pending.

use std::fmt::Debug;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Base trait for the state of any competitive game environment.
pub trait EnvironmentState:
    Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + Debug + 'static
{
    /// Player identifier type for this environment.
    type PlayerId: Clone + Send + Sync + Debug;

    /// Current turn number.
    fn turn_number(&self) -> u32;

    /// Current phase label (e.g., `"movement"`, `"guessing"`, `"resolution"`).
    fn current_phase(&self) -> &str;

    /// All player IDs participating in the environment.
    fn player_ids(&self) -> Vec<Self::PlayerId>;

    /// Whether the environment has reached a terminal state.
    fn is_terminal(&self) -> bool {
        false
    }
}

/// Describes the winner(s) of a completed environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvironmentWinner<P> {
    /// Single player won (e.g., last player standing).
    Player(P),
    /// A team won — contains all player IDs on the winning team.
    Team(Vec<P>),
    /// Draw / no winner.
    Draw,
}

/// Decision kind for sequential-turn environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequentialDecisionKind {
    /// Primary turn action from the active player.
    Active,
    /// Reactive decision (challenge/block/pass windows).
    Reactive,
    /// Forced decision (must respond to resolve state, e.g., reveal/lose/exchange).
    Forced,
}

/// Phase information for sequential-turn environments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequentialPhase<P> {
    /// One or more players must provide a decision.
    Decision {
        kind: SequentialDecisionKind,
        players: Vec<P>,
        deadline: Option<DateTime<Utc>>,
    },
    /// Game is resolving internally (no decisions expected).
    Resolving,
    /// Game over with winner information.
    GameOver { winner: EnvironmentWinner<P> },
}

/// Extension trait for sequential-turn environments.
pub trait SequentialState: EnvironmentState {
    /// Return the current sequential phase and expected decision set.
    fn sequential_phase(&self) -> SequentialPhase<Self::PlayerId>;
}

/// Base trait for actions submitted to an environment.
pub trait EnvironmentAction:
    Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + Debug + 'static
{
    /// Short type identifier for this action (e.g., `"guess"`, `"raise"`).
    fn action_type(&self) -> &str;
}
