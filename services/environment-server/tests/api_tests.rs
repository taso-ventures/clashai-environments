//! Integration tests for the unified environment server.
//!
//! These tests spin up the service on a random port using `spawn_app()` and
//! send real HTTP requests.  No mocking.

use std::net::SocketAddr;

use environment_server::{build_router, AppState};
use reqwest::StatusCode;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// -----------------------------------------------------------------------
// Test harness helpers
// -----------------------------------------------------------------------

async fn spawn_app() -> (SocketAddr, oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let state = AppState::new(format!("http://{addr}"), 64);
    let app = build_router(state);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("server error");
    });
    (addr, shutdown_tx)
}

async fn create_red_button_match(base_url: &str) -> String {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "environment_type": "red_button",
        "player_count": 2,
        "player_names": {"0": "PersuaderBot", "1": "ResistorBot"},
        "extra": {}
    });
    let resp = client
        .post(format!("{base_url}/matches"))
        .json(&body)
        .send()
        .await
        .expect("POST /matches");
    assert!(
        resp.status().is_success(),
        "create_match failed: {:?}",
        resp.status()
    );
    let json: serde_json::Value = resp.json().await.expect("json body");
    json.get("match_id")
        .and_then(|v| v.as_str())
        .expect("match_id in response")
        .to_string()
}

// -----------------------------------------------------------------------
// T1. Create a Red Button match
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_create_red_button_match_returns_match_id() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;
    assert!(!match_id.is_empty(), "match_id must not be empty");
}

#[tokio::test]
async fn test_create_red_button_match_returns_spectator_url() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "environment_type": "red_button",
        "player_count": 2,
    });
    let resp = client
        .post(format!("{base}/matches"))
        .json(&body)
        .send()
        .await
        .expect("POST /matches");
    let json: serde_json::Value = resp.json().await.expect("json");
    let spectator_url = json
        .get("spectator_url")
        .and_then(|v| v.as_str())
        .expect("spectator_url in response");
    assert!(
        spectator_url.contains("/viewer/red-button.html?matchId="),
        "spectator_url should point to the 3D viewer"
    );
}

// -----------------------------------------------------------------------
// T2. Get state
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_get_state_returns_initial_state() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/matches/{match_id}/state?player_id=0"))
        .send()
        .await
        .expect("GET /matches/{id}/state");
    assert_eq!(resp.status(), StatusCode::OK);

    let json: serde_json::Value = resp.json().await.expect("json");
    let state = json.get("state").expect("state field");

    // Round 1, Persuader's turn.
    assert_eq!(
        state["turn_info"]["round"].as_u64().unwrap_or(0),
        1,
        "initial round should be 1"
    );
    assert_eq!(
        state["turn_info"]["actor"].as_str().unwrap_or(""),
        "persuader",
        "initial actor should be persuader"
    );
    assert!(
        !state["is_terminal"].as_bool().unwrap_or(true),
        "match should not be terminal initially"
    );
    assert!(
        !state["button_pressed"].as_bool().unwrap_or(true),
        "button should not be pressed initially"
    );
    assert!(
        state["conversation_history"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "conversation history should be empty"
    );
}

#[tokio::test]
async fn test_get_state_without_player_id_returns_full_state() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "environment_type": "coup",
        "player_count": 2,
        "seed": 42
    });
    let resp = client
        .post(format!("{base}/matches"))
        .json(&body)
        .send()
        .await
        .expect("create coup match");
    assert_eq!(resp.status(), StatusCode::OK);
    let created: serde_json::Value = resp.json().await.expect("json");
    let match_id = created["match_id"].as_str().expect("match_id");

    let resp = client
        .get(format!("{base}/matches/{match_id}/state"))
        .send()
        .await
        .expect("GET full state");
    assert_eq!(resp.status(), StatusCode::OK);
    let full: serde_json::Value = resp.json().await.expect("json");
    let opponent_role = full["state"]["players"]["1"]["cards"][0]["role"]
        .as_str()
        .expect("opponent card role");
    assert_ne!(
        opponent_role, "unknown",
        "omitting player_id should return the authoritative full state"
    );
}

// -----------------------------------------------------------------------
// T3. Legal actions
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_initial_legal_actions_persuader() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{base}/matches/{match_id}/legal_actions?player_id=0"
        ))
        .send()
        .await
        .expect("GET legal_actions");
    let actions: serde_json::Value = resp.json().await.expect("json");
    let arr = actions.as_array().expect("array");
    assert_eq!(arr.len(), 1, "Persuader should have exactly 1 legal action");
    assert_eq!(
        arr[0]["action_type"].as_str().unwrap_or(""),
        "speak",
        "Persuader's legal action should be 'speak'"
    );
}

#[tokio::test]
async fn test_initial_legal_actions_resistor_empty_on_persuader_turn() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{base}/matches/{match_id}/legal_actions?player_id=1"
        ))
        .send()
        .await
        .expect("GET legal_actions for resistor");
    let actions: serde_json::Value = resp.json().await.expect("json");
    let arr = actions.as_array().expect("array");
    assert!(
        arr.is_empty(),
        "Resistor should have no legal actions on Persuader's turn"
    );
}

// -----------------------------------------------------------------------
// T4. Submit actions
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_persuader_speak_action_accepted() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;

    let client = reqwest::Client::new();
    let action = serde_json::json!({
        "player_id": 0,
        "action": {
            "action_type": "speak",
            "message": "You should press the button!"
        }
    });
    let resp = client
        .post(format!("{base}/matches/{match_id}/actions"))
        .json(&action)
        .send()
        .await
        .expect("POST actions");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("json");
    assert!(
        body["accepted"].as_bool().unwrap_or(false),
        "speak action should be accepted"
    );
}

#[tokio::test]
async fn test_wrong_actor_action_rejected() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;

    let client = reqwest::Client::new();
    // Try to submit Resistor action on Persuader's turn.
    let action = serde_json::json!({
        "player_id": 1,
        "action": { "action_type": "ignore_other_agent" }
    });
    let resp = client
        .post(format!("{base}/matches/{match_id}/actions"))
        .json(&action)
        .send()
        .await
        .expect("POST actions");
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "wrong actor should get 400"
    );
}

#[tokio::test]
async fn test_malformed_action_returns_400() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/matches/{match_id}/actions"))
        .json(&serde_json::json!({
            "player_id": "0",
            "action": { "message": "missing action_type" }
        }))
        .send()
        .await
        .expect("POST malformed action");
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "malformed model actions should be client errors"
    );
}

// -----------------------------------------------------------------------
// T5. Full match flow — Resistor wins by max_turns
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_full_match_resistor_wins_max_turns() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");

    // Create a match with max_turns=2 so it terminates quickly.
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "environment_type": "red_button",
        "player_count": 2,
        "extra": {
            "max_turns": 2,
            "per_turn_timeout_ms": 30000,
            "max_message_chars": 500,
            "allow_empty_speak": false,
            "persuader_system_prompt": "Press it",
            "resistor_system_prompt": "Don't press it",
            "publish_reasoning_live": true,
            "archive_reasoning": true,
            "raw_reasoning_enabled": true
        }
    });
    let resp = client
        .post(format!("{base}/matches"))
        .json(&body)
        .send()
        .await
        .expect("create match");
    let match_id = resp
        .json::<serde_json::Value>()
        .await
        .expect("json")
        .get("match_id")
        .and_then(|v| v.as_str())
        .expect("match_id")
        .to_string();

    // Play through 2 rounds: each round = Persuader speak + Resistor ignore.
    for _round in 0..2 {
        // Persuader speaks.
        let resp = client
            .post(format!("{base}/matches/{match_id}/actions"))
            .json(&serde_json::json!({
                "player_id": 0,
                "action": { "action_type": "speak", "message": "Press the button!" }
            }))
            .send()
            .await
            .expect("speak");
        assert!(resp.status().is_success());

        // Check if terminal after speak (shouldn't be yet).
        let state_resp = client
            .get(format!("{base}/matches/{match_id}/state?player_id=0"))
            .send()
            .await
            .expect("state");
        let state: serde_json::Value = state_resp.json().await.expect("json");
        if state["state"]["is_terminal"].as_bool() == Some(true) {
            break; // Shouldn't happen here, but defensive.
        }

        // Resistor ignores.
        let resp = client
            .post(format!("{base}/matches/{match_id}/actions"))
            .json(&serde_json::json!({
                "player_id": 1,
                "action": { "action_type": "ignore_other_agent" }
            }))
            .send()
            .await
            .expect("ignore");
        assert!(resp.status().is_success());
    }

    // After 2 rounds, match should be terminal with Resistor winning.
    let state_resp = client
        .get(format!("{base}/matches/{match_id}/state?player_id=0"))
        .send()
        .await
        .expect("final state");
    let final_state: serde_json::Value = state_resp.json().await.expect("json");
    let state = &final_state["state"];

    assert!(
        state["is_terminal"].as_bool().unwrap_or(false),
        "match should be terminal after max_turns"
    );
    assert_eq!(
        state["winner_role"].as_str().unwrap_or(""),
        "resistor",
        "Resistor should win when max_turns reached"
    );
    assert_eq!(
        state["terminal_reason"].as_str().unwrap_or(""),
        "max_turns",
        "terminal_reason should be max_turns"
    );
}

// -----------------------------------------------------------------------
// T6. Full match flow — Persuader wins by button press
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_full_match_persuader_wins_button_pressed() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;

    let client = reqwest::Client::new();

    // Persuader speaks.
    let resp = client
        .post(format!("{base}/matches/{match_id}/actions"))
        .json(&serde_json::json!({
            "player_id": 0,
            "action": { "action_type": "speak", "message": "Press the button!" }
        }))
        .send()
        .await
        .expect("speak");
    assert!(resp.status().is_success());

    // Resistor presses the button.
    let resp = client
        .post(format!("{base}/matches/{match_id}/actions"))
        .json(&serde_json::json!({
            "player_id": 1,
            "action": { "action_type": "press_button" }
        }))
        .send()
        .await
        .expect("press_button");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.expect("json");
    assert!(body["is_terminal"].as_bool() == Some(true));

    // Verify final state.
    let state_resp = client
        .get(format!("{base}/matches/{match_id}/state?player_id=0"))
        .send()
        .await
        .expect("final state");
    let final_state: serde_json::Value = state_resp.json().await.expect("json");
    let state = &final_state["state"];

    assert!(state["is_terminal"].as_bool().unwrap_or(false));
    assert!(state["button_pressed"].as_bool().unwrap_or(false));
    assert_eq!(state["winner_role"].as_str().unwrap_or(""), "persuader");
    assert_eq!(
        state["terminal_reason"].as_str().unwrap_or(""),
        "button_pressed"
    );
}

// -----------------------------------------------------------------------
// T7. Match status endpoint
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_match_status_endpoint() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/matches/{match_id}/status"))
        .send()
        .await
        .expect("GET status");
    assert_eq!(resp.status(), StatusCode::OK);

    let status: serde_json::Value = resp.json().await.expect("json");
    assert!(!status["is_terminal"].as_bool().unwrap_or(true));
    assert_eq!(
        status["environment_type"].as_str().unwrap_or(""),
        "red_button"
    );
}

// -----------------------------------------------------------------------
// T8. Reasoning endpoint
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_reasoning_endpoint_returns_204() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/matches/{match_id}/reasoning"))
        .json(&serde_json::json!({
            "player_id": 0,
            "reasoning": "I need to convince the Resistor by appealing to curiosity."
        }))
        .send()
        .await
        .expect("POST reasoning");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// -----------------------------------------------------------------------
// T9. Player names endpoint
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_player_names_endpoint() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "environment_type": "red_button",
        "player_count": 2,
        "player_names": {"0": "PersuaderAgent", "1": "ResistorAgent"},
    });
    let resp = client
        .post(format!("{base}/matches"))
        .json(&body)
        .send()
        .await
        .expect("create");
    let match_id: String = resp
        .json::<serde_json::Value>()
        .await
        .expect("json")
        .get("match_id")
        .and_then(|v| v.as_str())
        .expect("match_id")
        .to_string();

    let resp = client
        .get(format!("{base}/matches/{match_id}/player_names"))
        .send()
        .await
        .expect("player_names");
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.expect("json");
    let names = body.get("player_names").expect("player_names");
    assert_eq!(
        names.get("0").and_then(|v| v.as_str()),
        Some("PersuaderAgent"),
    );
    assert_eq!(
        names.get("1").and_then(|v| v.as_str()),
        Some("ResistorAgent"),
    );
}

// -----------------------------------------------------------------------
// T10. 404 for unknown match
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_unknown_match_returns_404() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/matches/NONEXISTENT/state?player_id=0"))
        .send()
        .await
        .expect("GET state");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// -----------------------------------------------------------------------
// T11. Health check
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_health_check() {
    let (addr, _shutdown) = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("GET /health");
    assert_eq!(resp.status(), StatusCode::OK);
}

// -----------------------------------------------------------------------
// T12. Actions rejected after terminal
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_actions_rejected_after_terminal() {
    let (addr, _shutdown) = spawn_app().await;
    let base = format!("http://{addr}");
    let match_id = create_red_button_match(&base).await;
    let client = reqwest::Client::new();

    // Quick terminal: Persuader speaks, Resistor presses button.
    client
        .post(format!("{base}/matches/{match_id}/actions"))
        .json(&serde_json::json!({
            "player_id": 0,
            "action": { "action_type": "speak", "message": "Press it" }
        }))
        .send()
        .await
        .expect("speak");
    client
        .post(format!("{base}/matches/{match_id}/actions"))
        .json(&serde_json::json!({
            "player_id": 1,
            "action": { "action_type": "press_button" }
        }))
        .send()
        .await
        .expect("press_button");

    // Now try another action — should be rejected.
    let resp = client
        .post(format!("{base}/matches/{match_id}/actions"))
        .json(&serde_json::json!({
            "player_id": 0,
            "action": { "action_type": "speak", "message": "Too late!" }
        }))
        .send()
        .await
        .expect("post-terminal action");
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "actions after terminal should be rejected"
    );
}
