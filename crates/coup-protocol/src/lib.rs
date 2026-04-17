//! Coup protocol types and shared API models.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use eval_runtime::{
    EnvironmentAction, EnvironmentState, EnvironmentWinner, SequentialDecisionKind,
    SequentialPhase, SequentialState,
};

pub type PlayerId = i32;
pub type ActionId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Duke,
    Assassin,
    Captain,
    Ambassador,
    Contessa,
    /// Sentinel value used in player-filtered state views.
    ///
    /// When [`CoupState::filtered_for_player`] redacts the state for a given
    /// player, all unrevealed cards belonging to *other* players have their
    /// role replaced with `Unknown`. This prevents agents from seeing
    /// opponents' hidden cards while preserving the card count and structure.
    /// `Unknown` never appears in the authoritative game state or the deck.
    Unknown,
}

impl Role {
    pub fn all_roles() -> &'static [Role] {
        &[
            Role::Duke,
            Role::Assassin,
            Role::Captain,
            Role::Ambassador,
            Role::Contessa,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub role: Role,
    pub revealed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    pub coins: i32,
    pub cards: Vec<Card>,
    pub eliminated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action_type")]
pub enum CoupAction {
    // Active turn actions
    Income,
    ForeignAid,
    Coup {
        target: PlayerId,
    },
    Tax,
    Assassinate {
        target: PlayerId,
    },
    Steal {
        target: PlayerId,
    },
    Exchange,

    // Reactive actions
    Challenge {
        action_id: ActionId,
    },
    Block {
        action_id: ActionId,
        claimed_role: Role,
    },
    Pass,

    // Resolution actions
    RevealCard {
        card_index: usize,
    },
    SelectCardToLose {
        card_index: usize,
    },
    ExchangeSelection {
        keep_indices: Vec<usize>,
    },

    // Orchestrator-only action
    Forfeit,
}

impl CoupAction {
    pub fn is_reactive(&self) -> bool {
        matches!(
            self,
            CoupAction::Challenge { .. } | CoupAction::Block { .. } | CoupAction::Pass
        )
    }

    pub fn is_resolution(&self) -> bool {
        matches!(
            self,
            CoupAction::RevealCard { .. }
                | CoupAction::SelectCardToLose { .. }
                | CoupAction::ExchangeSelection { .. }
        )
    }
}

impl EnvironmentAction for CoupAction {
    fn action_type(&self) -> &str {
        match self {
            CoupAction::Income => "income",
            CoupAction::ForeignAid => "foreign_aid",
            CoupAction::Coup { .. } => "coup",
            CoupAction::Tax => "tax",
            CoupAction::Assassinate { .. } => "assassinate",
            CoupAction::Steal { .. } => "steal",
            CoupAction::Exchange => "exchange",
            CoupAction::Challenge { .. } => "challenge",
            CoupAction::Block { .. } => "block",
            CoupAction::Pass => "pass",
            CoupAction::RevealCard { .. } => "reveal_card",
            CoupAction::SelectCardToLose { .. } => "select_card_to_lose",
            CoupAction::ExchangeSelection { .. } => "exchange_selection",
            CoupAction::Forfeit => "forfeit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingAction {
    pub id: ActionId,
    pub actor: PlayerId,
    pub action: CoupAction,
    pub target: Option<PlayerId>,
    pub claimed_role: Option<Role>,
    pub challenged_by: Option<PlayerId>,
    pub blocked_by: Option<PlayerId>,
    pub block_claimed_role: Option<Role>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exchange_draw: Vec<Role>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionHistoryEntry {
    pub turn: u32,
    pub actor: PlayerId,
    pub action: CoupAction,
    pub outcome: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnPhase {
    AwaitingAction,
    ChallengeWindow {
        waiting_on: Vec<PlayerId>,
        deadline: DateTime<Utc>,
    },
    BlockWindow {
        waiting_on: Vec<PlayerId>,
        deadline: DateTime<Utc>,
    },
    BlockChallengeWindow {
        waiting_on: Vec<PlayerId>,
        deadline: DateTime<Utc>,
    },
    RevealingCard {
        player: PlayerId,
        required_role: Role,
    },
    SelectingCardToLose {
        player: PlayerId,
    },
    ExchangeSelection {
        player: PlayerId,
    },
    ActionResolving,
    GameOver {
        winner: PlayerId,
    },
}

impl TurnPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            TurnPhase::AwaitingAction => "awaiting_action",
            TurnPhase::ChallengeWindow { .. } => "challenge_window",
            TurnPhase::BlockWindow { .. } => "block_window",
            TurnPhase::BlockChallengeWindow { .. } => "block_challenge_window",
            TurnPhase::RevealingCard { .. } => "revealing_card",
            TurnPhase::SelectingCardToLose { .. } => "selecting_card_to_lose",
            TurnPhase::ExchangeSelection { .. } => "exchange_selection",
            TurnPhase::ActionResolving => "action_resolving",
            TurnPhase::GameOver { .. } => "game_over",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoupState {
    pub turn_number: u32,
    pub current_phase: TurnPhase,
    pub active_player: PlayerId,
    pub players: HashMap<PlayerId, PlayerState>,
    pub pending_action: Option<PendingAction>,
    pub action_history: Vec<ActionHistoryEntry>,
    pub deck_count: usize,
}

impl CoupState {
    pub fn filtered_for_player(&self, player_id: PlayerId) -> Self {
        let mut filtered = self.clone();
        for (pid, state) in filtered.players.iter_mut() {
            if *pid != player_id {
                for card in &mut state.cards {
                    if !card.revealed {
                        card.role = Role::Unknown;
                    }
                }
            }
        }
        // Redact exchange_draw for non-actors (only the exchanging player should see drawn cards)
        if let Some(ref mut pending) = filtered.pending_action {
            if pending.actor != player_id {
                pending.exchange_draw.clear();
            }
        }
        filtered
    }
}

impl EnvironmentState for CoupState {
    type PlayerId = PlayerId;

    fn turn_number(&self) -> u32 {
        self.turn_number
    }

    fn current_phase(&self) -> &str {
        self.current_phase.as_str()
    }

    fn player_ids(&self) -> Vec<Self::PlayerId> {
        self.players.keys().copied().collect()
    }

    fn is_terminal(&self) -> bool {
        matches!(self.current_phase, TurnPhase::GameOver { .. })
    }
}

impl SequentialState for CoupState {
    fn sequential_phase(&self) -> SequentialPhase<Self::PlayerId> {
        match &self.current_phase {
            TurnPhase::AwaitingAction => SequentialPhase::Decision {
                kind: SequentialDecisionKind::Active,
                players: vec![self.active_player],
                deadline: None,
            },
            TurnPhase::ChallengeWindow {
                waiting_on,
                deadline,
            } => SequentialPhase::Decision {
                kind: SequentialDecisionKind::Reactive,
                players: waiting_on.clone(),
                deadline: Some(*deadline),
            },
            TurnPhase::BlockWindow {
                waiting_on,
                deadline,
            } => SequentialPhase::Decision {
                kind: SequentialDecisionKind::Reactive,
                players: waiting_on.clone(),
                deadline: Some(*deadline),
            },
            TurnPhase::BlockChallengeWindow {
                waiting_on,
                deadline,
            } => SequentialPhase::Decision {
                kind: SequentialDecisionKind::Reactive,
                players: waiting_on.clone(),
                deadline: Some(*deadline),
            },
            TurnPhase::RevealingCard { player, .. } => SequentialPhase::Decision {
                kind: SequentialDecisionKind::Forced,
                players: vec![*player],
                deadline: None,
            },
            TurnPhase::SelectingCardToLose { player } => SequentialPhase::Decision {
                kind: SequentialDecisionKind::Forced,
                players: vec![*player],
                deadline: None,
            },
            TurnPhase::ExchangeSelection { player } => SequentialPhase::Decision {
                kind: SequentialDecisionKind::Forced,
                players: vec![*player],
                deadline: None,
            },
            TurnPhase::ActionResolving => SequentialPhase::Resolving,
            TurnPhase::GameOver { winner } => SequentialPhase::GameOver {
                winner: EnvironmentWinner::Player(*winner),
            },
        }
    }
}

// =====================
// Spectator Events
// =====================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpectatorEvent {
    GameStarted {
        players: Vec<PlayerPublicInfo>,
    },
    TurnAdvanced {
        turn: u32,
        active_player: PlayerId,
    },
    AgentReasoning {
        player: PlayerId,
        reasoning: String,
    },
    ActionDeclared {
        player: PlayerId,
        action: CoupAction,
    },
    ChallengeIssued {
        challenger: PlayerId,
        against: PlayerId,
    },
    BlockDeclared {
        blocker: PlayerId,
        role: Role,
    },
    CardRevealed {
        player: PlayerId,
        role: Role,
    },
    InfluenceLost {
        player: PlayerId,
        role: Role,
    },
    PlayerEliminated {
        player: PlayerId,
    },
    GameOver {
        winner: PlayerId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerPublicInfo {
    pub player_id: PlayerId,
    pub eliminated: bool,
}

// =====================
// Service API Models
// =====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMatchRequest {
    pub match_id: Option<String>,
    pub player_count: usize,
    pub seed: Option<u64>,
    pub reaction_timeout_secs: Option<u64>,
    /// Optional mapping of player_id -> display name (agent name)
    #[serde(default)]
    pub player_names: Option<HashMap<PlayerId, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMatchResponse {
    pub match_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spectator_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStatusResponse {
    pub match_id: String,
    pub turn_number: u32,
    pub phase: String,
    pub is_terminal: bool,
    pub winner: Option<PlayerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitActionRequest {
    pub player_id: PlayerId,
    pub action: CoupAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitActionResponse {
    pub accepted: bool,
    pub error: Option<String>,
}
