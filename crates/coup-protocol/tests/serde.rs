use chrono::Utc;
use coup_protocol::{
    ActionHistoryEntry, Card, CoupAction, CoupState, MatchStatusResponse, PendingAction,
    PlayerPublicInfo, PlayerState, Role, SpectatorEvent, SubmitActionRequest, SubmitActionResponse,
    TurnPhase,
};
use eval_runtime::{EnvironmentWinner, SequentialPhase, SequentialState};
use std::collections::HashMap;

#[test]
fn test_serde_round_trip_state_and_actions() {
    let mut players = HashMap::new();
    players.insert(
        0,
        PlayerState {
            coins: 2,
            cards: vec![
                Card {
                    role: Role::Duke,
                    revealed: false,
                },
                Card {
                    role: Role::Assassin,
                    revealed: true,
                },
            ],
            eliminated: false,
        },
    );

    let pending_action = PendingAction {
        id: 1,
        actor: 0,
        action: CoupAction::Tax,
        target: None,
        claimed_role: Some(Role::Duke),
        challenged_by: None,
        blocked_by: None,
        block_claimed_role: None,
        exchange_draw: vec![],
    };

    let state = CoupState {
        turn_number: 1,
        current_phase: TurnPhase::ChallengeWindow {
            waiting_on: vec![1, 2],
            deadline: Utc::now(),
        },
        active_player: 0,
        players,
        pending_action: Some(pending_action),
        action_history: vec![ActionHistoryEntry {
            turn: 1,
            actor: 0,
            action: CoupAction::Tax,
            outcome: "tax".to_string(),
            timestamp: Utc::now(),
        }],
        deck_count: 9,
    };

    let serialized = serde_json::to_string(&state).expect("serialize state");
    let deserialized: CoupState = serde_json::from_str(&serialized).expect("deserialize state");
    assert_eq!(state, deserialized);

    let action = CoupAction::Steal { target: 1 };
    let action_json = serde_json::to_string(&action).expect("serialize action");
    let action_round: CoupAction = serde_json::from_str(&action_json).expect("deserialize action");
    assert_eq!(action, action_round);
}

#[test]
fn test_serde_round_trip_events_and_requests() {
    let event = SpectatorEvent::GameStarted {
        players: vec![PlayerPublicInfo {
            player_id: 1,
            eliminated: false,
        }],
    };
    let serialized = serde_json::to_string(&event).expect("serialize event");
    let deserialized: SpectatorEvent =
        serde_json::from_str(&serialized).expect("deserialize event");
    let event_json = serde_json::to_value(&event).expect("event json");
    let round_json = serde_json::to_value(&deserialized).expect("event json");
    assert_eq!(event_json, round_json);

    let request = SubmitActionRequest {
        player_id: 2,
        action: CoupAction::Income,
    };
    let request_json = serde_json::to_string(&request).expect("serialize request");
    let request_round: SubmitActionRequest =
        serde_json::from_str(&request_json).expect("deserialize request");
    let request_json = serde_json::to_value(&request).expect("request json");
    let request_round_json = serde_json::to_value(&request_round).expect("request json");
    assert_eq!(request_json, request_round_json);

    let response = SubmitActionResponse {
        accepted: true,
        error: None,
    };
    let response_json = serde_json::to_string(&response).expect("serialize response");
    let response_round: SubmitActionResponse =
        serde_json::from_str(&response_json).expect("deserialize response");
    let response_json = serde_json::to_value(&response).expect("response json");
    let response_round_json = serde_json::to_value(&response_round).expect("response json");
    assert_eq!(response_json, response_round_json);

    let status = MatchStatusResponse {
        match_id: "match".to_string(),
        turn_number: 2,
        phase: "awaiting_action".to_string(),
        is_terminal: false,
        winner: Some(0),
    };
    let status_json = serde_json::to_string(&status).expect("serialize status");
    let status_round: MatchStatusResponse =
        serde_json::from_str(&status_json).expect("deserialize status");
    let status_json = serde_json::to_value(&status).expect("status json");
    let status_round_json = serde_json::to_value(&status_round).expect("status json");
    assert_eq!(status_json, status_round_json);
}

// ─── Regression: Coup GameOver produces EnvironmentWinner::Player ───

fn make_coup_game_over_state(winner: i32) -> CoupState {
    let mut players = HashMap::new();
    for id in 0..3 {
        players.insert(
            id,
            PlayerState {
                coins: 0,
                cards: vec![],
                eliminated: id != winner,
            },
        );
    }
    CoupState {
        turn_number: 5,
        current_phase: TurnPhase::GameOver { winner },
        active_player: winner,
        players,
        pending_action: None,
        action_history: vec![],
        deck_count: 0,
    }
}

#[test]
fn test_sequential_phase_game_over_single_winner() {
    let state = make_coup_game_over_state(0);

    assert_eq!(
        state.sequential_phase(),
        SequentialPhase::GameOver {
            winner: EnvironmentWinner::Player(0),
        }
    );
}

#[test]
fn test_sequential_phase_game_over_different_winner() {
    let state = make_coup_game_over_state(2);

    assert_eq!(
        state.sequential_phase(),
        SequentialPhase::GameOver {
            winner: EnvironmentWinner::Player(2),
        }
    );
}
