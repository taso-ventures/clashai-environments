use std::collections::HashMap;

use thiserror::Error;

use crate::{
    CellState, MoveRecord, PlayerId, TerminalReason, TicTacToeAction, TicTacToeFullState,
    TicTacToePhase, TicTacToePlayer,
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("invalid setup: {0}")]
    InvalidSetup(String),
    #[error("unknown player id: {0}")]
    UnknownPlayer(PlayerId),
    #[error("not this player's turn")]
    NotYourTurn,
    #[error("cell ({row},{col}) is already occupied")]
    CellOccupied { row: u8, col: u8 },
    #[error("cell ({row},{col}) is out of bounds")]
    OutOfBounds { row: u8, col: u8 },
    #[error("game is already over")]
    GameOver,
}

/// The 8 lines to check for a win: 3 rows, 3 cols, 2 diagonals.
const WIN_LINES: [[(usize, usize); 3]; 8] = [
    // Rows
    [(0, 0), (0, 1), (0, 2)],
    [(1, 0), (1, 1), (1, 2)],
    [(2, 0), (2, 1), (2, 2)],
    // Columns
    [(0, 0), (1, 0), (2, 0)],
    [(0, 1), (1, 1), (2, 1)],
    [(0, 2), (1, 2), (2, 2)],
    // Diagonals
    [(0, 0), (1, 1), (2, 2)],
    [(0, 2), (1, 1), (2, 0)],
];

pub struct TicTacToeGame {
    board: [[CellState; 3]; 3],
    players: Vec<TicTacToePlayer>,
    current_player_idx: usize,
    turn: u32,
    phase: TicTacToePhase,
    winner: Option<PlayerId>,
    terminal_reason: Option<TerminalReason>,
    move_history: Vec<MoveRecord>,
}

impl TicTacToeGame {
    /// Create a new tic-tac-toe game.
    ///
    /// Requires exactly 2 player IDs. Player 0 in the list plays X (goes first),
    /// player 1 plays O.
    pub fn new(
        player_ids: Vec<PlayerId>,
        player_names: HashMap<PlayerId, String>,
    ) -> Result<Self, EngineError> {
        if player_ids.len() != 2 {
            return Err(EngineError::InvalidSetup(format!(
                "tic-tac-toe requires exactly 2 players, got {}",
                player_ids.len()
            )));
        }
        if player_ids[0] == player_ids[1] {
            return Err(EngineError::InvalidSetup(
                "player_ids must be unique".to_string(),
            ));
        }

        let marks = [CellState::X, CellState::O];
        let players = player_ids
            .iter()
            .enumerate()
            .map(|(idx, &pid)| TicTacToePlayer {
                player_id: pid,
                display_name: player_names
                    .get(&pid)
                    .cloned()
                    .unwrap_or_else(|| format!("Player {pid}")),
                mark: marks[idx],
            })
            .collect();

        Ok(Self {
            board: [[CellState::Empty; 3]; 3],
            players,
            current_player_idx: 0,
            turn: 1,
            phase: TicTacToePhase::Playing,
            winner: None,
            terminal_reason: None,
            move_history: Vec::new(),
        })
    }

    /// Return the full game state.
    pub fn full_state(&self) -> TicTacToeFullState {
        TicTacToeFullState {
            board: self.board,
            current_player: self.players[self.current_player_idx].player_id,
            turn: self.turn,
            phase: self.phase,
            winner: self.winner,
            terminal_reason: self.terminal_reason,
            move_history: self.move_history.clone(),
            players: self.players.clone(),
        }
    }

    /// Return the legal actions for the given player.
    pub fn legal_actions(&self, player_id: PlayerId) -> Vec<TicTacToeAction> {
        if self.phase == TicTacToePhase::GameOver {
            return vec![];
        }
        if self.players[self.current_player_idx].player_id != player_id {
            return vec![];
        }

        let mut actions = Vec::new();
        for row in 0..3u8 {
            for col in 0..3u8 {
                if self.board[row as usize][col as usize] == CellState::Empty {
                    actions.push(TicTacToeAction { row, col });
                }
            }
        }
        actions
    }

    /// Apply a move for the given player.
    pub fn apply_action(
        &mut self,
        player_id: PlayerId,
        action: &TicTacToeAction,
    ) -> Result<(), EngineError> {
        if self.phase == TicTacToePhase::GameOver {
            return Err(EngineError::GameOver);
        }

        if self.players[self.current_player_idx].player_id != player_id {
            return Err(EngineError::NotYourTurn);
        }

        let row = action.row as usize;
        let col = action.col as usize;

        if row >= 3 || col >= 3 {
            return Err(EngineError::OutOfBounds {
                row: action.row,
                col: action.col,
            });
        }

        if self.board[row][col] != CellState::Empty {
            return Err(EngineError::CellOccupied {
                row: action.row,
                col: action.col,
            });
        }

        let mark = self.players[self.current_player_idx].mark;
        self.board[row][col] = mark;

        self.move_history.push(MoveRecord {
            player_id,
            mark,
            row: action.row,
            col: action.col,
            turn: self.turn,
        });

        // Check for win
        if self.check_win(mark) {
            self.phase = TicTacToePhase::GameOver;
            self.winner = Some(player_id);
            self.terminal_reason = Some(TerminalReason::Win);
            return Ok(());
        }

        // Check for draw (board full)
        if self.is_board_full() {
            self.phase = TicTacToePhase::GameOver;
            self.terminal_reason = Some(TerminalReason::Draw);
            return Ok(());
        }

        // Switch player
        self.current_player_idx = 1 - self.current_player_idx;
        self.turn += 1;

        Ok(())
    }

    /// Whether the game has reached a terminal state.
    pub fn is_terminal(&self) -> bool {
        self.phase == TicTacToePhase::GameOver
    }

    fn check_win(&self, mark: CellState) -> bool {
        WIN_LINES
            .iter()
            .any(|line| line.iter().all(|&(r, c)| self.board[r][c] == mark))
    }

    fn is_board_full(&self) -> bool {
        self.board
            .iter()
            .all(|row| row.iter().all(|cell| *cell != CellState::Empty))
    }
}
