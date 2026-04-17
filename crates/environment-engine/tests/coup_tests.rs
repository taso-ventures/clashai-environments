//! Integration tests for CoupEnvironment

use environment_engine::coup::CoupEnvironment;
use environment_engine::registry::EnvironmentRegistry;
use environment_engine::{Environment, EnvironmentConfig};
use serde_json::json;

const SEED: u64 = 42;

fn create_environment(player_count: usize) -> CoupEnvironment {
    CoupEnvironment::new(player_count, SEED).expect("should create environment")
}

#[test]
fn test_environment_metadata() {
    let env = create_environment(4);
    assert_eq!(env.environment_type(), "coup");
    assert_eq!(env.display_name(), "Coup");
    assert_eq!(env.min_players(), 2);
    assert_eq!(env.max_players(), 6);
}

#[test]
fn test_player_ids() {
    let env = create_environment(4);
    let ids = env.player_ids();
    assert_eq!(ids.len(), 4);
    assert_eq!(ids, vec!["0", "1", "2", "3"]);
}

#[test]
fn test_initial_turn_info() {
    let env = create_environment(3);
    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.turn_number, 1);
    assert_eq!(info.phase, "awaiting_action");
    assert!(!info.is_terminal);
    assert_eq!(info.active_players.len(), 1);
    assert_eq!(info.active_players[0], "0");
}

#[test]
fn test_state_for_player_filters_hidden_info() {
    let env = create_environment(3);
    let state_p0 = env.state_for_player("0").expect("state for player 0");
    let state_p1 = env.state_for_player("1").expect("state for player 1");

    // Player 0's view should show their own cards with real roles
    let p0_cards = &state_p0["players"]["0"]["cards"];
    for card in p0_cards.as_array().unwrap() {
        assert_ne!(card["role"].as_str().unwrap(), "unknown");
    }

    // Player 0's view of player 1's unrevealed cards should be "unknown"
    let p1_cards_in_p0_view = &state_p0["players"]["1"]["cards"];
    for card in p1_cards_in_p0_view.as_array().unwrap() {
        if !card["revealed"].as_bool().unwrap() {
            assert_eq!(card["role"].as_str().unwrap(), "unknown");
        }
    }

    // Player 1's own cards should have real roles
    let p1_own_cards = &state_p1["players"]["1"]["cards"];
    for card in p1_own_cards.as_array().unwrap() {
        assert_ne!(card["role"].as_str().unwrap(), "unknown");
    }
}

#[test]
fn test_full_state_shows_all() {
    let env = create_environment(2);
    let full = env.full_state().expect("full state");
    // Both players' cards should have real roles
    for pid in ["0", "1"] {
        let cards = &full["players"][pid]["cards"];
        for card in cards.as_array().unwrap() {
            assert_ne!(card["role"].as_str().unwrap(), "unknown");
        }
    }
}

#[test]
fn test_legal_actions_active_player() {
    let env = create_environment(2);
    let actions = env.legal_actions("0").expect("legal actions for p0");
    let actions_arr = actions.as_array().unwrap();
    assert!(!actions_arr.is_empty(), "active player should have actions");

    // Income should always be available on the first turn
    let has_income = actions_arr
        .iter()
        .any(|a| a["action_type"].as_str() == Some("income"));
    assert!(has_income, "income should be a legal action");
}

#[test]
fn test_legal_actions_inactive_player() {
    let env = create_environment(2);
    // Player 1 is not the active player on turn 1
    let actions = env.legal_actions("1").expect("legal actions for p1");
    let actions_arr = actions.as_array().unwrap();
    assert!(
        actions_arr.is_empty(),
        "inactive player should have no actions"
    );
}

#[test]
fn test_apply_income_action() {
    let mut env = create_environment(2);
    let action = json!({"action_type": "income"});
    let result = env.apply_action("0", &action);
    assert!(result.is_ok(), "income should succeed");

    // After income, it should be player 1's turn
    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.turn_number, 2);
    assert_eq!(info.active_players[0], "1");
}

#[test]
fn test_apply_invalid_action_returns_error() {
    let mut env = create_environment(2);
    // Player 1 tries to act on player 0's turn
    let action = json!({"action_type": "income"});
    let result = env.apply_action("1", &action);
    assert!(result.is_err());
}

#[test]
fn test_apply_action_after_terminal_returns_error() {
    let mut env = create_environment(2);

    // Play until terminal by exchanging incomes and coups
    play_to_completion(&mut env);

    assert!(env.is_terminal());
    let action = json!({"action_type": "income"});
    let result = env.apply_action("0", &action);
    assert!(result.is_err());
    match result {
        Err(environment_engine::EnvironmentError::AlreadyTerminated) => {}
        other => panic!("expected AlreadyTerminated, got: {other:?}"),
    }
}

#[test]
fn test_rankings_none_while_in_progress() {
    let env = create_environment(2);
    assert!(env.rankings().is_none());
}

#[test]
fn test_rankings_after_completion() {
    let mut env = create_environment(2);
    play_to_completion(&mut env);

    assert!(env.is_terminal());
    let rankings = env.rankings().expect("should have rankings");
    assert_eq!(rankings.len(), 2);
    assert_eq!(rankings[0].rank, 1);
    assert_eq!(rankings[1].rank, 2);
}

#[test]
fn test_rules_markdown_not_empty() {
    let env = create_environment(2);
    let rules = env.rules_markdown();
    assert!(!rules.is_empty());
    assert!(rules.contains("Coup"));
}

#[test]
fn test_invalid_player_count() {
    // 0 players should fail
    // 8+ players should fail (not enough cards: 15 cards, 2 per player = max 7)
    let result = CoupEnvironment::new(8, SEED);
    assert!(result.is_err(), "8 players should exceed deck capacity");
}

#[test]
fn test_unknown_player_parse() {
    let env = create_environment(2);
    let result = env.state_for_player("not_a_number");
    assert!(result.is_err());
}

#[test]
fn test_registry_create_coup() {
    let registry = EnvironmentRegistry::with_defaults();
    let environments = registry.available_environments();
    assert!(environments.contains(&"coup".to_string()));

    let config = EnvironmentConfig {
        player_count: 4,
        seed: 123,
        extra: Default::default(),
        ..Default::default()
    };
    let env = registry
        .create("coup", &config)
        .expect("should create coup");
    assert_eq!(env.environment_type(), "coup");
    assert_eq!(env.player_ids().len(), 4);
}

#[test]
fn test_registry_unknown_environment() {
    let registry = EnvironmentRegistry::with_defaults();
    let config = EnvironmentConfig {
        player_count: 2,
        seed: 0,
        extra: Default::default(),
        ..Default::default()
    };
    let result = registry.create("unknown_env", &config);
    assert!(result.is_err());
}

// ─── Helpers ──────────────────────────────────────────────────────────

/// Play a 2-player environment to completion using a simple strategy:
/// alternate income until 7 coins, then coup.
fn play_to_completion(env: &mut CoupEnvironment) {
    let max_turns = 200;
    for _ in 0..max_turns {
        if env.is_terminal() {
            return;
        }

        let info = env.turn_info().expect("turn_info");
        if info.active_players.is_empty() {
            break;
        }

        let player_id = &info.active_players[0];
        let actions_val = env.legal_actions(player_id).unwrap();
        let actions = actions_val.as_array().unwrap();
        if actions.is_empty() {
            break;
        }

        // Strategy: prefer coup > income > first available
        let action = actions
            .iter()
            .find(|a| a["action_type"].as_str() == Some("coup"))
            .or_else(|| {
                actions
                    .iter()
                    .find(|a| a["action_type"].as_str() == Some("income"))
            })
            .unwrap_or(&actions[0]);

        let result = env.apply_action(player_id, action);
        if result.is_err() {
            // If action failed, try first available
            let _ = env.apply_action(player_id, &actions[0]);
        }
    }
}
