//! Integration tests for VibeCheckEnvironment

use environment_engine::registry::EnvironmentRegistry;
use environment_engine::vibe_check::VibeCheckEnvironment;
use environment_engine::{Environment, EnvironmentConfig};
use serde_json::json;

const SEED: u64 = 42;

fn create_environment(player_count: usize) -> VibeCheckEnvironment {
    VibeCheckEnvironment::new(player_count, SEED).expect("should create environment")
}

#[test]
fn test_environment_metadata() {
    let env = create_environment(4);
    assert_eq!(env.environment_type(), "vibe_check");
    assert_eq!(env.display_name(), "Vibe Check");
    assert_eq!(env.min_players(), 4);
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
fn test_player_ids_6_players() {
    let env = create_environment(6);
    let ids = env.player_ids();
    assert_eq!(ids.len(), 6);
    assert_eq!(ids, vec!["0", "1", "2", "3", "4", "5"]);
}

#[test]
fn test_initial_turn_info() {
    let env = create_environment(4);
    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.turn_number, 1);
    assert_eq!(info.phase, "clue_phase");
    assert!(!info.is_terminal);
    // Cluegiver is the only active player in CluePhase
    assert_eq!(info.active_players.len(), 1);
    assert_eq!(info.active_players[0], "0");
}

#[test]
fn test_state_for_player_filters_target() {
    let env = create_environment(4);

    // Cluegiver (player 0) should see target
    let state_p0 = env.state_for_player("0").expect("state for player 0");
    assert!(
        state_p0["target"].is_object(),
        "cluegiver should see target"
    );

    // Non-cluegiver teammate should not see target
    let state_p1 = env.state_for_player("1").expect("state for player 1");
    assert!(
        state_p1["target"].is_null(),
        "non-cluegiver should not see target"
    );

    // Opposing team should not see target
    let state_p2 = env.state_for_player("2").expect("state for player 2");
    assert!(
        state_p2["target"].is_null(),
        "opposing team should not see target"
    );
}

#[test]
fn test_full_state_shows_target() {
    let env = create_environment(4);
    let full = env.full_state().expect("full state");
    assert!(full["target"].is_object(), "full state should show target");
    assert!(
        full["spectrum"].is_object(),
        "full state should show spectrum"
    );
}

#[test]
fn test_legal_actions_cluegiver() {
    let env = create_environment(4);

    // Cluegiver (player 0) should have actions
    let actions = env.legal_actions("0").expect("legal actions for p0");
    let actions_arr = actions.as_array().unwrap();
    assert!(!actions_arr.is_empty(), "cluegiver should have actions");
    let has_give_clue = actions_arr
        .iter()
        .any(|a| a["action_type"].as_str() == Some("give_clue"));
    assert!(has_give_clue, "give_clue should be a legal action");
}

#[test]
fn test_legal_actions_non_active_player() {
    let env = create_environment(4);

    // Player 1 is not the cluegiver in CluePhase
    let actions = env.legal_actions("1").expect("legal actions for p1");
    let actions_arr = actions.as_array().unwrap();
    assert!(
        actions_arr.is_empty(),
        "non-cluegiver should have no actions in CluePhase"
    );
}

#[test]
fn test_apply_give_clue_action() {
    let mut env = create_environment(4);
    let action = json!({"action_type": "give_clue", "clue": "warm"});
    let result = env.apply_action("0", &action);
    assert!(result.is_ok(), "give_clue should succeed");

    // After clue, should be in guess_phase
    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.phase, "guess_phase");
}

#[test]
fn test_apply_action_wrong_player_returns_error() {
    let mut env = create_environment(4);
    // Player 1 tries to give clue on player 0's cluegiver turn
    let action = json!({"action_type": "give_clue", "clue": "test"});
    let result = env.apply_action("1", &action);
    assert!(result.is_err());
}

#[test]
fn test_apply_action_after_terminal_returns_error() {
    let mut env = create_environment(4);
    play_to_completion(&mut env);

    assert!(env.is_terminal());
    let action = json!({"action_type": "give_clue", "clue": "test"});
    let result = env.apply_action("0", &action);
    assert!(result.is_err());
    match result {
        Err(environment_engine::EnvironmentError::AlreadyTerminated) => {}
        other => panic!("expected AlreadyTerminated, got: {other:?}"),
    }
}

#[test]
fn test_rankings_none_while_in_progress() {
    let env = create_environment(4);
    assert!(env.rankings().is_none());
}

#[test]
fn test_rankings_team_winner() {
    let mut env = create_environment(4);
    play_to_completion(&mut env);

    assert!(env.is_terminal());
    let rankings = env.rankings().expect("should have rankings");
    assert_eq!(rankings.len(), 4);

    // All rank-1 players should be on the same team, all rank-2 on the other
    let rank1_players: Vec<&str> = rankings
        .iter()
        .filter(|r| r.rank == 1)
        .map(|r| r.player_id.as_str())
        .collect();
    let rank2_players: Vec<&str> = rankings
        .iter()
        .filter(|r| r.rank == 2)
        .map(|r| r.player_id.as_str())
        .collect();

    assert_eq!(rank1_players.len(), 2, "winning team should have 2 members");
    assert_eq!(rank2_players.len(), 2, "losing team should have 2 members");

    // Winning team is either [0,1] or [2,3]
    let is_team0_winner = rank1_players.contains(&"0") && rank1_players.contains(&"1");
    let is_team1_winner = rank1_players.contains(&"2") && rank1_players.contains(&"3");
    assert!(
        is_team0_winner || is_team1_winner,
        "winning team should be a complete team"
    );
}

#[test]
fn test_rankings_not_terminal() {
    let env = create_environment(4);
    assert!(env.rankings().is_none());
}

#[test]
fn test_rules_markdown_not_empty() {
    let env = create_environment(4);
    let rules = env.rules_markdown();
    assert!(!rules.is_empty());
}

#[test]
fn test_invalid_player_count_too_few() {
    let result = VibeCheckEnvironment::new(2, SEED);
    assert!(result.is_err(), "2 players should fail (min 4)");
}

#[test]
fn test_invalid_player_count_odd() {
    let result = VibeCheckEnvironment::new(5, SEED);
    assert!(result.is_err(), "odd player count should fail");
}

#[test]
fn test_invalid_player_count_too_many() {
    let result = VibeCheckEnvironment::new(8, SEED);
    assert!(result.is_err(), "8 players should fail (max 6)");
}

#[test]
fn test_unknown_player_parse() {
    let env = create_environment(4);
    let result = env.state_for_player("not_a_number");
    assert!(result.is_err());
}

#[test]
fn test_registry_create_vibe_check() {
    let registry = EnvironmentRegistry::with_defaults();
    let environments = registry.available_environments();
    assert!(environments.contains(&"vibe_check".to_string()));

    let config = EnvironmentConfig {
        player_count: 4,
        seed: 123,
        extra: Default::default(),
        ..Default::default()
    };
    let env = registry
        .create("vibe_check", &config)
        .expect("should create vibe_check");
    assert_eq!(env.environment_type(), "vibe_check");
    assert_eq!(env.player_ids().len(), 4);
}

#[test]
fn test_turn_info_guess_phase() {
    let mut env = create_environment(4);
    let action = json!({"action_type": "give_clue", "clue": "warm"});
    env.apply_action("0", &action).unwrap();

    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.phase, "guess_phase");
    // Guesser is player 1 (team 0, excluding cluegiver 0)
    assert_eq!(info.active_players, vec!["1"]);
}

#[test]
fn test_turn_info_steal_phase() {
    let mut env = create_environment(4);
    env.apply_action("0", &json!({"action_type": "give_clue", "clue": "warm"}))
        .unwrap();
    env.apply_action(
        "1",
        &json!({"action_type": "submit_guess", "position": 0.5}),
    )
    .unwrap();

    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.phase, "steal_phase");
    // Stealing team is team 1: players 2, 3
    assert!(info.active_players.contains(&"2".to_string()));
    assert!(info.active_players.contains(&"3".to_string()));
}

#[test]
fn test_legal_actions_terminal() {
    let mut env = create_environment(4);
    play_to_completion(&mut env);

    assert!(env.is_terminal());
    for pid in ["0", "1", "2", "3"] {
        let actions = env.legal_actions(pid).expect("legal_actions");
        assert_eq!(actions, json!([]));
    }
}

#[test]
fn test_full_round_advances_to_next_round() {
    let mut env = create_environment(4);

    // Round 1: Clue → Guess → Steal (all team members must submit)
    env.apply_action("0", &json!({"action_type": "give_clue", "clue": "warm"}))
        .unwrap();
    // Player 1 is the only guesser on team 0 (player 0 is cluegiver)
    env.apply_action(
        "1",
        &json!({"action_type": "submit_guess", "position": 0.5}),
    )
    .unwrap();
    // Both stealers on team 1 must submit
    env.apply_action(
        "2",
        &json!({"action_type": "submit_steal_guess", "direction": "right"}),
    )
    .unwrap();
    env.apply_action(
        "3",
        &json!({"action_type": "submit_steal_guess", "direction": "right"}),
    )
    .unwrap();

    if !env.is_terminal() {
        let info = env.turn_info().expect("turn_info");
        assert_eq!(info.turn_number, 2);
        assert_eq!(info.phase, "clue_phase");
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────

/// Play a 4-player Vibe Check to completion using a simple strategy:
/// cluegiver gives "test", guesser guesses at target position, steal misses.
fn play_to_completion(env: &mut VibeCheckEnvironment) {
    let max_rounds = 200;
    for _ in 0..max_rounds {
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

        // Get full state to read target position for bullseye strategy
        let full = env.full_state().unwrap();

        let action = match info.phase.as_str() {
            "clue_phase" => json!({"action_type": "give_clue", "clue": "test"}),
            "guess_phase" => {
                // Guess at target for bullseye
                let target_pos = full["target"]["position"].as_f64().unwrap_or(0.5);
                json!({"action_type": "submit_guess", "position": target_pos})
            }
            "steal_phase" => {
                // Steal with guaranteed wrong direction
                let target_pos = full["target"]["position"].as_f64().unwrap_or(0.5);
                let wrong_dir = if target_pos >= 0.5 { "left" } else { "right" };
                json!({"action_type": "submit_steal_guess", "direction": wrong_dir})
            }
            _ => break,
        };

        let result = env.apply_action(player_id, &action);
        if result.is_err() {
            // Fallback: try first available action
            if let Some(first_action) = actions.first() {
                let _ = env.apply_action(player_id, first_action);
            }
        }
    }
}
