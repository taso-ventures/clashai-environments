//! Wordle environment adapter implementing [`Environment`].
//!
//! Wraps [`wordle_protocol::WordleGame`] and exposes it through the
//! uniform [`Environment`] interface.

use std::collections::HashMap;

use wordle_protocol::{PlayerId, WordleAction, WordleConfig, WordleGame, WORDLE_RULES};

use crate::{Environment, EnvironmentError, PlayerRanking, Result, TurnInfo};

/// Environment adapter for the Wordle environment.
pub struct WordleEnvironment {
    game: WordleGame,
}

impl WordleEnvironment {
    /// Create a new Wordle environment.
    pub fn new(
        _match_id: impl Into<String>,
        player_ids: Vec<PlayerId>,
        player_names: HashMap<PlayerId, String>,
        config: WordleConfig,
        seed: u64,
    ) -> Result<Self> {
        let player_count = player_ids.len();
        if !(3..=6).contains(&player_count) {
            return Err(EnvironmentError::InvalidSetup(format!(
                "wordle supports 3-6 players, got {player_count}"
            )));
        }

        let game = WordleGame::new(player_ids, player_names, config, seed)
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
        if state.is_terminal {
            return vec![];
        }

        state
            .players
            .iter()
            .filter_map(|p| {
                let actions = self.game.legal_actions(p.player_id);
                if actions.is_empty() {
                    None
                } else {
                    Some(p.player_id.to_string())
                }
            })
            .collect()
    }
}

impl Environment for WordleEnvironment {
    fn environment_type(&self) -> &str {
        "wordle"
    }

    fn display_name(&self) -> &str {
        "Wordle"
    }

    fn min_players(&self) -> usize {
        3
    }

    fn max_players(&self) -> usize {
        6
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
        let phase_str = state.phase.as_str().to_string();
        Ok(TurnInfo {
            turn_number: state.turn,
            phase: phase_str.clone(),
            active_players: self.active_players(),
            is_terminal: state.is_terminal,
            decision_kind: Some("active".to_string()),
            state_revision: Some(format!("turn:{}:phase:{}", state.turn, phase_str)),
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
        let wordle_action: WordleAction = serde_json::from_value(action.clone()).map_err(|e| {
            EnvironmentError::InvalidAction(format!("failed to deserialize action {action}: {e}"))
        })?;

        self.game
            .apply_action(pid, &wordle_action)
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
        let mut rankings = Vec::with_capacity(state.players.len());

        let player_map: HashMap<PlayerId, &wordle_protocol::PlayerProgress> =
            state.players.iter().map(|p| (p.player_id, p)).collect();

        let mut solved_by_turn: HashMap<u32, Vec<PlayerId>> = HashMap::new();
        for pid in &state.solve_order {
            if let Some(player) = player_map.get(pid) {
                if let Some(turn) = player.solved_turn {
                    solved_by_turn.entry(turn).or_default().push(*pid);
                }
            }
        }

        let mut ranks: Vec<(u32, Vec<PlayerId>)> = solved_by_turn.into_iter().collect();
        ranks.sort_by_key(|(turn, _)| *turn);

        let mut rank = 1u32;
        for (_turn, players) in ranks {
            for player_id in &players {
                rankings.push(PlayerRanking {
                    player_id: player_id.to_string(),
                    rank,
                });
            }
            rank += players.len() as u32;
        }

        let unsolved: Vec<PlayerId> = state
            .players
            .iter()
            .map(|p| p.player_id)
            .filter(|pid| !state.solve_order.contains(pid))
            .collect();

        if !unsolved.is_empty() {
            for player_id in unsolved {
                rankings.push(PlayerRanking {
                    player_id: player_id.to_string(),
                    rank,
                });
            }
        }

        Some(rankings)
    }

    fn rules_markdown(&self) -> &str {
        WORDLE_RULES
    }

    fn player_ids(&self) -> Vec<String> {
        let mut ids: Vec<PlayerId> = self
            .game
            .full_state()
            .players
            .iter()
            .map(|p| p.player_id)
            .collect();
        ids.sort_unstable();
        ids.iter().map(|pid| pid.to_string()).collect()
    }
}
