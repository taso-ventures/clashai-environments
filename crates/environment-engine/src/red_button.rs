//! Red Button environment adapter implementing [`Environment`].
//!
//! Wraps [`red_button_protocol::RedButtonGame`] and exposes it through the
//! uniform [`Environment`] interface so the registry and environment-server
//! service can treat all environments identically.

use std::collections::HashMap;

use red_button_protocol::{
    PlayerId, RedButtonAction, RedButtonGame, RedButtonRole, TurnActor, RULES_MARKDOWN,
};
use serde_json;

use crate::{Environment, EnvironmentError, PlayerRanking, Result, TurnInfo};

const RED_BUTTON_RULES: &str = RULES_MARKDOWN;

/// Environment adapter for the Red Button persuasion environment.
///
/// Wraps a [`RedButtonGame`] instance and translates its strongly-typed API
/// into the JSON-based [`Environment`] interface.
pub struct RedButtonEnvironment {
    game: RedButtonGame,
}

impl RedButtonEnvironment {
    /// Create a new Red Button environment.
    ///
    /// `player_ids` must contain exactly 2 distinct IDs.
    /// `player_names` maps each ID to a display name; missing entries get a
    /// default `"Player N"` label.
    pub fn new(
        match_id: impl Into<String>,
        player_ids: Vec<PlayerId>,
        player_names: HashMap<PlayerId, String>,
        config: red_button_protocol::RedButtonConfig,
    ) -> Result<Self> {
        let game = RedButtonGame::new(match_id, player_ids, player_names, config)
            .map_err(|e| EnvironmentError::InvalidSetup(e.to_string()))?;
        Ok(Self { game })
    }

    /// Parse a `player_id` string (e.g. `"0"`, `"1"`) into [`PlayerId`].
    fn parse_player_id(player_id: &str) -> Result<PlayerId> {
        player_id
            .parse::<PlayerId>()
            .map_err(|_| EnvironmentError::UnknownPlayer(player_id.to_string()))
    }

    /// Build the active-player list from current turn state.
    fn active_player_string(&self) -> Vec<String> {
        let state = self.game.full_state();
        if state.is_terminal {
            return vec![];
        }
        // Find the player whose role matches the current actor.
        let target_role = match state.turn_info.actor {
            TurnActor::Persuader => RedButtonRole::Persuader,
            TurnActor::Resistor => RedButtonRole::Resistor,
        };
        self.game
            .player_roles()
            .iter()
            .filter(|(_, r)| **r == target_role)
            .map(|(pid, _)| pid.to_string())
            .collect()
    }
}

impl Environment for RedButtonEnvironment {
    fn environment_type(&self) -> &str {
        "red_button"
    }

    fn display_name(&self) -> &str {
        "Red Button"
    }

    fn min_players(&self) -> usize {
        2
    }

    fn max_players(&self) -> usize {
        2
    }

    fn state_for_player(&self, player_id: &str) -> Result<serde_json::Value> {
        let pid = Self::parse_player_id(player_id)?;
        let state = self
            .game
            .state_for_player(pid)
            .map_err(|e| EnvironmentError::Internal(e.to_string()))?;
        serde_json::to_value(&state)
            .map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn full_state(&self) -> Result<serde_json::Value> {
        let state = self.game.full_state();
        serde_json::to_value(&state)
            .map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn turn_info(&self) -> Result<TurnInfo> {
        let state = self.game.full_state();
        let phase = match state.turn_info.actor {
            TurnActor::Persuader => "persuader_turn",
            TurnActor::Resistor => "resistor_turn",
        };
        Ok(TurnInfo {
            turn_number: state.turn_info.round,
            phase: phase.to_string(),
            active_players: self.active_player_string(),
            is_terminal: state.is_terminal,
            decision_kind: Some("active".to_string()),
            state_revision: Some(format!("turn:{}:phase:{}", state.turn_info.round, phase)),
            step_deadline_ms: None,
        })
    }

    fn legal_actions(&self, player_id: &str) -> Result<serde_json::Value> {
        if self.game.is_terminal() {
            return Ok(serde_json::Value::Array(vec![]));
        }
        let pid = Self::parse_player_id(player_id)?;
        let actions = self.game.legal_actions(pid);
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
        let rb_action: RedButtonAction = serde_json::from_value(action.clone()).map_err(|e| {
            EnvironmentError::InvalidAction(format!(
                "failed to deserialize RedButtonAction from {action}: {e}"
            ))
        })?;
        let events = self
            .game
            .apply_action(pid, &rb_action)
            .map_err(|e| EnvironmentError::InvalidAction(e.to_string()))?;
        serde_json::to_value(&events)
            .map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn is_terminal(&self) -> bool {
        self.game.is_terminal()
    }

    fn rankings(&self) -> Option<Vec<PlayerRanking>> {
        if !self.game.is_terminal() {
            return None;
        }
        let winner_id = self.game.winner()?;
        let mut rankings = Vec::with_capacity(2);
        rankings.push(PlayerRanking {
            player_id: winner_id.to_string(),
            rank: 1,
        });
        for &pid in self.game.player_roles().keys() {
            if pid != winner_id {
                rankings.push(PlayerRanking {
                    player_id: pid.to_string(),
                    rank: 2,
                });
            }
        }
        Some(rankings)
    }

    fn rules_markdown(&self) -> &str {
        RED_BUTTON_RULES
    }

    fn player_ids(&self) -> Vec<String> {
        let mut ids: Vec<PlayerId> = self.game.player_roles().keys().copied().collect();
        ids.sort_unstable();
        ids.iter().map(|pid| pid.to_string()).collect()
    }
}
