//! Coup environment adapter implementing [`Environment`].
//!
//! Wraps [`coup_engine::CoupGame`] and tracks elimination order to derive
//! final rankings when the environment ends.

use coup_engine::{CoupGame, CoupGameConfig};
use coup_protocol::{CoupAction, CoupState, PlayerId, TurnPhase};
use serde_json;
use tracing::debug;

use crate::{Environment, EnvironmentError, PlayerRanking, Result, TurnInfo};

const COUP_RULES: &str = include_str!("../resources/coup_rules.md");

/// Environment adapter for the Coup card game.
///
/// Wraps a [`CoupGame`] instance and tracks the order in which players
/// are eliminated so that final rankings can be derived (the spec's
/// `rankings()` requirement). The winner gets rank 1, the last
/// eliminated gets rank 2, and so on.
pub struct CoupEnvironment {
    game: CoupGame,
    player_count: usize,
    /// Players in the order they were eliminated (first eliminated = first entry).
    elimination_order: Vec<PlayerId>,
}

impl CoupEnvironment {
    /// Create a new Coup environment with the given player count and RNG seed.
    pub fn new(player_count: usize, seed: u64) -> Result<Self> {
        let game = CoupGame::new(player_count, seed)
            .map_err(|e| EnvironmentError::InvalidSetup(e.to_string()))?;
        Ok(Self {
            game,
            player_count,
            elimination_order: Vec::new(),
        })
    }

    /// Create a new Coup environment with custom configuration.
    pub fn with_config(player_count: usize, seed: u64, config: CoupGameConfig) -> Result<Self> {
        let game = CoupGame::new_with_config(player_count, seed, config)
            .map_err(|e| EnvironmentError::InvalidSetup(e.to_string()))?;
        Ok(Self {
            game,
            player_count,
            elimination_order: Vec::new(),
        })
    }

    /// Parse a player_id string (e.g. `"0"`, `"1"`) into a [`PlayerId`].
    fn parse_player_id(player_id: &str) -> Result<PlayerId> {
        player_id
            .parse::<PlayerId>()
            .map_err(|_| EnvironmentError::UnknownPlayer(player_id.to_string()))
    }

    /// Snapshot which players are currently eliminated, to detect new
    /// eliminations after applying an action.
    fn eliminated_set(&self) -> Vec<PlayerId> {
        self.game
            .state()
            .players
            .iter()
            .filter(|(_, ps)| ps.eliminated)
            .map(|(pid, _)| *pid)
            .collect()
    }

    /// Record any newly eliminated players into `elimination_order`.
    fn record_new_eliminations(&mut self, before: &[PlayerId]) {
        let state = self.game.state();
        for (pid, ps) in &state.players {
            if ps.eliminated && !before.contains(pid) && !self.elimination_order.contains(pid) {
                debug!(player_id = pid, "Player eliminated, recording in order");
                self.elimination_order.push(*pid);
            }
        }
    }

    /// Build active-players list from current phase.
    fn active_players_from_state(state: &CoupState) -> Vec<String> {
        match &state.current_phase {
            TurnPhase::AwaitingAction => vec![state.active_player.to_string()],
            TurnPhase::ChallengeWindow { waiting_on, .. }
            | TurnPhase::BlockWindow { waiting_on, .. }
            | TurnPhase::BlockChallengeWindow { waiting_on, .. } => {
                waiting_on.iter().map(|pid| pid.to_string()).collect()
            }
            TurnPhase::RevealingCard { player, .. }
            | TurnPhase::SelectingCardToLose { player }
            | TurnPhase::ExchangeSelection { player } => vec![player.to_string()],
            TurnPhase::ActionResolving | TurnPhase::GameOver { .. } => vec![],
        }
    }
}

impl Environment for CoupEnvironment {
    fn environment_type(&self) -> &str {
        "coup"
    }

    fn display_name(&self) -> &str {
        "Coup"
    }

    fn min_players(&self) -> usize {
        2
    }

    fn max_players(&self) -> usize {
        6
    }

    fn state_for_player(&self, player_id: &str) -> Result<serde_json::Value> {
        let pid = Self::parse_player_id(player_id)?;
        if !self.game.state().players.contains_key(&pid) {
            return Err(EnvironmentError::UnknownPlayer(player_id.to_string()));
        }
        let filtered = self.game.state().filtered_for_player(pid);
        serde_json::to_value(&filtered)
            .map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn full_state(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self.game.state())
            .map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn turn_info(&self) -> Result<TurnInfo> {
        let state = self.game.state();
        let phase_str = state.current_phase.as_str();
        let decision_kind = match &state.current_phase {
            coup_protocol::TurnPhase::AwaitingAction => "active",
            coup_protocol::TurnPhase::ChallengeWindow { .. }
            | coup_protocol::TurnPhase::BlockWindow { .. }
            | coup_protocol::TurnPhase::BlockChallengeWindow { .. } => "reactive",
            coup_protocol::TurnPhase::RevealingCard { .. }
            | coup_protocol::TurnPhase::SelectingCardToLose { .. }
            | coup_protocol::TurnPhase::ExchangeSelection { .. } => "forced",
            coup_protocol::TurnPhase::ActionResolving
            | coup_protocol::TurnPhase::GameOver { .. } => "active",
        };
        Ok(TurnInfo {
            turn_number: state.turn_number,
            phase: phase_str.to_string(),
            active_players: Self::active_players_from_state(state),
            is_terminal: self.game.is_terminal(),
            decision_kind: Some(decision_kind.to_string()),
            state_revision: Some(format!("turn:{}:phase:{}", state.turn_number, phase_str)),
            step_deadline_ms: None,
        })
    }

    fn legal_actions(&self, player_id: &str) -> Result<serde_json::Value> {
        if self.game.is_terminal() {
            return Ok(serde_json::Value::Array(vec![]));
        }
        let pid = Self::parse_player_id(player_id)?;
        let actions = self.game.legal_actions(&pid);
        serde_json::to_value(&actions)
            .map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn apply_action(
        &mut self,
        player_id: &str,
        action: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        if self.game.is_terminal() {
            return Err(EnvironmentError::AlreadyTerminated);
        }

        let pid = Self::parse_player_id(player_id)?;
        let coup_action: CoupAction = serde_json::from_value(action.clone()).map_err(|e| {
            EnvironmentError::InvalidAction(format!("failed to deserialize action {action}: {e}"))
        })?;

        let eliminated_before = self.eliminated_set();

        let outcome = self
            .game
            .apply_action(&pid, &coup_action)
            .map_err(|e| EnvironmentError::InvalidAction(e.to_string()))?;

        self.record_new_eliminations(&eliminated_before);

        serde_json::to_value(&outcome.events)
            .map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn is_terminal(&self) -> bool {
        self.game.is_terminal()
    }

    fn rankings(&self) -> Option<Vec<PlayerRanking>> {
        let winner = self.game.winner()?;

        // Build rankings: winner = rank 1, then reverse elimination order.
        // Last eliminated = rank 2, second-to-last = rank 3, etc.
        //
        // NOTE: This assumes players are eliminated sequentially (one per action).
        // In standard Coup rules this holds — each action resolves one elimination
        // at most. If a future variant allows simultaneous eliminations, players
        // eliminated in the same action will share an arbitrary ordering determined
        // by HashMap iteration in `record_new_eliminations`.
        let mut rankings = Vec::with_capacity(self.player_count);

        rankings.push(PlayerRanking {
            player_id: winner.to_string(),
            rank: 1,
        });

        for (i, pid) in self.elimination_order.iter().rev().enumerate() {
            rankings.push(PlayerRanking {
                player_id: pid.to_string(),
                rank: (i + 2) as u32,
            });
        }

        Some(rankings)
    }

    fn rules_markdown(&self) -> &str {
        COUP_RULES
    }

    fn player_ids(&self) -> Vec<String> {
        let mut ids: Vec<PlayerId> = self.game.state().players.keys().copied().collect();
        ids.sort();
        ids.iter().map(|pid| pid.to_string()).collect()
    }
}
