//! Integration tests for TicTacToeEnvironment (Environment adapter).

use std::collections::HashMap;

use environment_engine::registry::EnvironmentRegistry;
use environment_engine::tic_tac_toe::TicTacToeEnvironment;
use environment_engine::{Environment, EnvironmentConfig};
use serde_json::json;

fn create_environment() -> TicTacToeEnvironment {
    let player_ids = vec![0, 1];
    let mut player_names = HashMap::new();
    player_names.insert(0, "Alice".to_string());
    player_names.insert(1, "Bob".to_string());

    TicTacToeEnvironment::new("match-ttt", player_ids, player_names)
        .expect("should create environment")
}

#[test]
fn test_environment_metadata() {
    let env = create_environment();
    assert_eq!(env.environment_type(), "tic_tac_toe");
    assert_eq!(env.display_name(), "Tic-Tac-Toe");
    assert_eq!(env.min_players(), 2);
    assert_eq!(env.max_players(), 2);
}

#[test]
fn test_player_ids() {
    let env = create_environment();
    assert_eq!(env.player_ids(), vec!["0", "1"]);
}

#[test]
fn test_initial_turn_info() {
    let env = create_environment();
    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.turn_number, 1);
    assert_eq!(info.phase, "playing");
    assert!(!info.is_terminal);
    assert_eq!(info.active_players, vec!["0"]);
}

#[test]
fn test_legal_actions_for_active_player() {
    let env = create_environment();
    let actions = env.legal_actions("0").expect("legal actions");
    let arr = actions.as_array().expect("actions should be an array");
    // 9 cells initially available
    assert_eq!(arr.len(), 9);
    // Each action has row and col fields
    assert!(arr[0]["row"].is_number());
    assert!(arr[0]["col"].is_number());
}

#[test]
fn test_state_for_player() {
    let env = create_environment();
    let state = env.state_for_player("0").expect("state");
    assert_eq!(state["turn"], json!(1));
    assert_eq!(state["phase"], json!("playing"));
    assert_eq!(state["current_player"], json!(0));
}

#[test]
fn test_full_state() {
    let env = create_environment();
    let state = env.full_state().expect("full state");
    assert_eq!(state["phase"], json!("playing"));
    assert_eq!(state["current_player"], json!(0));
}

#[test]
fn test_apply_action_advances_turn() {
    let mut env = create_environment();
    let result = env.apply_action("0", &json!({"action_type": "move", "row": 1, "col": 1}));
    assert!(result.is_ok());

    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.turn_number, 2);
    assert_eq!(info.active_players, vec!["1"]);
}

#[test]
fn test_wrong_player_cannot_move() {
    let mut env = create_environment();
    let result = env.apply_action("1", &json!({"action_type": "move", "row": 0, "col": 0}));
    assert!(result.is_err());
}

#[test]
fn test_invalid_action_rejected() {
    let mut env = create_environment();
    let result = env.apply_action("0", &json!({"not_valid": true}));
    assert!(result.is_err());
}

#[test]
fn test_cell_occupied_rejected() {
    let mut env = create_environment();
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 0}))
        .expect("first move");
    let result = env.apply_action("1", &json!({"action_type": "move", "row": 0, "col": 0}));
    assert!(result.is_err());
}

#[test]
fn test_win_produces_rankings() {
    let mut env = create_environment();
    // Player 0 (X) wins via top row
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 0}))
        .unwrap();
    env.apply_action("1", &json!({"action_type": "move", "row": 1, "col": 0}))
        .unwrap();
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 1}))
        .unwrap();
    env.apply_action("1", &json!({"action_type": "move", "row": 1, "col": 1}))
        .unwrap();
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 2}))
        .unwrap();

    assert!(env.is_terminal());

    let info = env.turn_info().expect("turn_info");
    assert!(info.is_terminal);
    assert!(info.active_players.is_empty());

    let rankings = env.rankings().expect("rankings for terminal game");
    assert_eq!(rankings.len(), 2);
    assert_eq!(rankings[0].player_id, "0");
    assert_eq!(rankings[0].rank, 1);
    assert_eq!(rankings[1].player_id, "1");
    assert_eq!(rankings[1].rank, 2);
}

#[test]
fn test_draw_produces_tied_rankings() {
    let mut env = create_environment();
    // Play a draw:
    // x o x
    // x x o
    // o x o
    let moves = [
        ("0", 0, 0), // X
        ("1", 0, 1), // O
        ("0", 0, 2), // X
        ("1", 1, 2), // O
        ("0", 1, 0), // X
        ("1", 2, 0), // O
        ("0", 1, 1), // X
        ("1", 2, 2), // O
        ("0", 2, 1), // X
    ];
    for (player, row, col) in moves {
        env.apply_action(
            player,
            &json!({"action_type": "move", "row": row, "col": col}),
        )
        .unwrap();
    }

    assert!(env.is_terminal());
    let rankings = env.rankings().expect("rankings for draw");
    assert_eq!(rankings.len(), 2);
    assert!(rankings.iter().all(|r| r.rank == 1));
}

#[test]
fn test_action_after_terminal_rejected() {
    let mut env = create_environment();
    // Quick win for player 0
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 0}))
        .unwrap();
    env.apply_action("1", &json!({"action_type": "move", "row": 1, "col": 0}))
        .unwrap();
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 1}))
        .unwrap();
    env.apply_action("1", &json!({"action_type": "move", "row": 1, "col": 1}))
        .unwrap();
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 2}))
        .unwrap();

    let result = env.apply_action("1", &json!({"action_type": "move", "row": 2, "col": 2}));
    assert!(result.is_err());
}

#[test]
fn test_legal_actions_empty_after_terminal() {
    let mut env = create_environment();
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 0}))
        .unwrap();
    env.apply_action("1", &json!({"action_type": "move", "row": 1, "col": 0}))
        .unwrap();
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 1}))
        .unwrap();
    env.apply_action("1", &json!({"action_type": "move", "row": 1, "col": 1}))
        .unwrap();
    env.apply_action("0", &json!({"action_type": "move", "row": 0, "col": 2}))
        .unwrap();

    let actions = env.legal_actions("0").expect("legal actions");
    assert_eq!(actions.as_array().unwrap().len(), 0);
}

#[test]
fn test_rankings_none_before_terminal() {
    let env = create_environment();
    assert!(env.rankings().is_none());
}

#[test]
fn test_unknown_player_rejected() {
    let env = create_environment();
    let result = env.state_for_player("42");
    assert!(result.is_err());
}

#[test]
fn test_rules_markdown_not_empty() {
    let env = create_environment();
    assert!(!env.rules_markdown().is_empty());
}

#[test]
fn test_requires_exactly_two_players() {
    let result = TicTacToeEnvironment::new(
        "match-err",
        vec![0, 1, 2],
        HashMap::from([
            (0, "A".to_string()),
            (1, "B".to_string()),
            (2, "C".to_string()),
        ]),
    );
    assert!(result.is_err());
}

#[test]
fn test_registry_create_tic_tac_toe_smoke() {
    let registry = EnvironmentRegistry::with_defaults();
    let environments = registry.available_environments();
    assert!(environments.contains(&"tic_tac_toe".to_string()));

    let config = EnvironmentConfig {
        player_count: 2,
        seed: 0,
        extra: HashMap::new(),
        match_id: Some("match-reg".to_string()),
        player_ids: Some(vec!["10".to_string(), "20".to_string()]),
        player_names: Some(HashMap::from([
            ("10".to_string(), "Alpha".to_string()),
            ("20".to_string(), "Bravo".to_string()),
        ])),
    };

    let env = registry
        .create("tic_tac_toe", &config)
        .expect("should create tic_tac_toe");
    assert_eq!(env.environment_type(), "tic_tac_toe");
    assert_eq!(env.player_ids(), vec!["10", "20"]);
}
