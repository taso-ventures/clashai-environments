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
//! POST   /matches/:id/reasoning           — forward sanitized rationale to spectators
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
use serde::{de, Deserialize, Deserializer, Serialize};
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
    pub player_names: HashMap<String, String>,
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

/// TTL for Redis replay keys: 7 days.
/// i64 because redis::expire requires it.
const REDIS_TTL_SECS: i64 = 604_800;

/// Maximum number of spectator events retained per match for catchup-on-connect.
/// When a match exceeds this, the oldest entries are evicted FIFO. The live
/// broadcast stream is unaffected — only late-joining spectators see the
/// truncated tail.
pub const EVENT_LOG_CAP: usize = 5_000;

/// Append `event` to `log`, evicting the oldest entry if the cap is exceeded.
pub fn push_event_capped(log: &mut Vec<serde_json::Value>, event: serde_json::Value) {
    if log.len() >= EVENT_LOG_CAP {
        log.remove(0);
    }
    log.push(event);
}

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

    pub async fn redis_store_player_names(&self, match_id: &str, names: &HashMap<String, String>) {
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
    ) -> Option<(Vec<serde_json::Value>, HashMap<String, String>)> {
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

        let names: HashMap<String, String> = conn
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
    pub player_names: Option<HashMap<String, String>>,
    #[serde(default, deserialize_with = "deserialize_optional_player_ids")]
    pub player_ids: Option<Vec<String>>,
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
    #[serde(deserialize_with = "deserialize_player_id")]
    pub player_id: String,
    pub action: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct SubmitReasoningRequest {
    #[serde(deserialize_with = "deserialize_player_id")]
    pub player_id: String,
    pub reasoning: String,
}

#[derive(Debug, Deserialize)]
pub struct StateQuery {
    #[serde(default, deserialize_with = "deserialize_optional_player_id")]
    pub player_id: Option<String>,
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

fn deserialize_player_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PlayerIdWire {
        String(String),
        Signed(i64),
        Unsigned(u64),
    }

    match PlayerIdWire::deserialize(deserializer)? {
        PlayerIdWire::String(id) => Ok(id),
        PlayerIdWire::Signed(id) => Ok(id.to_string()),
        PlayerIdWire::Unsigned(id) => Ok(id.to_string()),
    }
}

fn deserialize_optional_player_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(value) = Option::<serde_json::Value>::deserialize(deserializer)? else {
        return Ok(None);
    };
    player_id_from_value(value)
        .map(Some)
        .map_err(de::Error::custom)
}

fn deserialize_optional_player_ids<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(values) = Option::<Vec<serde_json::Value>>::deserialize(deserializer)? else {
        return Ok(None);
    };
    values
        .into_iter()
        .map(player_id_from_value)
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
        .map_err(de::Error::custom)
}

fn player_id_from_value(value: serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::String(id) => Ok(id),
        serde_json::Value::Number(number) => Ok(number.to_string()),
        other => Err(format!("player id must be a string or number, got {other}")),
    }
}

// =====================
// Router
// =====================

pub fn build_router(state: AppState) -> Router {
    // Determine the static directory path.
    // Docker sets STATIC_DIR explicitly. Otherwise fall back to CARGO_MANIFEST_DIR
    // (the path is baked in at compile time) so `cargo run [--release]` works from
    // the workspace root. The runtime existence check guards against the source
    // tree being absent on the host.
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| {
        let manifest_relative = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
        if std::path::Path::new(manifest_relative).is_dir() {
            manifest_relative.to_string()
        } else {
            "static".to_string()
        }
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
        // Serve the 3D viewer assets out of static/viewer/.
        .nest_service("/viewer", ServeDir::new(format!("{static_dir}/viewer")))
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
