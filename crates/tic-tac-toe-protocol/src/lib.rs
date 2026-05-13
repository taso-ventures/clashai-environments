pub mod engine;

pub use engine::{EngineError, TicTacToeGame};

use std::fmt;

use eval_runtime::{
    EnvironmentAction, EnvironmentState, EnvironmentWinner, SequentialDecisionKind,
    SequentialPhase, SequentialState,
};
use serde::{Deserialize, Serialize};

pub type PlayerId = i32;

pub const TIC_TAC_TOE_RULES: &str = include_str!("../resources/tic_tac_toe_rules.md");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellState {
    Empty,
    X,
    O,
}

impl CellState {
    pub fn as_char(self) -> char {
        match self {
            CellState::Empty => '.',
            CellState::X => 'x',
            CellState::O => 'o',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicTacToePhase {
    Playing,
    GameOver,
}

impl TicTacToePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            TicTacToePhase::Playing => "playing",
            TicTacToePhase::GameOver => "game_over",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReason {
    Win,
    Draw,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicTacToeAction {
    pub row: u8,
    pub col: u8,
}

impl EnvironmentAction for TicTacToeAction {
    fn action_type(&self) -> &str {
        "move"
    }
}

impl fmt::Display for TicTacToeAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({},{})", self.row, self.col)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveRecord {
    pub player_id: PlayerId,
    pub mark: CellState,
    pub row: u8,
    pub col: u8,
    pub turn: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicTacToePlayer {
    pub player_id: PlayerId,
    pub display_name: String,
    pub mark: CellState,
}

/// Full game state — used by orchestration and spectators.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicTacToeFullState {
    pub board: [[CellState; 3]; 3],
    /// Id of the player to act next, or `None` if the game is in
    /// `game_over` (there is no next turn).
    pub current_player: Option<PlayerId>,
    pub turn: u32,
    pub phase: TicTacToePhase,
    pub winner: Option<PlayerId>,
    pub terminal_reason: Option<TerminalReason>,
    pub move_history: Vec<MoveRecord>,
    pub players: Vec<TicTacToePlayer>,
}

/// Player-specific view — same as full state (tic-tac-toe is a perfect-information game).
pub type TicTacToePlayerView = TicTacToeFullState;

impl EnvironmentState for TicTacToeFullState {
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
        self.phase == TicTacToePhase::GameOver
    }
}

impl SequentialState for TicTacToeFullState {
    fn sequential_phase(&self) -> SequentialPhase<Self::PlayerId> {
        if self.is_terminal() {
            let winner = match self.winner {
                Some(pid) => EnvironmentWinner::Player(pid),
                None => EnvironmentWinner::Draw,
            };
            return SequentialPhase::GameOver { winner };
        }

        // Tic-tac-toe: exactly one active player per turn.
        SequentialPhase::Decision {
            kind: SequentialDecisionKind::Active,
            players: self.current_player.into_iter().collect(),
            deadline: None,
        }
    }
}

/// Render the board as ASCII text for LLM prompts.
///
/// Format: `x . .` / `. o .` / `. . x` (one row per line, space-separated).
pub fn board_to_ascii(board: &[[CellState; 3]; 3]) -> String {
    board
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.as_char().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render move history as a string: `x(0,0) o(1,1) x(2,2)`.
pub fn move_history_to_string(history: &[MoveRecord]) -> String {
    history
        .iter()
        .map(|m| format!("{}({},{})", m.mark.as_char(), m.row, m.col))
        .collect::<Vec<_>>()
        .join(" ")
}
