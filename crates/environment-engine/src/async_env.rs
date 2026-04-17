//! Async variant of [`Environment`] for remote/networked environments.
//!
//! [`AsyncEnvironment`] mirrors [`Environment`] but with `async` methods,
//! suitable for environments backed by network I/O (e.g., FreeCiv WebSocket).
//!
//! [`SyncAdapter`] bridges any [`Environment`] to [`AsyncEnvironment`],
//! so environments like Coup can be used with [`AsyncEnvironmentToolExecutor`].

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::{Environment, PlayerRanking, Result, TurnInfo};

/// Async variant of [`Environment`] for remote/networked environments.
///
/// All state/action methods are `async`, enabling implementations that
/// perform network I/O (WebSocket, HTTP, etc.). Methods that return
/// static metadata (`environment_type`, `display_name`, etc.) remain
/// synchronous.
///
/// Unlike [`Environment::apply_action`] which takes `&mut self`,
/// this trait uses `&self` for `apply_action` — remote environments
/// handle mutation server-side, so the adapter only needs shared access.
#[async_trait]
pub trait AsyncEnvironment: Send + Sync {
    /// Unique identifier for this environment type (e.g. `"freeciv"`).
    fn environment_type(&self) -> &str;

    /// Human-readable display name (e.g. `"FreeCiv"`).
    fn display_name(&self) -> &str;

    /// Minimum number of players supported.
    fn min_players(&self) -> usize;

    /// Maximum number of players supported.
    fn max_players(&self) -> usize;

    /// Return the current state filtered for the given player.
    async fn state_for_player(&self, player_id: &str) -> Result<serde_json::Value>;

    /// Return the full (unfiltered) state.
    async fn full_state(&self) -> Result<serde_json::Value>;

    /// Return information about the current turn.
    async fn turn_info(&self) -> Result<TurnInfo>;

    /// Return the legal actions available to a player as a JSON value.
    async fn legal_actions(&self, player_id: &str) -> Result<serde_json::Value>;

    /// Apply an action submitted by a player.
    ///
    /// Returns the outcome as a JSON value (environment-specific).
    async fn apply_action(
        &self,
        player_id: &str,
        action: &serde_json::Value,
    ) -> Result<serde_json::Value>;

    /// Whether the environment has reached a terminal state.
    async fn is_terminal(&self) -> bool;

    /// Return final rankings when the environment is terminal.
    async fn rankings(&self) -> Option<Vec<PlayerRanking>>;

    /// Return the rules/instructions for this environment as markdown text.
    fn rules_markdown(&self) -> &str;

    /// Return a list of player IDs in this environment.
    fn player_ids(&self) -> Vec<String>;
}

/// Adapter that wraps a sync [`Environment`] behind [`AsyncEnvironment`].
///
/// This enables sync environments (Coup, etc.) to be used with
/// [`AsyncEnvironmentToolExecutor`] without any code changes.
///
/// Internally uses `Arc<RwLock<Box<dyn Environment>>>` — same
/// locking pattern as the existing [`EnvironmentToolExecutor`].
pub struct SyncAdapter {
    inner: Arc<RwLock<Box<dyn Environment>>>,
    /// Cached rules markdown (loaded once at construction).
    rules: String,
    /// Cached player IDs (loaded once at construction).
    /// Assumption: player IDs are fixed for the environment lifetime (true for Coup, FreeCiv).
    /// If a future environment supports dynamic join/leave, replace with delegating call.
    player_ids: Vec<String>,
    /// Cached metadata (loaded once at construction).
    environment_type: String,
    display_name: String,
    min_players: usize,
    max_players: usize,
}

impl SyncAdapter {
    /// Create a new adapter wrapping a shared Environment.
    ///
    /// Caches static metadata to avoid repeated lock acquisitions.
    pub async fn new(inner: Arc<RwLock<Box<dyn Environment>>>) -> Self {
        let guard = inner.read().await;
        let rules = guard.rules_markdown().to_string();
        let player_ids = guard.player_ids();
        let environment_type = guard.environment_type().to_string();
        let display_name = guard.display_name().to_string();
        let min_players = guard.min_players();
        let max_players = guard.max_players();
        drop(guard);

        Self {
            inner,
            rules,
            player_ids,
            environment_type,
            display_name,
            min_players,
            max_players,
        }
    }
}

#[async_trait]
impl AsyncEnvironment for SyncAdapter {
    fn environment_type(&self) -> &str {
        &self.environment_type
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn min_players(&self) -> usize {
        self.min_players
    }

    fn max_players(&self) -> usize {
        self.max_players
    }

    async fn state_for_player(&self, player_id: &str) -> Result<serde_json::Value> {
        self.inner.read().await.state_for_player(player_id)
    }

    async fn full_state(&self) -> Result<serde_json::Value> {
        self.inner.read().await.full_state()
    }

    async fn turn_info(&self) -> Result<TurnInfo> {
        self.inner.read().await.turn_info()
    }

    async fn legal_actions(&self, player_id: &str) -> Result<serde_json::Value> {
        self.inner.read().await.legal_actions(player_id)
    }

    async fn apply_action(
        &self,
        player_id: &str,
        action: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.inner.write().await.apply_action(player_id, action)
    }

    async fn is_terminal(&self) -> bool {
        self.inner.read().await.is_terminal()
    }

    async fn rankings(&self) -> Option<Vec<PlayerRanking>> {
        self.inner.read().await.rankings()
    }

    fn rules_markdown(&self) -> &str {
        &self.rules
    }

    fn player_ids(&self) -> Vec<String> {
        self.player_ids.clone()
    }
}
