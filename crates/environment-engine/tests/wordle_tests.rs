//! Integration tests for WordleEnvironment

use std::collections::HashMap;

use environment_engine::registry::EnvironmentRegistry;
use environment_engine::wordle::WordleEnvironment;
use environment_engine::{Environment, EnvironmentConfig};
use serde_json::{json, Value};

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
    assert_eq!(arr.len(), 1);
    assert_eq!(
        arr[0]["action_type"],
        Value::String("send_message".to_string())
    );
}

#[test]
fn test_apply_lobby_messages_advances_to_guessing() {
    let mut env = create_environment();
    env.apply_action(
        "0",
        &json!({"action_type": "send_message", "message": "ready"}),
    )
    .expect("p0 message");
    env.apply_action(
        "1",
        &json!({"action_type": "send_message", "message": "ready"}),
    )
    .expect("p1 message");
    env.apply_action(
        "2",
        &json!({"action_type": "send_message", "message": "ready"}),
    )
    .expect("p2 message");

    let info = env.turn_info().expect("turn_info");
    assert_eq!(info.turn_number, 1);
    assert_eq!(info.phase, "guessing");
    assert_eq!(info.active_players, vec!["0", "1", "2"]);
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
        player_ids: Some(vec![11, 12, 13]),
        player_names: Some(HashMap::from([
            (11, "A".to_string()),
            (12, "B".to_string()),
            (13, "C".to_string()),
        ])),
    };

    let env = registry
        .create("wordle", &config)
        .expect("should create wordle");
    assert_eq!(env.environment_type(), "wordle");
    assert_eq!(env.player_ids(), vec!["11", "12", "13"]);
}
