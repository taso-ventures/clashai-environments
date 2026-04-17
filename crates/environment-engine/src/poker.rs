//! Poker environment adapter implementing [`Environment`].
//!
//! Wraps [`poker_protocol::engine::PokerMatch`] and exposes it through the
//! uniform [`Environment`] interface.

use poker_protocol::engine::PokerMatch;
use poker_protocol::{MatchPhase, PlayerId, PokerAction};
use serde_json;

use crate::{Environment, EnvironmentError, PlayerRanking, Result, TurnInfo};

const POKER_RULES: &str = include_str!("../resources/poker_rules.md");

/// Environment adapter for Heads-Up No-Limit Texas Hold'em.
pub struct PokerEnvironment {
    game: PokerMatch,
}

impl PokerEnvironment {
    /// Create a new Poker environment with the given seed.
    pub fn new(seed: u64) -> Result<Self> {
        Ok(Self {
            game: PokerMatch::new(seed).map_err(|e| EnvironmentError::Internal(e.to_string()))?,
        })
    }

    fn parse_player_id(player_id: &str) -> Result<PlayerId> {
        player_id
            .parse::<PlayerId>()
            .map_err(|_| EnvironmentError::UnknownPlayer(player_id.to_string()))
    }
}

impl Environment for PokerEnvironment {
    fn environment_type(&self) -> &str {
        "poker"
    }

    fn display_name(&self) -> &str {
        "Poker"
    }

    fn min_players(&self) -> usize {
        2
    }

    fn max_players(&self) -> usize {
        2
    }

    fn state_for_player(&self, player_id: &str) -> Result<serde_json::Value> {
        let pid = Self::parse_player_id(player_id)?;
        let view = self.game.state_for_player(pid);
        serde_json::to_value(&view).map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn full_state(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self.game.state())
            .map_err(|e| EnvironmentError::SerializationError(e.to_string()))
    }

    fn turn_info(&self) -> Result<TurnInfo> {
        let state = self.game.state();
        let active_players = if let Some(hand) = &state.current_hand {
            if !hand.finished {
                vec![hand.action_on.to_string()]
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        Ok(TurnInfo {
            turn_number: state.hand_number,
            phase: state
                .current_hand
                .as_ref()
                .map(|h| h.round.as_str().to_string())
                .unwrap_or_else(|| match state.phase {
                    MatchPhase::Completed => "completed".to_string(),
                    _ => "waiting".to_string(),
                }),
            active_players,
            is_terminal: self.game.is_terminal(),
            decision_kind: None,
            state_revision: None,
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
        let poker_action: PokerAction = serde_json::from_value(action.clone()).map_err(|e| {
            EnvironmentError::InvalidAction(format!("failed to deserialize action {action}: {e}"))
        })?;

        self.game
            .apply_action(pid, &poker_action)
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

        let state = self.game.state();
        let mut rankings = Vec::with_capacity(2);

        if state.profits[0] > state.profits[1] {
            rankings.push(PlayerRanking {
                player_id: "0".to_string(),
                rank: 1,
            });
            rankings.push(PlayerRanking {
                player_id: "1".to_string(),
                rank: 2,
            });
        } else if state.profits[1] > state.profits[0] {
            rankings.push(PlayerRanking {
                player_id: "1".to_string(),
                rank: 1,
            });
            rankings.push(PlayerRanking {
                player_id: "0".to_string(),
                rank: 2,
            });
        } else {
            // Draw: both rank 1
            rankings.push(PlayerRanking {
                player_id: "0".to_string(),
                rank: 1,
            });
            rankings.push(PlayerRanking {
                player_id: "1".to_string(),
                rank: 1,
            });
        }

        Some(rankings)
    }

    fn rules_markdown(&self) -> &str {
        POKER_RULES
    }

    fn player_ids(&self) -> Vec<String> {
        vec!["0".to_string(), "1".to_string()]
    }
}
