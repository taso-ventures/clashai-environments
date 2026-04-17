use std::collections::HashMap;

use thiserror::Error;

use crate::{
    CellState, ConnectFourAction, ConnectFourFullState, ConnectFourPhase, ConnectFourPlayer,
    MoveRecord, PlayerId, TerminalReason, COLS, CONNECT, ROWS,
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("invalid setup: {0}")]
    InvalidSetup(String),
    #[error("unknown player id: {0}")]
    UnknownPlayer(PlayerId),
    #[error("not this player's turn")]
    NotYourTurn,
    #[error("column {0} is full")]
    ColumnFull(u8),
    #[error("column {0} is out of bounds (must be 0–{max})", max = COLS - 1)]
    OutOfBounds(u8),
    #[error("game is already over")]
    GameOver,
}

#[derive(Clone)]
pub struct ConnectFourGame {
    board: [[CellState; COLS]; ROWS],
    players: Vec<ConnectFourPlayer>,
    current_player_idx: usize,
    turn: u32,
    phase: ConnectFourPhase,
    winner: Option<PlayerId>,
    terminal_reason: Option<TerminalReason>,
    move_history: Vec<MoveRecord>,
}

impl ConnectFourGame {
    /// Create a new Connect Four game.
    ///
    /// Requires exactly 2 player IDs. Player 0 in the list plays Blue (goes first),
    /// player 1 plays Orange.
    pub fn new(
        player_ids: Vec<PlayerId>,
        player_names: HashMap<PlayerId, String>,
    ) -> Result<Self, EngineError> {
        if player_ids.len() != 2 {
            return Err(EngineError::InvalidSetup(format!(
                "connect four requires exactly 2 players, got {}",
                player_ids.len()
            )));
        }
        if player_ids[0] == player_ids[1] {
            return Err(EngineError::InvalidSetup(
                "player_ids must be unique".to_string(),
            ));
        }

        let discs = [CellState::Blue, CellState::Orange];
        let players = player_ids
            .iter()
            .enumerate()
            .map(|(idx, &pid)| ConnectFourPlayer {
                player_id: pid,
                display_name: player_names
                    .get(&pid)
                    .cloned()
                    .unwrap_or_else(|| format!("Player {pid}")),
                disc: discs[idx],
            })
            .collect();

        Ok(Self {
            board: [[CellState::Empty; COLS]; ROWS],
            players,
            current_player_idx: 0,
            turn: 1,
            phase: ConnectFourPhase::Playing,
            winner: None,
            terminal_reason: None,
            move_history: Vec::new(),
        })
    }

    /// Return the full game state.
    pub fn full_state(&self) -> ConnectFourFullState {
        ConnectFourFullState {
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
    ///
    /// A column is legal if its top row (row 0) is empty.
    pub fn legal_actions(&self, player_id: PlayerId) -> Vec<ConnectFourAction> {
        if self.phase == ConnectFourPhase::GameOver {
            return vec![];
        }
        if self.players[self.current_player_idx].player_id != player_id {
            return vec![];
        }

        (0..COLS as u8)
            .filter(|&col| self.board[0][col as usize] == CellState::Empty)
            .map(|column| ConnectFourAction { column })
            .collect()
    }

    /// Apply a move: drop a disc into the given column.
    ///
    /// The disc falls to the lowest empty row (gravity).
    pub fn apply_action(
        &mut self,
        player_id: PlayerId,
        action: &ConnectFourAction,
    ) -> Result<(), EngineError> {
        if self.phase == ConnectFourPhase::GameOver {
            return Err(EngineError::GameOver);
        }

        if self.players[self.current_player_idx].player_id != player_id {
            return Err(EngineError::NotYourTurn);
        }

        let col = action.column as usize;
        if col >= COLS {
            return Err(EngineError::OutOfBounds(action.column));
        }

        // Find the lowest empty row in this column (gravity)
        let row = self
            .lowest_empty_row(col)
            .ok_or(EngineError::ColumnFull(action.column))?;

        let disc = self.players[self.current_player_idx].disc;
        self.board[row][col] = disc;

        self.move_history.push(MoveRecord {
            player_id,
            disc,
            column: action.column,
            row: row as u8,
            turn: self.turn,
        });

        // Check for win
        if self.check_win(row, col, disc) {
            self.phase = ConnectFourPhase::GameOver;
            self.winner = Some(player_id);
            self.terminal_reason = Some(TerminalReason::Win);
            return Ok(());
        }

        // Check for draw (board full)
        if self.is_board_full() {
            self.phase = ConnectFourPhase::GameOver;
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
        self.phase == ConnectFourPhase::GameOver
    }

    /// Find the lowest empty row in a column (bottom = row ROWS-1).
    fn lowest_empty_row(&self, col: usize) -> Option<usize> {
        (0..ROWS)
            .rev()
            .find(|&r| self.board[r][col] == CellState::Empty)
    }

    /// Check if the last placed disc at (row, col) forms a winning line.
    ///
    /// Checks all 4 directions: horizontal, vertical, diagonal-down-right,
    /// diagonal-down-left.
    fn check_win(&self, row: usize, col: usize, disc: CellState) -> bool {
        // Direction vectors: (row_delta, col_delta)
        const DIRECTIONS: [(i32, i32); 4] = [
            (0, 1),  // horizontal
            (1, 0),  // vertical
            (1, 1),  // diagonal down-right
            (1, -1), // diagonal down-left
        ];

        for (dr, dc) in DIRECTIONS {
            let mut count = 1; // count the placed piece itself

            // Count in positive direction
            count += self.count_in_direction(row, col, dr, dc, disc);
            // Count in negative direction
            count += self.count_in_direction(row, col, -dr, -dc, disc);

            if count >= CONNECT {
                return true;
            }
        }

        false
    }

    /// Count consecutive matching discs starting from (row, col) in direction (dr, dc),
    /// not counting the starting cell itself.
    fn count_in_direction(
        &self,
        row: usize,
        col: usize,
        dr: i32,
        dc: i32,
        disc: CellState,
    ) -> usize {
        let mut count = 0;
        let mut r = row as i32 + dr;
        let mut c = col as i32 + dc;

        while r >= 0 && r < ROWS as i32 && c >= 0 && c < COLS as i32 {
            if self.board[r as usize][c as usize] != disc {
                break;
            }
            count += 1;
            r += dr;
            c += dc;
        }

        count
    }

    fn is_board_full(&self) -> bool {
        // Board is full if all top-row cells are occupied
        (0..COLS).all(|col| self.board[0][col] != CellState::Empty)
    }
}
