//! Poker protocol types and game engine for Heads-Up No-Limit Texas Hold'em.

pub mod card;
pub mod engine;

use eval_runtime::{
    EnvironmentAction, EnvironmentState, EnvironmentWinner, SequentialDecisionKind,
    SequentialPhase, SequentialState,
};
use serde::{Deserialize, Serialize};

use crate::card::{Card, HandScore};

// =====================
// Constants
// =====================

/// Starting stack for each player per hand.
pub const INITIAL_STACK: i32 = 200;
/// Small blind.
pub const SMALL_BLIND: i32 = 1;
/// Big blind.
pub const BIG_BLIND: i32 = 2;
/// Maximum number of hands per match.
pub const MAX_HANDS: u32 = 100;
/// Number of players (always 2 for HU).
pub const NUM_PLAYERS: usize = 2;

pub type PlayerId = i32;

// =====================
// Actions
// =====================

/// High-level poker actions submitted by agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action_type")]
pub enum PokerAction {
    Fold,
    Check,
    Call,
    /// Raise to a total street bet of the given amount.
    Raise {
        amount: i32,
    },
}

impl PokerAction {
    pub fn raise(amount: i32) -> Self {
        PokerAction::Raise { amount }
    }
}

impl EnvironmentAction for PokerAction {
    fn action_type(&self) -> &str {
        match self {
            PokerAction::Fold => "fold",
            PokerAction::Check => "check",
            PokerAction::Call => "call",
            PokerAction::Raise { .. } => "raise",
        }
    }
}

/// Internal engine action representation (also used in action history).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerAction {
    Fold,
    Check,
    Call(i32),
    Raise(i32),
}

// =====================
// Betting round
// =====================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BettingRound {
    Preflop,
    Flop,
    Turn,
    River,
}

impl BettingRound {
    pub fn as_str(&self) -> &'static str {
        match self {
            BettingRound::Preflop => "preflop",
            BettingRound::Flop => "flop",
            BettingRound::Turn => "turn",
            BettingRound::River => "river",
        }
    }
}

// =====================
// Legal actions
// =====================

/// Legal actions available to the active player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalActions {
    pub can_fold: bool,
    pub can_check: bool,
    pub can_call: bool,
    /// Amount needed to call (0 if can't call).
    pub call_amount: i32,
    pub can_raise: bool,
    /// Minimum total street bet for a raise.
    pub min_raise: i32,
    /// Maximum total street bet for a raise (all-in).
    pub max_raise: i32,
}

impl LegalActions {
    /// No actions available.
    pub fn none() -> Self {
        Self {
            can_fold: false,
            can_check: false,
            can_call: false,
            call_amount: 0,
            can_raise: false,
            min_raise: 0,
            max_raise: 0,
        }
    }

    /// Convert to a list of available PokerAction variants for display.
    pub fn to_action_list(&self) -> Vec<PokerAction> {
        let mut actions = Vec::new();
        if self.can_fold {
            actions.push(PokerAction::Fold);
        }
        if self.can_check {
            actions.push(PokerAction::Check);
        }
        if self.can_call {
            actions.push(PokerAction::Call);
        }
        if self.can_raise {
            actions.push(PokerAction::Raise {
                amount: self.min_raise,
            });
        }
        actions
    }
}

// =====================
// Match state
// =====================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchPhase {
    PreMatch,
    Playing,
    Completed,
}

/// Full match state (admin/spectator view).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchState {
    pub hand_number: u32,
    pub max_hands: u32,
    pub profits: [i32; 2],
    pub phase: MatchPhase,
    pub button: PlayerId,
    pub current_hand: Option<HandState>,
    pub hand_history: Vec<HandResult>,
}

/// Full state of a single hand (admin/spectator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandState {
    pub hole_cards: [[Card; 2]; 2],
    pub community: Vec<Card>,
    pub round: BettingRound,
    pub stacks: [i32; 2],
    pub pot: i32,
    pub street_bets: [i32; 2],
    pub pot_contributions: [i32; 2],
    pub button: PlayerId,
    pub action_on: PlayerId,
    pub folded: [bool; 2],
    pub finished: bool,
    pub action_history: Vec<(PlayerId, PlayerAction)>,
}

/// Player-filtered view of the match (hides opponent hole cards while a
/// hand is in progress; the most recent completed hand's showdown info is
/// exposed via `last_hand_result`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMatchView {
    pub player_id: PlayerId,
    pub hand_number: u32,
    pub max_hands: u32,
    pub your_profit: i32,
    pub opponent_profit: i32,
    pub your_stack: i32,
    pub opponent_stack: i32,
    pub phase: MatchPhase,
    pub button: PlayerId,
    pub current_hand: Option<PlayerHandView>,
    /// Most recent completed hand's result (opponent hole cards revealed
    /// on showdown, or `None` before the first hand finishes). Lets agents
    /// see what the opponent held after the hand ends.
    pub last_hand_result: Option<HandResult>,
}

/// Player-filtered view of a single hand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerHandView {
    pub your_cards: [Card; 2],
    pub community: Vec<Card>,
    pub round: BettingRound,
    pub your_stack: i32,
    pub opponent_stack: i32,
    pub pot: i32,
    pub your_street_bet: i32,
    pub opponent_street_bet: i32,
    pub button: PlayerId,
    pub action_on: PlayerId,
    pub folded: [bool; 2],
    pub finished: bool,
    pub action_history: Vec<(PlayerId, PlayerAction)>,
}

/// Result of a completed hand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandResult {
    pub hand_number: u32,
    pub winner: Option<PlayerId>,
    pub profits: [i32; 2],
    pub pot: i32,
    pub hole_cards: [[Card; 2]; 2],
    pub community: Vec<Card>,
    pub showdown: bool,
    pub winning_hand: Option<HandScore>,
}

// =====================
// Trait implementations
// =====================

impl EnvironmentState for MatchState {
    type PlayerId = PlayerId;

    fn turn_number(&self) -> u32 {
        self.hand_number
    }

    fn current_phase(&self) -> &str {
        match &self.phase {
            MatchPhase::PreMatch => "pre_match",
            MatchPhase::Playing => self
                .current_hand
                .as_ref()
                .map(|h| h.round.as_str())
                .unwrap_or("playing"),
            MatchPhase::Completed => "completed",
        }
    }

    fn player_ids(&self) -> Vec<Self::PlayerId> {
        vec![0, 1]
    }

    fn is_terminal(&self) -> bool {
        matches!(self.phase, MatchPhase::Completed)
    }
}

impl SequentialState for MatchState {
    fn sequential_phase(&self) -> SequentialPhase<Self::PlayerId> {
        match &self.phase {
            MatchPhase::PreMatch => SequentialPhase::Resolving,
            MatchPhase::Playing => {
                if let Some(hand) = &self.current_hand {
                    if hand.finished {
                        SequentialPhase::Resolving
                    } else {
                        SequentialPhase::Decision {
                            kind: SequentialDecisionKind::Active,
                            players: vec![hand.action_on],
                            deadline: None,
                        }
                    }
                } else {
                    SequentialPhase::Resolving
                }
            }
            MatchPhase::Completed => {
                let winner = if self.profits[0] > self.profits[1] {
                    EnvironmentWinner::Player(0)
                } else if self.profits[1] > self.profits[0] {
                    EnvironmentWinner::Player(1)
                } else {
                    EnvironmentWinner::Draw
                };
                SequentialPhase::GameOver { winner }
            }
        }
    }
}

// =====================
// Spectator events
// =====================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpectatorEvent {
    MatchStarted {
        player_ids: Vec<PlayerId>,
    },
    HandStarted {
        hand_number: u32,
        button: PlayerId,
    },
    CardsDealt {
        round: BettingRound,
        community: Vec<Card>,
    },
    PlayerActed {
        player: PlayerId,
        action: PlayerAction,
    },
    AgentReasoning {
        player: PlayerId,
        reasoning: String,
    },
    HandCompleted {
        result: HandResult,
    },
    MatchCompleted {
        profits: [i32; 2],
        winner: Option<PlayerId>,
    },
}
