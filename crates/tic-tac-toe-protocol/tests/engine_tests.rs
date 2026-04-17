use std::collections::HashMap;

use tic_tac_toe_protocol::{
    board_to_ascii, move_history_to_string, CellState, EngineError, PlayerId, TerminalReason,
    TicTacToeAction, TicTacToeGame, TicTacToePhase,
};

fn make_game() -> TicTacToeGame {
    let ids = vec![0, 1];
    let names = HashMap::from([(0, "Alice".to_string()), (1, "Bob".to_string())]);
    TicTacToeGame::new(ids, names).unwrap()
}

fn apply(game: &mut TicTacToeGame, player: PlayerId, row: u8, col: u8) {
    game.apply_action(player, &TicTacToeAction { row, col })
        .unwrap();
}

// ── Setup tests ──────────────────────────────────────────────────────

#[test]
fn new_game_initial_state() {
    let game = make_game();
    let state = game.full_state();
    assert_eq!(state.turn, 1);
    assert_eq!(state.phase, TicTacToePhase::Playing);
    assert_eq!(state.current_player, 0);
    assert!(state.winner.is_none());
    assert!(state.terminal_reason.is_none());
    assert!(state.move_history.is_empty());
    assert_eq!(state.players.len(), 2);
    assert_eq!(state.players[0].mark, CellState::X);
    assert_eq!(state.players[1].mark, CellState::O);
}

#[test]
fn rejects_wrong_player_count() {
    let result = TicTacToeGame::new(vec![0], HashMap::new());
    assert!(matches!(result, Err(EngineError::InvalidSetup(_))));

    let result = TicTacToeGame::new(vec![0, 1, 2], HashMap::new());
    assert!(matches!(result, Err(EngineError::InvalidSetup(_))));
}

#[test]
fn rejects_duplicate_player_ids() {
    let result = TicTacToeGame::new(vec![0, 0], HashMap::new());
    assert!(matches!(result, Err(EngineError::InvalidSetup(_))));
}

// ── Move validation ─────────────────────────────────────────────────

#[test]
fn alternating_turns() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    assert_eq!(game.full_state().current_player, 1);
    apply(&mut game, 1, 1, 1);
    assert_eq!(game.full_state().current_player, 0);
}

#[test]
fn rejects_wrong_player_turn() {
    let mut game = make_game();
    let result = game.apply_action(1, &TicTacToeAction { row: 0, col: 0 });
    assert!(matches!(result, Err(EngineError::NotYourTurn)));
}

#[test]
fn rejects_occupied_cell() {
    let mut game = make_game();
    apply(&mut game, 0, 1, 1);
    let result = game.apply_action(1, &TicTacToeAction { row: 1, col: 1 });
    assert!(matches!(result, Err(EngineError::CellOccupied { .. })));
}

#[test]
fn rejects_out_of_bounds() {
    let mut game = make_game();
    let result = game.apply_action(0, &TicTacToeAction { row: 3, col: 0 });
    assert!(matches!(result, Err(EngineError::OutOfBounds { .. })));
}

#[test]
fn rejects_move_after_game_over() {
    let mut game = make_game();
    // X wins top row
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 1, 0);
    apply(&mut game, 0, 0, 1);
    apply(&mut game, 1, 1, 1);
    apply(&mut game, 0, 0, 2); // X wins

    let result = game.apply_action(1, &TicTacToeAction { row: 2, col: 0 });
    assert!(matches!(result, Err(EngineError::GameOver)));
}

// ── Legal actions ────────────────────────────────────────────────────

#[test]
fn legal_actions_initial() {
    let game = make_game();
    let actions = game.legal_actions(0);
    assert_eq!(actions.len(), 9);
    // Non-current player has no actions
    assert!(game.legal_actions(1).is_empty());
}

#[test]
fn legal_actions_decrease_after_moves() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 1, 1);
    let actions = game.legal_actions(0);
    assert_eq!(actions.len(), 7);
}

#[test]
fn legal_actions_empty_when_game_over() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 1, 0);
    apply(&mut game, 0, 0, 1);
    apply(&mut game, 1, 1, 1);
    apply(&mut game, 0, 0, 2); // X wins
    assert!(game.legal_actions(0).is_empty());
    assert!(game.legal_actions(1).is_empty());
}

// ── Win detection (all 8 lines) ──────────────────────────────────────

#[test]
fn win_row_0() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 1, 0);
    apply(&mut game, 0, 0, 1);
    apply(&mut game, 1, 1, 1);
    apply(&mut game, 0, 0, 2);
    let state = game.full_state();
    assert_eq!(state.winner, Some(0));
    assert_eq!(state.terminal_reason, Some(TerminalReason::Win));
}

#[test]
fn win_row_1() {
    let mut game = make_game();
    apply(&mut game, 0, 1, 0);
    apply(&mut game, 1, 0, 0);
    apply(&mut game, 0, 1, 1);
    apply(&mut game, 1, 0, 1);
    apply(&mut game, 0, 1, 2);
    assert_eq!(game.full_state().winner, Some(0));
}

#[test]
fn win_row_2() {
    let mut game = make_game();
    apply(&mut game, 0, 2, 0);
    apply(&mut game, 1, 0, 0);
    apply(&mut game, 0, 2, 1);
    apply(&mut game, 1, 0, 1);
    apply(&mut game, 0, 2, 2);
    assert_eq!(game.full_state().winner, Some(0));
}

#[test]
fn win_col_0() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 0, 1);
    apply(&mut game, 0, 1, 0);
    apply(&mut game, 1, 1, 1);
    apply(&mut game, 0, 2, 0);
    assert_eq!(game.full_state().winner, Some(0));
}

#[test]
fn win_col_1() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 1);
    apply(&mut game, 1, 0, 0);
    apply(&mut game, 0, 1, 1);
    apply(&mut game, 1, 1, 0);
    apply(&mut game, 0, 2, 1);
    assert_eq!(game.full_state().winner, Some(0));
}

#[test]
fn win_col_2() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 2);
    apply(&mut game, 1, 0, 0);
    apply(&mut game, 0, 1, 2);
    apply(&mut game, 1, 1, 0);
    apply(&mut game, 0, 2, 2);
    assert_eq!(game.full_state().winner, Some(0));
}

#[test]
fn win_diagonal_top_left_to_bottom_right() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 0, 1);
    apply(&mut game, 0, 1, 1);
    apply(&mut game, 1, 0, 2);
    apply(&mut game, 0, 2, 2);
    assert_eq!(game.full_state().winner, Some(0));
}

#[test]
fn win_diagonal_top_right_to_bottom_left() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 2);
    apply(&mut game, 1, 0, 0);
    apply(&mut game, 0, 1, 1);
    apply(&mut game, 1, 1, 0);
    apply(&mut game, 0, 2, 0);
    assert_eq!(game.full_state().winner, Some(0));
}

#[test]
fn player_o_can_win() {
    let mut game = make_game();
    // X plays non-winning moves, O wins col 2
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 0, 2);
    apply(&mut game, 0, 1, 0);
    apply(&mut game, 1, 1, 2);
    apply(&mut game, 0, 2, 1); // X doesn't complete anything
    apply(&mut game, 1, 2, 2); // O wins col 2
    let state = game.full_state();
    assert_eq!(state.winner, Some(1));
    assert_eq!(state.terminal_reason, Some(TerminalReason::Win));
}

// ── Draw detection ───────────────────────────────────────────────────

#[test]
fn draw_full_board() {
    let mut game = make_game();
    // Classic draw pattern:
    // x o x
    // x x o
    // o x o
    apply(&mut game, 0, 0, 0); // X
    apply(&mut game, 1, 0, 1); // O
    apply(&mut game, 0, 0, 2); // X
    apply(&mut game, 1, 1, 2); // O
    apply(&mut game, 0, 1, 0); // X
    apply(&mut game, 1, 2, 0); // O
    apply(&mut game, 0, 1, 1); // X
    apply(&mut game, 1, 2, 2); // O
    apply(&mut game, 0, 2, 1); // X

    let state = game.full_state();
    assert!(state.winner.is_none());
    assert_eq!(state.terminal_reason, Some(TerminalReason::Draw));
    assert_eq!(state.phase, TicTacToePhase::GameOver);
}

// ── Move history ─────────────────────────────────────────────────────

#[test]
fn move_history_tracks_moves() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 1, 1);
    apply(&mut game, 0, 2, 2);

    let state = game.full_state();
    assert_eq!(state.move_history.len(), 3);
    assert_eq!(state.move_history[0].mark, CellState::X);
    assert_eq!(state.move_history[1].mark, CellState::O);
    assert_eq!(state.move_history[2].mark, CellState::X);
}

// ── Board rendering ──────────────────────────────────────────────────

#[test]
fn board_to_ascii_empty() {
    let board = [[CellState::Empty; 3]; 3];
    assert_eq!(board_to_ascii(&board), ". . .\n. . .\n. . .");
}

#[test]
fn board_to_ascii_with_marks() {
    let mut board = [[CellState::Empty; 3]; 3];
    board[0][0] = CellState::X;
    board[1][1] = CellState::O;
    board[2][2] = CellState::X;
    assert_eq!(board_to_ascii(&board), "x . .\n. o .\n. . x");
}

#[test]
fn move_history_string_empty() {
    assert_eq!(move_history_to_string(&[]), "");
}

#[test]
fn move_history_string_with_moves() {
    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 1, 1);
    apply(&mut game, 0, 2, 2);

    let state = game.full_state();
    assert_eq!(
        move_history_to_string(&state.move_history),
        "x(0,0) o(1,1) x(2,2)"
    );
}

// ── SequentialState trait ────────────────────────────────────────────

#[test]
fn sequential_phase_active_during_play() {
    use eval_runtime::{SequentialDecisionKind, SequentialPhase, SequentialState};

    let game = make_game();
    let state = game.full_state();
    match state.sequential_phase() {
        SequentialPhase::Decision {
            kind,
            players,
            deadline,
        } => {
            assert_eq!(kind, SequentialDecisionKind::Active);
            assert_eq!(players, vec![0]);
            assert!(deadline.is_none());
        }
        other => panic!("expected Decision, got {other:?}"),
    }
}

#[test]
fn sequential_phase_game_over_win() {
    use eval_runtime::{EnvironmentWinner, SequentialPhase, SequentialState};

    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 1, 0);
    apply(&mut game, 0, 0, 1);
    apply(&mut game, 1, 1, 1);
    apply(&mut game, 0, 0, 2);

    let state = game.full_state();
    match state.sequential_phase() {
        SequentialPhase::GameOver { winner } => {
            assert_eq!(winner, EnvironmentWinner::Player(0));
        }
        other => panic!("expected GameOver, got {other:?}"),
    }
}

#[test]
fn sequential_phase_game_over_draw() {
    use eval_runtime::{EnvironmentWinner, SequentialPhase, SequentialState};

    let mut game = make_game();
    apply(&mut game, 0, 0, 0);
    apply(&mut game, 1, 0, 1);
    apply(&mut game, 0, 0, 2);
    apply(&mut game, 1, 1, 2);
    apply(&mut game, 0, 1, 0);
    apply(&mut game, 1, 2, 0);
    apply(&mut game, 0, 1, 1);
    apply(&mut game, 1, 2, 2);
    apply(&mut game, 0, 2, 1);

    let state = game.full_state();
    match state.sequential_phase() {
        SequentialPhase::GameOver { winner } => {
            assert_eq!(winner, EnvironmentWinner::Draw);
        }
        other => panic!("expected GameOver, got {other:?}"),
    }
}
