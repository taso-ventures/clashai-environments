//! Route handlers for the unified environment server.

use std::sync::{atomic::Ordering, Arc};

use axum::{
    extract::{ws::Message, Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use rand::RngCore;
use serde_json;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

use environment_engine::{EnvironmentConfig, EnvironmentError};
use unified_event_protocol::UnifiedEvent;

use crate::{
    push_event_capped, AppState, CreateMatchRequest, CreateMatchResponse, ErrorResponse,
    MatchInstance, MatchStatusResponse, StateQuery, SubmitActionRequest, SubmitReasoningRequest,
};

// =====================
// Helpers
// =====================

fn not_found(id: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("match '{id}' not found"),
        }),
    )
        .into_response()
}

fn internal(msg: impl ToString) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn bad_request(msg: impl ToString) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

// =====================
// POST /matches
// =====================

pub async fn create_match(
    State(state): State<AppState>,
    Json(request): Json<CreateMatchRequest>,
) -> Result<Json<CreateMatchResponse>, Response> {
    state.reap_completed_matches().await;

    let env_type = request.environment_type.clone();
    let player_count = request.player_count.unwrap_or(2);
    let seed = request
        .seed
        .unwrap_or_else(|| rand::thread_rng().next_u64());
    let match_id = request
        .match_id
        .clone()
        .unwrap_or_else(|| ulid::Ulid::new().to_string());

    let player_names = request.player_names.clone().unwrap_or_default();
    let player_ids = request.player_ids.clone();

    let config = EnvironmentConfig {
        player_count,
        seed,
        extra: request.extra.clone(),
        match_id: Some(match_id.clone()),
        player_ids: player_ids
            .or_else(|| Some((0..player_count).map(|id| id.to_string()).collect())),
        player_names: if player_names.is_empty() {
            None
        } else {
            Some(player_names.clone())
        },
    };

    let env = state
        .registry
        .create(&env_type, &config)
        .map_err(|e| bad_request(format!("failed to create '{env_type}' match: {e}")))?;

    // Build the initial GameStarted-equivalent event (rules / display name).
    let init_event = UnifiedEvent::match_start(
        &env_type,
        &match_id,
        0,
        serde_json::json!({
            "display_name": env.display_name(),
            "player_count": player_count,
            "player_names": player_names,
        }),
    );

    let (tx, _rx) = broadcast::channel::<serde_json::Value>(state.ws_capacity);
    let init_value = serde_json::to_value(&init_event).expect("UnifiedEvent always serializes");

    // Seed event log with match_start so spectators connecting later receive it.
    let event_log = RwLock::new(vec![init_value.clone()]);

    // Persist initial event + player names to Redis.
    // NOTE: Only the init event is stored here. Full event streaming to Redis
    // during live play is planned but not yet implemented; the in-memory
    // event_log handles spectator catchup in the meantime.
    if let Ok(json) = serde_json::to_string(&init_event) {
        state.redis_store_event(&match_id, &json).await;
    }
    state
        .redis_store_player_names(&match_id, &player_names)
        .await;

    // Broadcast (no receivers yet, silently discarded).
    let _ = tx.send(init_value);

    let instance = Arc::new(MatchInstance {
        environment: Arc::new(RwLock::new(env)),
        broadcaster: tx,
        player_names,
        environment_type: env_type.clone(),
        sequence: std::sync::atomic::AtomicU64::new(1),
        event_log,
    });

    state
        .matches
        .write()
        .await
        .insert(match_id.clone(), instance);

    let viewer_page = match env_type.as_str() {
        "coup" => Some("coup.html"),
        "vibe_check" => Some("vibe-check.html"),
        "red_button" => Some("red-button.html"),
        "tic_tac_toe" => Some("tic-tac-toe.html"),
        "connect_four" => Some("connect-four.html"),
        "wordle" => Some("wordle.html"),
        "poker" => Some("poker.html"),
        _ => None,
    };
    let spectator_url = viewer_page.map(|page| {
        format!(
            "{}/viewer/{}?matchId={}",
            state.public_base_url.trim_end_matches('/'),
            page,
            match_id
        )
    });

    info!(match_id = %match_id, env_type = %env_type, "Match created");

    Ok(Json(CreateMatchResponse {
        match_id,
        spectator_url,
        environment_type: env_type,
    }))
}

// =====================
// GET /matches/:id/state
// =====================

pub async fn get_state(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<StateQuery>,
) -> Response {
    let map = state.matches.read().await;
    let Some(inst) = map.get(&match_id) else {
        return not_found(&match_id);
    };
    let env = inst.environment.read().await;
    match query.player_id.as_deref() {
        Some(player_id) => match env.state_for_player(player_id) {
            Ok(state_json) => Json(serde_json::json!({ "state": state_json })).into_response(),
            Err(EnvironmentError::UnknownPlayer(_)) => {
                bad_request(format!("unknown player_id '{player_id}'"))
            }
            Err(e) => internal(e),
        },
        None => match env.full_state() {
            Ok(state_json) => Json(serde_json::json!({ "state": state_json })).into_response(),
            Err(e) => internal(e),
        },
    }
}

// =====================
// GET /matches/:id/legal_actions
// =====================

pub async fn get_legal_actions(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<StateQuery>,
) -> Response {
    let map = state.matches.read().await;
    let Some(inst) = map.get(&match_id) else {
        return not_found(&match_id);
    };
    let Some(player_id) = query.player_id.as_deref() else {
        return bad_request("player_id is required for legal_actions");
    };
    let env = inst.environment.read().await;
    match env.legal_actions(player_id) {
        Ok(actions_json) => Json(actions_json).into_response(),
        Err(EnvironmentError::UnknownPlayer(_)) => {
            bad_request(format!("unknown player_id '{player_id}'"))
        }
        Err(e) => internal(e),
    }
}

// =====================
// POST /matches/:id/actions
// =====================

pub async fn submit_action(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    Json(request): Json<SubmitActionRequest>,
) -> Response {
    let map = state.matches.read().await;
    let Some(inst) = map.get(&match_id) else {
        return not_found(&match_id);
    };

    let mut env = inst.environment.write().await;
    let was_terminal = env.is_terminal();

    match env.apply_action(&request.player_id, &request.action) {
        Ok(events_json) => {
            let seq = inst.sequence.fetch_add(1, Ordering::SeqCst);
            let is_terminal = env.is_terminal();
            drop(env);

            // Populate actor so spectators can attribute the action.
            let actor = Some(unified_event_protocol::EventActor {
                player_id: request.player_id.clone(),
                role: None,
                agent_id: None,
                agent_name: inst.player_names.get(&request.player_id).cloned(),
                model_provider: None,
            });

            // Wrap in a UnifiedEvent and broadcast.
            let unified = if is_terminal && !was_terminal {
                UnifiedEvent::terminal(
                    &inst.environment_type,
                    &match_id,
                    seq,
                    actor,
                    events_json.clone(),
                )
            } else {
                UnifiedEvent::action(
                    &inst.environment_type,
                    &match_id,
                    None,
                    seq,
                    actor,
                    events_json.clone(),
                    None,
                    is_terminal,
                )
            };
            let event_value =
                serde_json::to_value(&unified).expect("UnifiedEvent always serializes");
            {
                let mut log = inst.event_log.write().await;
                push_event_capped(&mut log, event_value.clone());
            }
            let _ = inst.broadcaster.send(event_value);

            Json(serde_json::json!({
                "accepted": true,
                "events": events_json,
                "is_terminal": is_terminal,
            }))
            .into_response()
        }
        Err(e) => match e {
            err @ (EnvironmentError::InvalidAction(_)
            | EnvironmentError::UnknownPlayer(_)
            | EnvironmentError::AlreadyTerminated
            | EnvironmentError::SerializationError(_)) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "accepted": false,
                    "error": err.to_string(),
                })),
            )
                .into_response(),
            err @ (EnvironmentError::InvalidSetup(_) | EnvironmentError::Internal(_)) => {
                internal(err)
            }
        },
    }
}

// =====================
// POST /matches/:id/reasoning
// =====================

pub async fn submit_reasoning(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    Json(request): Json<SubmitReasoningRequest>,
) -> Response {
    let map = state.matches.read().await;
    let Some(inst) = map.get(&match_id) else {
        return not_found(&match_id);
    };

    let seq = inst.sequence.fetch_add(1, Ordering::SeqCst);
    let event = UnifiedEvent::system(
        &inst.environment_type,
        &match_id,
        seq,
        serde_json::json!({
            "event_name": "agent_reasoning",
            "player_id": request.player_id,
            "reasoning": request.reasoning,
        }),
    );
    let event_value = serde_json::to_value(&event).expect("UnifiedEvent always serializes");
    {
        let mut log = inst.event_log.write().await;
        push_event_capped(&mut log, event_value.clone());
    }
    let _ = inst.broadcaster.send(event_value);

    StatusCode::NO_CONTENT.into_response()
}

// =====================
// GET /matches/:id/status
// =====================

pub async fn get_status(Path(match_id): Path<String>, State(state): State<AppState>) -> Response {
    let map = state.matches.read().await;
    let Some(inst) = map.get(&match_id) else {
        return not_found(&match_id);
    };
    let env = inst.environment.read().await;
    let is_terminal = env.is_terminal();
    let env_type = inst.environment_type.clone();
    drop(env);

    Json(MatchStatusResponse {
        is_terminal,
        environment_type: env_type,
        match_id: match_id.clone(),
    })
    .into_response()
}

// =====================
// GET /matches/:id/player_names
// =====================

pub async fn get_player_names(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let map = state.matches.read().await;
    let Some(inst) = map.get(&match_id) else {
        return not_found(&match_id);
    };
    Json(serde_json::json!({ "player_names": inst.player_names })).into_response()
}

// =====================
// GET /matches/:id/spectator/ws
// =====================

pub async fn spectator_ws(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> Response {
    let map = state.matches.read().await;
    let Some(inst) = map.get(&match_id) else {
        return not_found(&match_id);
    };
    // Subscribe to live events *before* reading the log to avoid gaps.
    let mut rx = inst.broadcaster.subscribe();
    // Snapshot the event log for catchup replay.
    let past_events = inst.event_log.read().await.clone();
    drop(map);

    ws.on_upgrade(move |mut socket| async move {
        // Send catchup: all past events bracketed by markers.
        if !past_events.is_empty() {
            let start = serde_json::json!({ "catchup_start": true });
            if socket.send(Message::Text(start.to_string())).await.is_err() {
                return;
            }
            for event in &past_events {
                let text = match serde_json::to_string(event) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                if socket.send(Message::Text(text)).await.is_err() {
                    return;
                }
            }
            let end = serde_json::json!({ "catchup_end": true });
            if socket.send(Message::Text(end.to_string())).await.is_err() {
                return;
            }
        }

        // Stream live events.
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let text = match serde_json::to_string(&event) {
                        Ok(t) => t,
                        Err(e) => {
                            warn!(error = %e, "failed to serialise spectator event");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(text)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, match_id = %match_id, "spectator WS lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}
