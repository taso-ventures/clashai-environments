//! Tic-Tac-Toe environment adapter implementing [`Environment`].
//!
//! Wraps [`tic_tac_toe_protocol::TicTacToeGame`] and exposes it through the
//! uniform [`Environment`] interface.

use std::collections::HashMap;

use tic_tac_toe_protocol::{PlayerId, TicTacToeAction, TicTacToeGame, TIC_TAC_TOE_RULES};

use crate::{Environment, EnvironmentError, PlayerRanking, Result, TurnInfo};

/// Environment adapter for the Tic-Tac-Toe environment.
pub struct TicTacToeEnvironment {
    game: TicTacToeGame,
}

impl TicTacToeEnvironment {
    /// Create a new Tic-Tac-Toe environment.
    pub fn new(
        _match_id: impl Into<String>,
        player_ids: Vec<PlayerId>,
        player_names: HashMap<PlayerId, String>,
    ) -> Result<Self> {
        let player_count = player_ids.len();
        if player_count != 2 {
            return Err(EnvironmentError::InvalidSetup(format!(
                "tic-tac-toe requires exactly 2 players, got {player_count}"
            )));
        }

        let game = TicTacToeGame::new(player_ids, player_names)
            .map_err(|e| EnvironmentError::InvalidSetup(e.to_string()))?;
        Ok(Self { game })
    }

    fn parse_player_id(player_id: &str) -> Result<PlayerId> {
        player_id
            .parse::<PlayerId>()
            .map_err(|_| EnvironmentError::UnknownPlayer(player_id.to_string()))
    }

    fn active_players(&self) -> Vec<String> {
        let state = self.game.full_state();
        match state.current_player {
            Some(pid) => vec![pid.to_string()],
            None => vec![],
        }
    }
}

impl Environment for TicTacToeEnvironment {
    fn environment_type(&self) -> &str {
        "tic_tac_toe"
    }

    fn display_name(&self) -> &str {
        "Tic-Tac-Toe"
    }

    fn min_players(&self) -> usize {
        2
    }

    fn max_players(&self) -> usize {
        2
    }

    fn state_for_player(&self, player_id: &str) -> Result<serde_json::Value> {
        let pid = Self::parse_player_id(player_id)?;
        let state = self.game.full_state();
        // Verify player exists
        if !state.players.iter().any(|p| p.player_id == pid) {
            return Err(EnvironmentError::UnknownPlayer(player_id.to_string()));
        }
        // Perfect-information game: player view = full state
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
        Ok(TurnInfo {
            turn_number: state.turn,
            phase: state.phase.as_str().to_string(),
            active_players: self.active_players(),
            is_terminal: state.phase == tic_tac_toe_protocol::TicTacToePhase::GameOver,
            decision_kind: Some("active".to_string()),
            state_revision: Some(format!("turn:{}", state.turn)),
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
        let ttt_action: TicTacToeAction = serde_json::from_value(action.clone()).map_err(|e| {
            EnvironmentError::InvalidAction(format!("failed to deserialize action {action}: {e}"))
        })?;

        self.game
            .apply_action(pid, &ttt_action)
            .map_err(|e| EnvironmentError::InvalidAction(e.to_string()))?;

        Ok(serde_json::Value::Object(Default::default()))
    }

    fn is_terminal(&self) -> bool {
        self.game.is_terminal()
    }

    fn rankings(&self) -> Option<Vec<PlayerRanking>> {
        if !self.game.is_terminal() {
            return None;
        }

        let state = self.game.full_state();
        match state.winner {
            Some(winner_id) => {
                let loser_id = state
                    .players
                    .iter()
                    .find(|p| p.player_id != winner_id)
                    .map(|p| p.player_id);

                let mut rankings = vec![PlayerRanking {
                    player_id: winner_id.to_string(),
                    rank: 1,
                }];
                if let Some(lid) = loser_id {
                    rankings.push(PlayerRanking {
                        player_id: lid.to_string(),
                        rank: 2,
                    });
                }
                Some(rankings)
            }
            None => {
                // Draw: both players rank 1
                Some(
                    state
                        .players
                        .iter()
                        .map(|p| PlayerRanking {
                            player_id: p.player_id.to_string(),
                            rank: 1,
                        })
                        .collect(),
                )
            }
        }
    }

    fn rules_markdown(&self) -> &str {
        TIC_TAC_TOE_RULES
    }

    fn player_ids(&self) -> Vec<String> {
        let state = self.game.full_state();
        state
            .players
            .iter()
            .map(|p| p.player_id.to_string())
            .collect()
    }
}
