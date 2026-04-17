use std::collections::HashMap;

use connect_four_protocol::engine::EngineError;
use connect_four_protocol::*;

fn make_game() -> ConnectFourGame {
    let mut names = HashMap::new();
    names.insert(1, "Alice".to_string());
    names.insert(2, "Bob".to_string());
    ConnectFourGame::new(vec![1, 2], names).unwrap()
}

// ── Setup ───────────────────────────────────────────────────────────────

#[test]
fn new_game_requires_exactly_two_players() {
    let names = HashMap::new();
    assert!(ConnectFourGame::new(vec![1], names.clone()).is_err());
    assert!(ConnectFourGame::new(vec![1, 2, 3], names).is_err());
}

#[test]
fn new_game_rejects_duplicate_ids() {
    let names = HashMap::new();
    assert!(ConnectFourGame::new(vec![1, 1], names).is_err());
}

#[test]
fn initial_state_is_correct() {
    let game = make_game();
    let state = game.full_state();

    assert_eq!(state.turn, 1);
    assert_eq!(state.phase, ConnectFourPhase::Playing);
    assert_eq!(state.current_player, Some(1)); // Blue goes first
    assert!(state.winner.is_none());
    assert!(state.terminal_reason.is_none());
    assert!(state.move_history.is_empty());
    assert_eq!(state.players.len(), 2);
    assert_eq!(state.players[0].disc, CellState::Blue);
    assert_eq!(state.players[1].disc, CellState::Orange);
}

// ── Turn alternation ────────────────────────────────────────────────────

#[test]
fn turns_alternate_between_players() {
    let mut game = make_game();
    assert_eq!(game.full_state().current_player, Some(1));

    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    assert_eq!(game.full_state().current_player, Some(2));

    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    assert_eq!(game.full_state().current_player, Some(1));
}

#[test]
fn wrong_player_is_rejected() {
    let mut game = make_game();
    let err = game
        .apply_action(2, &ConnectFourAction { column: 0 })
        .unwrap_err();
    assert!(matches!(err, EngineError::NotYourTurn));
}

// ── Gravity ─────────────────────────────────────────────────────────────

#[test]
fn disc_falls_to_bottom() {
    let mut game = make_game();
    game.apply_action(1, &ConnectFourAction { column: 3 })
        .unwrap();

    let state = game.full_state();
    // Bottom row (row 5) should have the disc
    assert_eq!(state.board[5][3], CellState::Blue);
    // Row 4 should still be empty
    assert_eq!(state.board[4][3], CellState::Empty);
}

#[test]
fn discs_stack_in_same_column() {
    let mut game = make_game();
    game.apply_action(1, &ConnectFourAction { column: 3 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 3 })
        .unwrap();

    let state = game.full_state();
    assert_eq!(state.board[5][3], CellState::Blue);
    assert_eq!(state.board[4][3], CellState::Orange);
}

#[test]
fn full_column_is_rejected() {
    let mut game = make_game();
    // Fill column 0 (6 rows) by alternating: P1 col0, P2 col0, P1 col0, ...
    // Turns strictly alternate, so both players drop into col 0.
    for _ in 0..3 {
        game.apply_action(1, &ConnectFourAction { column: 0 })
            .unwrap();
        game.apply_action(2, &ConnectFourAction { column: 0 })
            .unwrap();
    }
    // Column 0 is now full (3 Red, 3 Yellow stacked)

    let state = game.full_state();
    assert_ne!(state.board[0][0], CellState::Empty);

    // Next action in column 0 should fail
    let current = state.current_player.expect("game still active");
    let err = game
        .apply_action(current, &ConnectFourAction { column: 0 })
        .unwrap_err();
    assert!(matches!(err, EngineError::ColumnFull(0)));
}

// ── Out of bounds ───────────────────────────────────────────────────────

#[test]
fn out_of_bounds_column_is_rejected() {
    let mut game = make_game();
    let err = game
        .apply_action(1, &ConnectFourAction { column: 7 })
        .unwrap_err();
    assert!(matches!(err, EngineError::OutOfBounds(7)));

    let err = game
        .apply_action(1, &ConnectFourAction { column: 255 })
        .unwrap_err();
    assert!(matches!(err, EngineError::OutOfBounds(255)));
}

// ── Legal actions ───────────────────────────────────────────────────────

#[test]
fn initial_legal_actions_are_all_columns() {
    let game = make_game();
    let actions = game.legal_actions(1);
    assert_eq!(actions.len(), 7);
    for (i, action) in actions.iter().enumerate() {
        assert_eq!(action.column, i as u8);
    }
}

#[test]
fn legal_actions_empty_for_wrong_player() {
    let game = make_game();
    assert!(game.legal_actions(2).is_empty());
}

#[test]
fn legal_actions_empty_when_terminal() {
    let mut game = make_game();
    // Horizontal win for player 1 (Red)
    // P1: col 0, P2: col 0, P1: col 1, P2: col 1, P1: col 2, P2: col 2, P1: col 3
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 3 })
        .unwrap();

    assert!(game.legal_actions(1).is_empty());
    assert!(game.legal_actions(2).is_empty());
}

// ── Win detection ───────────────────────────────────────────────────────

#[test]
fn horizontal_win() {
    let mut game = make_game();
    // P1 drops cols 0-3, P2 drops cols 0-2 (on top)
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 3 })
        .unwrap();

    let state = game.full_state();
    assert_eq!(state.phase, ConnectFourPhase::GameOver);
    assert_eq!(state.winner, Some(1));
    assert_eq!(state.terminal_reason, Some(TerminalReason::Win));
}

#[test]
fn vertical_win() {
    let mut game = make_game();
    // P1 stacks column 0, P2 plays column 1
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();

    let state = game.full_state();
    assert_eq!(state.phase, ConnectFourPhase::GameOver);
    assert_eq!(state.winner, Some(1));
}

#[test]
fn diagonal_down_right_win() {
    let mut game = make_game();
    // Build a rising diagonal for P1 (Red) from bottom-left:
    // P1 at (5,0), (4,1), (3,2), (2,3)
    //
    // Col 0: P1 drops → row 5  (1 disc)
    // Col 1: P2 drops → row 5, then P1 drops → row 4  (P1 at row 4)
    // Col 2: P2 drops → row 5, P1 drops → row 4 (waste), P2 drops → row 3... wait
    //
    // Easier approach: use specific turn-by-turn moves.
    // Turn 1: P1 col 0 → (5,0) Red
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    // Turn 2: P2 col 1 → (5,1) Yellow
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    // Turn 3: P1 col 1 → (4,1) Red
    game.apply_action(1, &ConnectFourAction { column: 1 })
        .unwrap();
    // Turn 4: P2 col 2 → (5,2) Yellow
    game.apply_action(2, &ConnectFourAction { column: 2 })
        .unwrap();
    // Turn 5: P1 col 6 → (5,6) Red  (waste move, avoid vertical in col 2)
    game.apply_action(1, &ConnectFourAction { column: 6 })
        .unwrap();
    // Turn 6: P2 col 2 → (4,2) Yellow
    game.apply_action(2, &ConnectFourAction { column: 2 })
        .unwrap();
    // Turn 7: P1 col 2 → (3,2) Red
    game.apply_action(1, &ConnectFourAction { column: 2 })
        .unwrap();
    // Turn 8: P2 col 3 → (5,3) Yellow
    game.apply_action(2, &ConnectFourAction { column: 3 })
        .unwrap();
    // Turn 9: P1 col 5 → (5,5) Red  (waste)
    game.apply_action(1, &ConnectFourAction { column: 5 })
        .unwrap();
    // Turn 10: P2 col 3 → (4,3) Yellow
    game.apply_action(2, &ConnectFourAction { column: 3 })
        .unwrap();
    // Turn 11: P1 col 4 → (5,4) Red  (waste)
    game.apply_action(1, &ConnectFourAction { column: 4 })
        .unwrap();
    // Turn 12: P2 col 3 → (3,3) Yellow
    game.apply_action(2, &ConnectFourAction { column: 3 })
        .unwrap();
    // Turn 13: P1 col 3 → (2,3) Red — completes diagonal: (5,0),(4,1),(3,2),(2,3)
    game.apply_action(1, &ConnectFourAction { column: 3 })
        .unwrap();

    let state = game.full_state();
    assert_eq!(state.phase, ConnectFourPhase::GameOver);
    assert_eq!(state.winner, Some(1));
}

#[test]
fn player_two_can_win() {
    let mut game = make_game();
    // P2 (Yellow) stacks column 2, P1 plays scattered
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 2 })
        .unwrap();

    let state = game.full_state();
    assert_eq!(state.phase, ConnectFourPhase::GameOver);
    assert_eq!(state.winner, Some(2));
}

// ── Draw ────────────────────────────────────────────────────────────────

#[test]
fn draw_when_board_full() {
    let mut game = make_game();
    // Use a greedy search to find a valid 42-move draw sequence.
    // For each turn, try columns 0-6 and pick one that doesn't immediately win.
    // Backtrack if stuck (all legal moves produce a win).
    fn find_draw_sequence() -> Option<Vec<u8>> {
        use connect_four_protocol::*;

        fn search(game: &mut ConnectFourGame, moves: &mut Vec<u8>, move_num: usize) -> bool {
            if move_num == 42 {
                // Board full — check it's actually a draw
                let state = game.full_state();
                return state.winner.is_none();
            }

            let player = if move_num.is_multiple_of(2) { 1 } else { 2 };
            let legal = game.legal_actions(player);

            for action in &legal {
                let col = action.column;
                let mut clone = game.clone();
                if clone.apply_action(player, action).is_err() {
                    continue;
                }

                // If this move caused a win, skip it
                if clone.is_terminal() {
                    let state = clone.full_state();
                    if state.winner.is_some() {
                        continue;
                    }
                    // Terminal with no winner = draw (board full)
                    moves.push(col);
                    return true;
                }

                moves.push(col);
                if search(&mut clone, moves, move_num + 1) {
                    return true;
                }
                moves.pop();
            }
            false
        }

        let mut game = {
            let mut names = std::collections::HashMap::new();
            names.insert(1, "Alice".to_string());
            names.insert(2, "Bob".to_string());
            ConnectFourGame::new(vec![1, 2], names).unwrap()
        };
        let mut moves = Vec::with_capacity(42);
        if search(&mut game, &mut moves, 0) {
            Some(moves)
        } else {
            None
        }
    }

    let columns = find_draw_sequence().expect("Should be able to find a draw sequence");
    assert_eq!(columns.len(), 42);

    // Replay through the actual game
    for (i, &col) in columns.iter().enumerate() {
        let player = if i.is_multiple_of(2) { 1 } else { 2 };
        game.apply_action(player, &ConnectFourAction { column: col })
            .unwrap_or_else(|e| panic!("Move {i} (P{player} col {col}) failed: {e}"));
    }

    let state = game.full_state();
    assert_eq!(
        state.phase,
        ConnectFourPhase::GameOver,
        "Game should be over"
    );
    assert_eq!(state.winner, None, "Expected draw but someone won");
    assert_eq!(state.terminal_reason, Some(TerminalReason::Draw));
}

// ── Post-terminal rejection ─────────────────────────────────────────────

#[test]
fn action_after_game_over_is_rejected() {
    let mut game = make_game();
    // Quick horizontal win
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 3 })
        .unwrap();

    assert!(game.is_terminal());
    let err = game
        .apply_action(2, &ConnectFourAction { column: 4 })
        .unwrap_err();
    assert!(matches!(err, EngineError::GameOver));
}

// ── Move history ────────────────────────────────────────────────────────

#[test]
fn move_history_tracks_all_moves() {
    let mut game = make_game();
    game.apply_action(1, &ConnectFourAction { column: 3 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 4 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 3 })
        .unwrap();

    let state = game.full_state();
    assert_eq!(state.move_history.len(), 3);

    assert_eq!(state.move_history[0].player_id, 1);
    assert_eq!(state.move_history[0].column, 3);
    assert_eq!(state.move_history[0].row, 5); // bottom row
    assert_eq!(state.move_history[0].disc, CellState::Blue);
    assert_eq!(state.move_history[0].turn, 1);

    assert_eq!(state.move_history[1].player_id, 2);
    assert_eq!(state.move_history[1].column, 4);
    assert_eq!(state.move_history[1].row, 5);
    assert_eq!(state.move_history[1].disc, CellState::Orange);
    assert_eq!(state.move_history[1].turn, 2);

    assert_eq!(state.move_history[2].player_id, 1);
    assert_eq!(state.move_history[2].column, 3);
    assert_eq!(state.move_history[2].row, 4); // stacked above first
    assert_eq!(state.move_history[2].turn, 3);
}

// ── Rendering ───────────────────────────────────────────────────────────

#[test]
fn ascii_board_rendering() {
    let mut game = make_game();
    game.apply_action(1, &ConnectFourAction { column: 2 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 3 })
        .unwrap();

    let ascii = board_to_ascii(&game.full_state().board);
    let lines: Vec<&str> = ascii.lines().collect();
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[5], ". . x o . . ."); // bottom row
    assert_eq!(lines[0], ". . . . . . ."); // top row
}

#[test]
fn ascii_board_with_labels() {
    let game = make_game();
    let ascii = board_to_ascii_with_labels(&game.full_state().board);
    let lines: Vec<&str> = ascii.lines().collect();
    assert_eq!(lines.len(), 7); // 1 header + 6 rows
    assert_eq!(lines[0], "0 1 2 3 4 5 6");
}

#[test]
fn move_history_string() {
    let mut game = make_game();
    game.apply_action(1, &ConnectFourAction { column: 3 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 4 })
        .unwrap();

    let history = move_history_to_string(&game.full_state().move_history);
    assert_eq!(history, "x(col=3) o(col=4)");
}

// ── SequentialState trait ───────────────────────────────────────────────

#[test]
fn sequential_phase_during_play() {
    use eval_runtime::{SequentialPhase, SequentialState};

    let game = make_game();
    let state = game.full_state();

    match state.sequential_phase() {
        SequentialPhase::Decision { players, .. } => {
            assert_eq!(players, vec![1]);
        }
        _ => panic!("expected Decision phase"),
    }
}

#[test]
fn sequential_phase_game_over_win() {
    use eval_runtime::{EnvironmentWinner, SequentialPhase, SequentialState};

    let mut game = make_game();
    // Vertical win for P1
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();
    game.apply_action(2, &ConnectFourAction { column: 1 })
        .unwrap();
    game.apply_action(1, &ConnectFourAction { column: 0 })
        .unwrap();

    let state = game.full_state();
    match state.sequential_phase() {
        SequentialPhase::GameOver { winner } => {
            assert_eq!(winner, EnvironmentWinner::Player(1));
        }
        _ => panic!("expected GameOver phase"),
    }
}
