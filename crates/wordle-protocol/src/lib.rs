pub mod engine;
pub mod seed;
pub mod word_list;

pub use engine::{EngineError, WordleGame};
pub use seed::{build_wordle_daily_seed_key, derive_wordle_daily_seed};

use eval_runtime::{
    EnvironmentAction, EnvironmentState, EnvironmentWinner, SequentialDecisionKind,
    SequentialPhase, SequentialState,
};
use serde::{Deserialize, Serialize};

pub type PlayerId = i32;

pub const WORDLE_RULES: &str = include_str!("../resources/wordle_rules.md");

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LetterFeedback {
    Correct,
    Present,
    Absent,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuessResult {
    pub word: String,
    pub feedback: Vec<LetterFeedback>,
    pub is_correct: bool,
    pub turn: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatPhase {
    Lobby,
    Win,
    Banter,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub player_id: PlayerId,
    pub player_name: String,
    pub text: String,
    pub turn: u32,
    pub timestamp_ms: i64,
    pub phase: ChatPhase,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WordlePhase {
    Lobby,
    Guessing,
    Banter,
    GameOver,
}

impl WordlePhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            WordlePhase::Lobby => "lobby",
            WordlePhase::Guessing => "guessing",
            WordlePhase::Banter => "banter",
            WordlePhase::GameOver => "game_over",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReason {
    AllSolvedOrEliminated,
    MaxGuessesExhausted,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action_type")]
pub enum WordleAction {
    Guess { word: String },
    SendMessage { message: String },
}

impl EnvironmentAction for WordleAction {
    fn action_type(&self) -> &str {
        match self {
            WordleAction::Guess { .. } => "guess",
            WordleAction::SendMessage { .. } => "send_message",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerProgress {
    pub player_id: PlayerId,
    pub display_name: String,
    pub guesses: Vec<GuessResult>,
    pub solved: bool,
    pub eliminated: bool,
    pub solved_turn: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpponentSummary {
    pub player_id: PlayerId,
    pub display_name: String,
    pub guess_count: u32,
    pub solved: bool,
    pub eliminated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WordlePlayerView {
    pub turn: u32,
    pub phase: String,
    pub my_progress: PlayerProgress,
    pub opponents: Vec<OpponentSummary>,
    pub chat_messages: Vec<ChatMessage>,
    pub revealed_target_word: Option<String>,
    pub needs_guess_this_turn: bool,
    pub is_terminal: bool,
    pub max_guesses: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WordleFullState {
    pub target_word: Option<String>,
    pub turn: u32,
    pub phase: WordlePhase,
    pub players: Vec<PlayerProgress>,
    pub chat_messages: Vec<ChatMessage>,
    pub is_terminal: bool,
    pub terminal_reason: Option<TerminalReason>,
    pub solve_order: Vec<PlayerId>,
}

impl EnvironmentState for WordleFullState {
    type PlayerId = PlayerId;

    fn turn_number(&self) -> u32 {
        self.turn
    }

    fn current_phase(&self) -> &str {
        self.phase.as_str()
    }

    fn player_ids(&self) -> Vec<Self::PlayerId> {
        self.players.iter().map(|p| p.player_id).collect()
    }

    fn is_terminal(&self) -> bool {
        self.is_terminal
    }
}

impl SequentialState for WordleFullState {
    fn sequential_phase(&self) -> SequentialPhase<Self::PlayerId> {
        if self.is_terminal {
            let winner = self
                .solve_order
                .first()
                .copied()
                .map(EnvironmentWinner::Player)
                .unwrap_or(EnvironmentWinner::Draw);
            return SequentialPhase::GameOver { winner };
        }

        let sent_message = |player_id: PlayerId, phase: ChatPhase| {
            self.chat_messages
                .iter()
                .any(|m| m.player_id == player_id && m.phase == phase)
        };
        let guessed_this_turn = |player: &PlayerProgress| {
            player
                .guesses
                .last()
                .map(|guess| guess.turn == self.turn)
                .unwrap_or(false)
        };

        let players: Vec<PlayerId> = match self.phase {
            WordlePhase::Lobby => self
                .players
                .iter()
                .filter(|p| !sent_message(p.player_id, ChatPhase::Lobby))
                .map(|p| p.player_id)
                .collect(),
            WordlePhase::Guessing => self
                .players
                .iter()
                .filter(|p| {
                    (p.solved && !sent_message(p.player_id, ChatPhase::Win))
                        || (!p.solved && !p.eliminated && !guessed_this_turn(p))
                })
                .map(|p| p.player_id)
                .collect(),
            WordlePhase::Banter => self
                .players
                .iter()
                .filter(|p| !sent_message(p.player_id, ChatPhase::Banter))
                .map(|p| p.player_id)
                .collect(),
            WordlePhase::GameOver => Vec::new(),
        };

        SequentialPhase::Decision {
            kind: SequentialDecisionKind::Active,
            players,
            deadline: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WordleConfig {
    pub max_guesses: u32,
    pub max_message_chars: u32,
}

impl Default for WordleConfig {
    fn default() -> Self {
        Self {
            max_guesses: 6,
            max_message_chars: 200,
        }
    }
}
