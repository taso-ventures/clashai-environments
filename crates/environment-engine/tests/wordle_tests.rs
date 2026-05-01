//! Integration tests for WordleEnvironment

use std::collections::HashMap;

use environment_engine::registry::EnvironmentRegistry;
use environment_engine::wordle::WordleEnvironment;
use environment_engine::{Environment, EnvironmentConfig};
use serde_json::json;

fn create_environment() -> WordleEnvironment {
    let player_ids = vec![0, 1, 2];
    let mut player_names = HashMap::new();
    player_names.insert(0, "Alpha".to_string());
    player_names.insert(1, "Bravo".to_string());
    player_names.insert(2, "Charlie".to_string());

    WordleEnvironment::new(
        "match-123",
        player_ids,
        player_names,
        wordle_protocol::WordleConfig::default(),
        123,
    )
    .expect("should create environment")
}

#[test]
fn test_environment_metadata() {
    let env = create_environment();
    assert_eq!(env.environment_type(), "wordle");
    assert_eq!(env.display_name(), "Wordle");
    assert_eq!(env.min_players(), 3);
    assert_eq!(env.max_players(), 6);
}

#[test]
fn test_initial_turn_info_and_legal_actions() {
    let env = create_environment();
    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.turn_number, 0);
    assert_eq!(info.phase, "lobby");
    assert!(!info.is_terminal);
    assert_eq!(info.active_players, vec!["0", "1", "2"]);

    let actions = env.legal_actions("0").expect("legal actions");
    let arr = actions.as_array().expect("actions should be an array");
    // Lobby is non-blocking: every player's legal-action set contains
    // both send_message and guess so a silent player can't hang the match.
    assert_eq!(arr.len(), 2);
    let action_types: std::collections::HashSet<_> = arr
        .iter()
        .map(|a| a["action_type"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(action_types.contains("send_message"));
    assert!(action_types.contains("guess"));
}

#[test]
fn test_first_guess_advances_lobby_to_guessing() {
    let mut env = create_environment();
    env.apply_action(
        "0",
        &json!({"action_type": "send_message", "message": "ready"}),
    )
    .expect("p0 message");
    // Still in lobby — chat alone doesn't advance.
    assert_eq!(env.turn_info().unwrap().phase, "lobby");

    // Any player's first guess kicks the match into `guessing`, even if
    // the other two players never sent a lobby message.
    let valid_guess = json!({"action_type": "guess", "word": "crane"});
    env.apply_action("0", &valid_guess).expect("p0 guess");

    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.turn_number, 1);
    assert_eq!(info.phase, "guessing");
}

#[test]
fn test_registry_create_wordle_smoke() {
    let registry = EnvironmentRegistry::with_defaults();
    let environments = registry.available_environments();
    assert!(environments.contains(&"wordle".to_string()));

    let mut extra = HashMap::new();
    extra.insert("max_guesses".to_string(), json!(6));
    extra.insert("max_message_chars".to_string(), json!(200));

    let config = EnvironmentConfig {
        player_count: 3,
        seed: 99,
        extra,
        match_id: Some("match-registry".to_string()),
        player_ids: Some(vec!["11".to_string(), "12".to_string(), "13".to_string()]),
        player_names: Some(HashMap::from([
            ("11".to_string(), "A".to_string()),
            ("12".to_string(), "B".to_string()),
            ("13".to_string(), "C".to_string()),
        ])),
    };

    let env = registry
        .create("wordle", &config)
        .expect("should create wordle");
    assert_eq!(env.environment_type(), "wordle");
    assert_eq!(env.player_ids(), vec!["11", "12", "13"]);
}
