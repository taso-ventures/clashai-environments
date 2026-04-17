pub mod engine;

pub use engine::{ConnectFourGame, EngineError};

use std::fmt;

use eval_runtime::{
    EnvironmentAction, EnvironmentState, EnvironmentWinner, SequentialDecisionKind,
    SequentialPhase, SequentialState,
};
use serde::{Deserialize, Serialize};

pub type PlayerId = i32;

pub const ROWS: usize = 6;
pub const COLS: usize = 7;
pub const CONNECT: usize = 4;

pub const CONNECT_FOUR_RULES: &str = include_str!("../resources/connect_four_rules.md");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellState {
    Empty,
    Blue,
    Orange,
}

impl CellState {
    pub fn as_char(self) -> char {
        match self {
            CellState::Empty => '.',
            CellState::Blue => 'x',
            CellState::Orange => 'o',
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            CellState::Empty => "empty",
            CellState::Blue => "Blue",
            CellState::Orange => "Orange",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectFourPhase {
    Playing,
    GameOver,
}

impl ConnectFourPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectFourPhase::Playing => "playing",
            ConnectFourPhase::GameOver => "game_over",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReason {
    Win,
    Draw,
}

/// A Connect Four move — just the column (0–6). The row is determined by gravity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectFourAction {
    pub column: u8,
}

impl EnvironmentAction for ConnectFourAction {
    fn action_type(&self) -> &str {
        "drop"
    }
}

impl fmt::Display for ConnectFourAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.column)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveRecord {
    pub player_id: PlayerId,
    pub disc: CellState,
    pub column: u8,
    pub row: u8,
    pub turn: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectFourPlayer {
    pub player_id: PlayerId,
    pub display_name: String,
    pub disc: CellState,
}

/// Full game state — used by orchestration and spectators.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectFourFullState {
    pub board: [[CellState; COLS]; ROWS],
    pub current_player: PlayerId,
    pub turn: u32,
    pub phase: ConnectFourPhase,
    pub winner: Option<PlayerId>,
    pub terminal_reason: Option<TerminalReason>,
    pub move_history: Vec<MoveRecord>,
    pub players: Vec<ConnectFourPlayer>,
}

/// Player-specific view — same as full state (Connect Four is a perfect-information game).
pub type ConnectFourPlayerView = ConnectFourFullState;

impl EnvironmentState for ConnectFourFullState {
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
        self.phase == ConnectFourPhase::GameOver
    }
}

impl SequentialState for ConnectFourFullState {
    fn sequential_phase(&self) -> SequentialPhase<Self::PlayerId> {
        if self.is_terminal() {
            let winner = match self.winner {
                Some(pid) => EnvironmentWinner::Player(pid),
                None => EnvironmentWinner::Draw,
            };
            return SequentialPhase::GameOver { winner };
        }

        SequentialPhase::Decision {
            kind: SequentialDecisionKind::Active,
            players: vec![self.current_player],
            deadline: None,
        }
    }
}

/// Render the board as ASCII text.
///
/// Format: rows top-to-bottom, columns left-to-right.
/// Row 0 is the top row. Uses `x` for Blue, `o` for Orange, `.` for empty.
///
/// ```text
/// . . . . . . .
/// . . . . . . .
/// . . . . . . .
/// . . . . . . .
/// . . . . . . .
/// . . x o . . .
/// ```
pub fn board_to_ascii(board: &[[CellState; COLS]; ROWS]) -> String {
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

/// Render the board with column labels for LLM prompts.
///
/// ```text
/// 0 1 2 3 4 5 6
/// . . . . . . .
/// . . . . . . .
/// . . . . . . .
/// . . . . . . .
/// . . . . . . .
/// . . x o . . .
/// ```
pub fn board_to_ascii_with_labels(board: &[[CellState; COLS]; ROWS]) -> String {
    let header = (0..COLS)
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{header}\n{}", board_to_ascii(board))
}

/// Render move history as a string: `x(col=2) o(col=3) x(col=2)`.
pub fn move_history_to_string(history: &[MoveRecord]) -> String {
    history
        .iter()
        .map(|m| format!("{}(col={})", m.disc.as_char(), m.column))
        .collect::<Vec<_>>()
        .join(" ")
}
