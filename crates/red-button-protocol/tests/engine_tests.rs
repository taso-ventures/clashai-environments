//! Unit tests for the Red Button game engine.
//!
//! Covers:
//! - Turn alternation correctness
//! - Role-gated legal actions
//! - Message length / empty validation
//! - Terminal transitions (button_pressed, max_turns)
//! - Config defaults

use std::collections::HashMap;

use red_button_protocol::engine::EngineError;
use red_button_protocol::{
    RedButtonAction, RedButtonConfig, RedButtonGame, RedButtonRole, SpectatorEvent, TerminalReason,
    TurnActor,
};

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

fn default_config() -> RedButtonConfig {
    RedButtonConfig::default()
}

fn small_config(max_turns: u32) -> RedButtonConfig {
    RedButtonConfig {
        max_turns,
        ..Default::default()
    }
}

fn names() -> HashMap<i32, String> {
    [
        (0, "Persuader Agent".to_string()),
        (1, "Resistor Agent".to_string()),
    ]
    .into_iter()
    .collect()
}

fn new_game(config: RedButtonConfig) -> RedButtonGame {
    RedButtonGame::new("test-match", vec![0, 1], names(), config).unwrap()
}

fn speak(msg: &str) -> RedButtonAction {
    RedButtonAction::Speak {
        message: msg.to_string(),
    }
}

fn respond(msg: &str) -> RedButtonAction {
    RedButtonAction::RespondToOtherAgent {
        message: msg.to_string(),
    }
}

// -----------------------------------------------------------------------
// Config defaults
// -----------------------------------------------------------------------

#[test]
fn test_default_config_values() {
    let cfg = RedButtonConfig::default();
    assert_eq!(cfg.max_turns, 200);
    assert_eq!(cfg.per_turn_timeout_ms, 30_000);
    assert_eq!(cfg.max_message_chars, 500);
    assert!(!cfg.allow_empty_speak);
    assert!(cfg.publish_reasoning_live);
    assert!(cfg.archive_reasoning);
    assert!(cfg.raw_reasoning_enabled);
}

// -----------------------------------------------------------------------
// Setup validation
// -----------------------------------------------------------------------

#[test]
fn test_requires_exactly_two_players() {
    let cfg = default_config();
    assert!(RedButtonGame::new("m", vec![0], names(), cfg.clone()).is_err());
    assert!(RedButtonGame::new("m", vec![0, 1, 2], names(), cfg).is_err());
}

#[test]
fn test_max_turns_zero_rejected() {
    let cfg = RedButtonConfig {
        max_turns: 0,
        ..Default::default()
    };
    assert!(RedButtonGame::new("m", vec![0, 1], names(), cfg).is_err());
}

// -----------------------------------------------------------------------
// Role assignment
// -----------------------------------------------------------------------

#[test]
fn test_role_assignment_lower_id_is_persuader() {
    let game = new_game(default_config());
    let roles = game.player_roles();
    assert_eq!(*roles.get(&0).unwrap(), RedButtonRole::Persuader);
    assert_eq!(*roles.get(&1).unwrap(), RedButtonRole::Resistor);
}

// -----------------------------------------------------------------------
// Initial state
// -----------------------------------------------------------------------

#[test]
fn test_initial_state() {
    let game = new_game(default_config());
    let state = game.full_state();
    assert!(!state.is_terminal);
    assert!(state.conversation_history.is_empty());
    assert!(state.most_recent_message.is_none());
    assert!(!state.button_pressed);
    assert_eq!(state.turn_info.round, 1);
    assert_eq!(state.turn_info.actor, TurnActor::Persuader);
}

// -----------------------------------------------------------------------
// Legal actions
// -----------------------------------------------------------------------

#[test]
fn test_initial_legal_actions() {
    let game = new_game(default_config());
    // Player 0 = Persuader — should have exactly one legal action (Speak).
    let p_actions = game.legal_actions(0);
    assert_eq!(p_actions.len(), 1);
    assert!(matches!(p_actions[0], RedButtonAction::Speak { .. }));

    // Player 1 = Resistor — out of turn, no legal actions.
    let r_actions = game.legal_actions(1);
    assert!(r_actions.is_empty());
}

#[test]
fn test_resistor_legal_actions_after_persuader_speaks() {
    let mut game = new_game(default_config());
    game.apply_action(0, &speak("Speak now")).unwrap();

    // Now it's the Resistor's turn.
    let r_actions = game.legal_actions(1);
    assert_eq!(r_actions.len(), 3);
    let types: Vec<&str> = r_actions
        .iter()
        .map(|a| match a {
            RedButtonAction::IgnoreOtherAgent => "ignore",
            RedButtonAction::RespondToOtherAgent { .. } => "respond",
            RedButtonAction::PressButton => "press",
            RedButtonAction::Speak { .. } => "speak",
        })
        .collect();
    assert!(types.contains(&"ignore"));
    assert!(types.contains(&"respond"));
    assert!(types.contains(&"press"));

    // Persuader is out of turn.
    assert!(game.legal_actions(0).is_empty());
}

// -----------------------------------------------------------------------
// Turn alternation
// -----------------------------------------------------------------------

#[test]
fn test_turn_alternation_across_rounds() {
    let mut game = new_game(default_config());

    // Round 1: Persuader → Resistor
    let state = game.full_state();
    assert_eq!(state.turn_info.actor, TurnActor::Persuader);
    assert_eq!(state.turn_info.round, 1);

    game.apply_action(0, &speak("Come on")).unwrap();
    let state = game.full_state();
    assert_eq!(state.turn_info.actor, TurnActor::Resistor);
    assert_eq!(state.turn_info.round, 1);

    game.apply_action(1, &respond("No")).unwrap();
    let state = game.full_state();
    // Round 2 should start with Persuader.
    assert_eq!(state.turn_info.actor, TurnActor::Persuader);
    assert_eq!(state.turn_info.round, 2);
}

// -----------------------------------------------------------------------
// Message validation
// -----------------------------------------------------------------------

#[test]
fn test_empty_message_rejected() {
    let mut game = new_game(default_config());
    let result = game.apply_action(0, &speak(""));
    assert!(result.is_err());
}

#[test]
fn test_message_too_long_rejected() {
    let mut game = new_game(default_config()); // max 500 chars
    let long_msg = "x".repeat(501);
    let result = game.apply_action(0, &speak(&long_msg));
    assert!(result.is_err());
}

#[test]
fn test_message_at_max_length_accepted() {
    let mut game = new_game(default_config());
    let max_msg = "x".repeat(500);
    assert!(game.apply_action(0, &speak(&max_msg)).is_ok());
}

#[test]
fn test_empty_speak_allowed_when_configured() {
    let mut game = new_game(RedButtonConfig {
        allow_empty_speak: true,
        ..Default::default()
    });
    assert!(game.apply_action(0, &speak("")).is_ok());
}

// -----------------------------------------------------------------------
// Wrong actor enforcement
// -----------------------------------------------------------------------

#[test]
fn test_resistor_cannot_act_on_persuader_turn() {
    let mut game = new_game(default_config());
    let result = game.apply_action(1, &RedButtonAction::IgnoreOtherAgent);
    assert!(result.is_err());
}

#[test]
fn test_persuader_cannot_act_on_resistor_turn() {
    let mut game = new_game(default_config());
    game.apply_action(0, &speak("hello")).unwrap();
    let result = game.apply_action(0, &speak("again"));
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// Button press — Persuader wins
// -----------------------------------------------------------------------

#[test]
fn test_button_press_terminates_with_persuader_win() {
    let mut game = new_game(default_config());
    game.apply_action(0, &speak("Press it")).unwrap();
    let events = game.apply_action(1, &RedButtonAction::PressButton).unwrap();

    assert!(game.is_terminal());
    let state = game.full_state();
    assert!(state.button_pressed);
    assert_eq!(state.winner_role, Some(RedButtonRole::Persuader));
    assert_eq!(state.terminal_reason, Some(TerminalReason::ButtonPressed));

    // GameOver event must be present.
    let game_over = events.iter().find(|e| {
        matches!(
            e,
            SpectatorEvent::GameOver {
                winner_role: RedButtonRole::Persuader,
                ..
            }
        )
    });
    assert!(game_over.is_some(), "expected GameOver(Persuader) event");
}

#[test]
fn test_no_actions_after_terminal() {
    let mut game = new_game(default_config());
    game.apply_action(0, &speak("Press it")).unwrap();
    game.apply_action(1, &RedButtonAction::PressButton).unwrap();

    assert!(game.is_terminal());
    assert!(game.legal_actions(0).is_empty());
    assert!(game.legal_actions(1).is_empty());
    assert!(game.apply_action(0, &speak("again")).is_err());
}

// -----------------------------------------------------------------------
// Max turns — Resistor wins
// -----------------------------------------------------------------------

#[test]
fn test_max_turns_terminates_with_resistor_win() {
    let mut game = new_game(small_config(2)); // only 2 rounds

    // Round 1
    game.apply_action(0, &speak("Press it")).unwrap();
    game.apply_action(1, &RedButtonAction::IgnoreOtherAgent)
        .unwrap();
    // Round 2
    game.apply_action(0, &speak("Please")).unwrap();
    let events = game
        .apply_action(1, &RedButtonAction::IgnoreOtherAgent)
        .unwrap();

    assert!(game.is_terminal());
    let state = game.full_state();
    assert!(!state.button_pressed);
    assert_eq!(state.winner_role, Some(RedButtonRole::Resistor));
    assert_eq!(state.terminal_reason, Some(TerminalReason::MaxTurns));

    let game_over = events.iter().find(|e| {
        matches!(
            e,
            SpectatorEvent::GameOver {
                winner_role: RedButtonRole::Resistor,
                terminal_reason: TerminalReason::MaxTurns,
                ..
            }
        )
    });
    assert!(
        game_over.is_some(),
        "expected GameOver(Resistor, MaxTurns) event"
    );
}

#[test]
fn test_single_round_game() {
    let mut game = new_game(small_config(1));
    game.apply_action(0, &speak("Go")).unwrap();

    // After Resistor ignores in round 1, max_turns=1 means Resistor wins.
    game.apply_action(1, &RedButtonAction::IgnoreOtherAgent)
        .unwrap();
    assert!(game.is_terminal());
    assert_eq!(game.full_state().winner_role, Some(RedButtonRole::Resistor));
}

// -----------------------------------------------------------------------
// Conversation history
// -----------------------------------------------------------------------

#[test]
fn test_conversation_history_accumulates() {
    let mut game = new_game(default_config());

    game.apply_action(0, &speak("Round 1 persuade")).unwrap();
    game.apply_action(1, &respond("Round 1 refuse")).unwrap();
    game.apply_action(0, &speak("Round 2 persuade")).unwrap();
    game.apply_action(1, &RedButtonAction::IgnoreOtherAgent)
        .unwrap();

    let state = game.full_state();
    // IgnoreOtherAgent produces no spoken message; 3 spoken entries expected.
    assert_eq!(state.conversation_history.len(), 3);
    assert_eq!(
        state.most_recent_message.as_ref().unwrap().text,
        "Round 2 persuade"
    );
}

// -----------------------------------------------------------------------
// Spectator events
// -----------------------------------------------------------------------

#[test]
fn test_speak_emits_correct_events() {
    let mut game = new_game(default_config());
    let events = game.apply_action(0, &speak("Hello Resistor")).unwrap();

    assert!(events
        .iter()
        .any(|e| matches!(e, SpectatorEvent::MessageSpoken { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, SpectatorEvent::ActionTaken { .. })));
    assert!(events.iter().any(|e| matches!(
        e,
        SpectatorEvent::TurnAdvanced {
            actor: TurnActor::Resistor,
            ..
        }
    )));
}

#[test]
fn test_ignore_emits_turn_advanced_next_round() {
    let mut game = new_game(small_config(3));
    game.apply_action(0, &speak("Hi")).unwrap();
    let events = game
        .apply_action(1, &RedButtonAction::IgnoreOtherAgent)
        .unwrap();

    // TurnAdvanced to next round with Persuader.
    let advanced = events.iter().find(|e| {
        matches!(
            e,
            SpectatorEvent::TurnAdvanced {
                actor: TurnActor::Persuader,
                round: 2
            }
        )
    });
    assert!(advanced.is_some());
}

// -----------------------------------------------------------------------
// game_started_event
// -----------------------------------------------------------------------

#[test]
fn test_game_started_event() {
    let game = new_game(default_config());
    let event = game.game_started_event();
    match event {
        SpectatorEvent::GameStarted {
            players,
            config_summary,
        } => {
            assert_eq!(players.len(), 2);
            assert_eq!(config_summary.max_turns, 200);
            assert_eq!(config_summary.max_message_chars, 500);
        }
        _ => panic!("expected GameStarted event"),
    }
}

// -----------------------------------------------------------------------
// Winner
// -----------------------------------------------------------------------

#[test]
fn test_winner_is_persuader_after_button_press() {
    let mut game = new_game(default_config());
    game.apply_action(0, &speak("Press")).unwrap();
    game.apply_action(1, &RedButtonAction::PressButton).unwrap();

    assert_eq!(game.winner(), Some(0)); // player 0 = Persuader
}

#[test]
fn test_winner_is_resistor_after_max_turns() {
    let mut game = new_game(small_config(1));
    game.apply_action(0, &speak("Press")).unwrap();
    game.apply_action(1, &RedButtonAction::IgnoreOtherAgent)
        .unwrap();

    assert_eq!(game.winner(), Some(1)); // player 1 = Resistor
}

// -----------------------------------------------------------------------
// Regression: action-type legality per role.
// The engine must reject actions that the current actor's role is not
// allowed to submit, even if it is that actor's turn.
// -----------------------------------------------------------------------

#[test]
fn persuader_cannot_press_the_button() {
    let mut game = new_game(default_config());
    // On turn 1 it is the Persuader's (player 0) turn. Attempting to
    // press the button themselves must be rejected — only the Resistor
    // may end the match this way.
    let err = game
        .apply_action(0, &RedButtonAction::PressButton)
        .expect_err("persuader must not be able to press the button");
    assert!(matches!(err, EngineError::IllegalAction(_)));
    // Match still live — button not pressed.
    assert!(game.winner().is_none());
}

#[test]
fn resistor_cannot_speak() {
    let mut game = new_game(default_config());
    // Persuader speaks → resistor's turn.
    game.apply_action(0, &speak("please")).unwrap();
    // The Resistor's speaking action-type is `RespondToOtherAgent`, not
    // `Speak`. The engine must reject `Speak` from the Resistor.
    let err = game
        .apply_action(1, &speak("no"))
        .expect_err("resistor must not be able to `speak` — only `respond_to_other_agent`");
    assert!(matches!(err, EngineError::IllegalAction(_)));
}
