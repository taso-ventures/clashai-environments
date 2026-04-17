//! Unified environment server.
//!
//! Serves all supported game environments from a single binary.
//! Uses the [`Environment`] trait from `environment-engine` as the
//! abstraction layer, routing all operations through the environment registry.
//!
//! # Route structure
//!
//! ```text
//! POST   /matches                          — create a match (any environment type)
//! GET    /matches/:id/state               — query player-filtered state
//! GET    /matches/:id/legal_actions       — legal actions for active player
//! POST   /matches/:id/actions             — submit a player action
//! POST   /matches/:id/reasoning           — forward LLM reasoning to spectators
//! GET    /matches/:id/status              — match status (terminal, winner, turn)
//! GET    /matches/:id/player_names        — player id → name mapping
//! GET    /matches/:id/spectator/ws        — WebSocket spectator stream
//! GET    /health                          — liveness probe
//! ```

pub mod routes;

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;

use environment_engine::{registry::EnvironmentRegistry, Environment};
use redis::AsyncCommands;

// =====================
// AppState
// =====================

type BoxEnv = Box<dyn Environment + Send + Sync>;
type BroadcastSender = broadcast::Sender<serde_json::Value>;

/// A live match managed by the service.
pub struct MatchInstance {
    pub environment: Arc<RwLock<BoxEnv>>,
    pub broadcaster: BroadcastSender,
    pub player_names: HashMap<i32, String>,
    pub environment_type: String,
    pub sequence: std::sync::atomic::AtomicU64,
    /// Append-only log of all broadcast events for spectator catchup on connect.
    pub event_log: RwLock<Vec<serde_json::Value>>,
}

#[derive(Clone)]
pub struct AppState {
    pub matches: Arc<RwLock<HashMap<String, Arc<MatchInstance>>>>,
    pub registry: Arc<EnvironmentRegistry>,
    pub public_base_url: String,
    pub ws_capacity: usize,
    pub redis: Option<redis::aio::ConnectionManager>,
}

/// TTL for Redis replay keys: 7 days (matches gateway replay.rs REDIS_TTL_SECS).
/// i64 because redis::expire requires it.
const REDIS_TTL_SECS: i64 = 604_800;

impl AppState {
    pub fn new(public_base_url: String, ws_capacity: usize) -> Self {
        let registry = EnvironmentRegistry::with_defaults();
        Self {
            matches: Arc::new(RwLock::new(HashMap::new())),
            registry: Arc::new(registry),
            public_base_url,
            ws_capacity,
            redis: None,
        }
    }

    pub fn with_redis(mut self, conn: redis::aio::ConnectionManager) -> Self {
        self.redis = Some(conn);
        self
    }

    pub async fn redis_store_event(&self, match_id: &str, event_json: &str) {
        let Some(ref redis) = self.redis else { return };
        let key = format!("spectator:{match_id}:events");
        let mut conn = redis.clone();
        let result: Result<(), _> = redis::pipe()
            .rpush(&key, event_json)
            .expire(&key, REDIS_TTL_SECS)
            .query_async(&mut conn)
            .await;
        if let Err(e) = result {
            tracing::warn!(match_id = %match_id, error = %e, "Failed to cache spectator event in Redis");
        }
    }

    pub async fn redis_store_player_names(&self, match_id: &str, names: &HashMap<i32, String>) {
        let Some(ref redis) = self.redis else { return };
        let key = format!("spectator:{match_id}:player_names");
        let mut conn = redis.clone();
        if let Ok(json) = serde_json::to_string(names) {
            let result: Result<(), _> = redis::pipe()
                .set(&key, &json)
                .expire(&key, REDIS_TTL_SECS)
                .query_async(&mut conn)
                .await;
            if let Err(e) = result {
                tracing::warn!(match_id = %match_id, error = %e, "Failed to cache player names in Redis");
            }
        }
    }

    pub async fn redis_load_replay(
        &self,
        match_id: &str,
    ) -> Option<(Vec<serde_json::Value>, HashMap<i32, String>)> {
        let redis = self.redis.as_ref()?;
        let mut conn = redis.clone();

        let events_key = format!("spectator:{match_id}:events");
        let names_key = format!("spectator:{match_id}:player_names");

        let event_strings: Vec<String> = conn.lrange(&events_key, 0, -1).await.ok()?;
        if event_strings.is_empty() {
            return None;
        }

        let events: Vec<serde_json::Value> = event_strings
            .into_iter()
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect();

        let names: HashMap<i32, String> = conn
            .get::<_, Option<String>>(&names_key)
            .await
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        Some((events, names))
    }

    /// Reap completed matches when the total count exceeds 100.
    pub async fn reap_completed_matches(&self) {
        let mut map = self.matches.write().await;
        if map.len() <= 100 {
            return;
        }
        let terminal_ids: Vec<String> = {
            let mut ids = Vec::new();
            for (id, inst) in map.iter() {
                let env = inst.environment.read().await;
                if env.is_terminal() {
                    ids.push(id.clone());
                }
            }
            ids
        };
        for id in terminal_ids {
            map.remove(&id);
        }
    }
}

// =====================
// HTTP Models
// =====================

/// Generic `POST /matches` request body.
#[derive(Debug, Deserialize)]
pub struct CreateMatchRequest {
    pub environment_type: String,
    pub player_count: Option<usize>,
    pub seed: Option<u64>,
    pub match_id: Option<String>,
    pub player_names: Option<HashMap<i32, String>>,
    pub player_ids: Option<Vec<i32>>,
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct CreateMatchResponse {
    pub match_id: String,
    pub spectator_url: Option<String>,
    pub environment_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SubmitActionRequest {
    pub player_id: i32,
    pub action: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct SubmitReasoningRequest {
    pub player_id: i32,
    pub reasoning: String,
}

#[derive(Debug, Deserialize)]
pub struct StateQuery {
    pub player_id: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct MatchStatusResponse {
    pub is_terminal: bool,
    pub environment_type: String,
    pub match_id: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// =====================
// Router
// =====================

pub fn build_router(state: AppState) -> Router {
    // Determine the static directory path.
    // In debug builds, fall back to CARGO_MANIFEST_DIR so `cargo run` works from
    // the workspace root without setting STATIC_DIR.  In release / Docker the
    // env var (or relative "static") is used instead.
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| {
        #[cfg(debug_assertions)]
        {
            let manifest_relative = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
            if std::path::Path::new(manifest_relative).is_dir() {
                return manifest_relative.to_string();
            }
        }
        "static".to_string()
    });

    Router::new()
        .route("/health", get(health))
        .route("/matches", post(routes::create_match))
        .route("/matches/:id/state", get(routes::get_state))
        .route("/matches/:id/legal_actions", get(routes::get_legal_actions))
        .route("/matches/:id/actions", post(routes::submit_action))
        .route("/matches/:id/reasoning", post(routes::submit_reasoning))
        .route("/matches/:id/status", get(routes::get_status))
        .route("/matches/:id/player_names", get(routes::get_player_names))
        .route("/matches/:id/spectator/ws", get(routes::spectator_ws))
        // Serve the 3D viewer (static/viewer/) and legacy static assets.
        .nest_service("/viewer", ServeDir::new(format!("{static_dir}/viewer")))
        .nest_service("/legacy-viewer", ServeDir::new(&static_dir))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub fn log_startup(addr: &str) {
    info!(%addr, "environment-server starting");
}

// =====================
// Shared handlers
// =====================

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}
