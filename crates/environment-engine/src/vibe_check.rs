//! Vibe Check environment adapter implementing [`Environment`].
//!
//! Wraps [`vibe_check_engine::VibeCheckGame`] and derives final rankings
//! from team scores when the environment ends.

use vibe_check_engine::VibeCheckGame;
use vibe_check_protocol::{PlayerId, TurnPhase, VibeCheckAction, VibeCheckState};

use crate::{Environment, EnvironmentError, PlayerRanking, Result, TurnInfo};

const VIBE_CHECK_RULES: &str =
    include_str!("../../vibe-check-engine/resources/vibe_check_rules.md");

/// Environment adapter for the Vibe Check spectrum-guessing game.
///
/// Wraps a [`VibeCheckGame`] instance. Rankings are team-based:
/// winning team members get rank 1, losing team gets rank 2.
/// On a draw, all players get rank 1.
pub struct VibeCheckEnvironment {
    game: VibeCheckGame,
    player_count: usize,
}

impl VibeCheckEnvironment {
    /// Create a new Vibe Check environment with the given player count and RNG seed.
    pub fn new(player_count: usize, seed: u64) -> Result<Self> {
        let game = VibeCheckGame::new(player_count, seed)
            .map_err(|e| EnvironmentError::InvalidSetup(e.to_string()))?;
        Ok(Self { game, player_count })
    }

    /// Parse a player_id string (e.g. `"0"`, `"1"`) into a [`PlayerId`].
    fn parse_player_id(player_id: &str) -> Result<PlayerId> {
        player_id
            .parse::<PlayerId>()
            .map_err(|_| EnvironmentError::UnknownPlayer(player_id.to_string()))
    }

    /// Build active-players list from current phase.
    fn active_players_from_state(state: &VibeCheckState) -> Vec<String> {
        match &state.phase {
            TurnPhase::CluePhase { cluegiver, .. } => vec![cluegiver.to_string()],
            TurnPhase::GuessPhase {
                active_team,
                cluegiver,
                pending_guesses,
                ..
            } => {
                // Guessers on active team who haven't submitted yet, excluding cluegiver
                state
                    .teams
                    .iter()
                    .find(|t| t.team_id == *active_team)
                    .map(|t| {
                        t.player_ids
                            .iter()
                            .filter(|pid| **pid != *cluegiver && !pending_guesses.contains_key(pid))
                            .map(|pid| pid.to_string())
                            .collect()
                    })
                    .unwrap_or_default()
            }
            TurnPhase::StealPhase {
                stealing_team,
                pending_steals,
                ..
            } => state
                .teams
                .iter()
                .find(|t| t.team_id == *stealing_team)
                .map(|t| {
                    t.player_ids
                        .iter()
                        .filter(|pid| !pending_steals.contains_key(pid))
                        .map(|pid| pid.to_string())
                        .collect()
                })
                .unwrap_or_default(),
            TurnPhase::Resolving { .. } | TurnPhase::GameOver { .. } => vec![],
        }
    }
}

impl Environment for VibeCheckEnvironment {
    fn environment_type(&self) -> &str {
        "vibe_check"
    }

    fn display_name(&self) -> &str {
        "Vibe Check"
    }

    fn min_players(&self) -> usize {
        4
    }

    fn max_players(&self) -> usize {
        6
    }

    fn state_for_player(&self, player_id: &str) -> Result<serde_json::Value> {
        let pid = Self::parse_player_id(player_id)?;
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
        let phase_str = state.phase.as_str().to_string();
        let decision_kind = if phase_str.contains("steal") {
            "reactive"
        } else {
            "active"
        };
        Ok(TurnInfo {
            turn_number: state.round,
            phase: phase_str.clone(),
            active_players: Self::active_players_from_state(state),
            is_terminal: self.game.is_terminal(),
            decision_kind: Some(decision_kind.to_string()),
            state_revision: Some(format!("turn:{}:phase:{}", state.round, phase_str)),
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
        let vc_action: VibeCheckAction = serde_json::from_value(action.clone()).map_err(|e| {
            EnvironmentError::InvalidAction(format!("failed to deserialize action {action}: {e}"))
        })?;

        let outcome = self
            .game
            .apply_action(&pid, &vc_action)
            .map_err(|e| EnvironmentError::InvalidAction(e.to_string()))?;

        serde_json::to_value(&outcome.events)
            .map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn is_terminal(&self) -> bool {
        self.game.is_terminal()
    }

    fn rankings(&self) -> Option<Vec<PlayerRanking>> {
        if !self.game.is_terminal() {
            return None;
        }

        let winner = self.game.winner();
        let state = self.game.state();

        let mut rankings = Vec::with_capacity(self.player_count);

        match winner {
            Some(winning_team_id) => {
                let losing_team_id = 1 - winning_team_id;

                // Winning team = rank 1
                if let Some(team) = state.teams.iter().find(|t| t.team_id == winning_team_id) {
                    for pid in &team.player_ids {
                        rankings.push(PlayerRanking {
                            player_id: pid.to_string(),
                            rank: 1,
                        });
                    }
                }

                // Losing team = rank 2
                if let Some(team) = state.teams.iter().find(|t| t.team_id == losing_team_id) {
                    for pid in &team.player_ids {
                        rankings.push(PlayerRanking {
                            player_id: pid.to_string(),
                            rank: 2,
                        });
                    }
                }
            }
            None => {
                // Draw — all players rank 1
                for player in &state.players {
                    rankings.push(PlayerRanking {
                        player_id: player.player_id.to_string(),
                        rank: 1,
                    });
                }
            }
        }

        Some(rankings)
    }

    fn rules_markdown(&self) -> &str {
        VIBE_CHECK_RULES
    }

    fn player_ids(&self) -> Vec<String> {
        (0..self.player_count as i32)
            .map(|pid| pid.to_string())
            .collect()
    }
}
