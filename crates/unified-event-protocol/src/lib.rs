//! Unified event envelope for cross-environment spectator streaming, replay,
//! and eval analytics.
//!
//! The [`UnifiedEvent`] type wraps per-environment spectator events in a
//! standard envelope that provides:
//!
//! - First-class reasoning data with a consistent shape across all environments
//! - Actor metadata (model, provider) for cross-model analytics
//! - Monotonic sequence numbers for replay, catch-up, and gap detection
//! - Standard event types usable by any downstream consumer
//!
//! # Backward compatibility
//!
//! Existing Coup and VibeCheck viewers consume the `action` field directly.
//! The `reasoning` field is additive — existing viewers can ignore it
//! and progressively add a reasoning panel.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

// =====================
// Event Type
// =====================

/// Top-level classification for all unified events.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnifiedEventType {
    /// Match created; player assignments and config broadcast.
    MatchStart,
    /// Agent submitted an action (with optional reasoning).
    Action,
    /// Non-action state transition (e.g. challenge window opened).
    StateChange,
    /// Match reached a terminal condition.
    Terminal,
    /// Infrastructure events: keepalive, catch-up markers.
    System,
}

// =====================
// Actor Metadata
// =====================

/// Who performed the action recorded in the event.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventActor {
    /// Numeric player ID as a string (matches per-environment PlayerId).
    pub player_id: String,
    /// Role name as a string (environment-specific, e.g. `"persuader"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Opaque agent ID registered in the gateway.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Human-readable agent display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// LLM provider identifier (e.g. `"openai"`, `"anthropic"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
}

// =====================
// Reasoning Payload
// =====================

/// LLM reasoning trace attached to an `Action` event.
///
/// Shape is identical across all environments, enabling cross-environment
/// eval analytics without joins.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventReasoning {
    /// Full reasoning / scratchpad text from the LLM.
    pub text: String,
    /// Prompt (input) token count, when reported by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u32>,
    /// Completion (output) token count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u32>,
    /// Wall-clock LLM latency in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Total context window length seen by the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
}

// =====================
// Unified Event Envelope
// =====================

/// The standard event envelope for all environments.
///
/// Consumers read `event_type` to dispatch, then use `action` for the
/// environment-specific payload and `reasoning` for the LLM trace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnifiedEvent {
    /// ULID-based globally unique event identifier.
    pub event_id: String,
    /// Classification of this event.
    pub event_type: UnifiedEventType,
    /// Environment type string (e.g. `"coup"`, `"red_button"`).
    pub environment_type: String,
    /// Match identifier.
    pub match_id: String,
    /// Turn or round number when the event occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<u32>,
    /// Monotonically increasing per-match sequence number (for replay/gap detection).
    pub sequence: u64,
    /// Actor metadata, present for `Action` events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<EventActor>,
    /// Environment-specific action payload (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<serde_json::Value>,
    /// LLM reasoning trace, present on `Action` events when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<EventReasoning>,
    /// Whether the match is in a terminal state after this event.
    pub is_terminal: bool,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: i64,
}

impl UnifiedEvent {
    /// Create a `MatchStart` event.
    pub fn match_start(
        environment_type: impl Into<String>,
        match_id: impl Into<String>,
        sequence: u64,
        action: serde_json::Value,
    ) -> Self {
        Self {
            event_id: Ulid::new().to_string(),
            event_type: UnifiedEventType::MatchStart,
            environment_type: environment_type.into(),
            match_id: match_id.into(),
            turn: None,
            sequence,
            actor: None,
            action: Some(action),
            reasoning: None,
            is_terminal: false,
            timestamp_ms: Utc::now().timestamp_millis(),
        }
    }

    /// Create an `Action` event.
    #[allow(clippy::too_many_arguments)]
    pub fn action(
        environment_type: impl Into<String>,
        match_id: impl Into<String>,
        turn: Option<u32>,
        sequence: u64,
        actor: Option<EventActor>,
        action: serde_json::Value,
        reasoning: Option<EventReasoning>,
        is_terminal: bool,
    ) -> Self {
        Self {
            event_id: Ulid::new().to_string(),
            event_type: UnifiedEventType::Action,
            environment_type: environment_type.into(),
            match_id: match_id.into(),
            turn,
            sequence,
            actor,
            action: Some(action),
            reasoning,
            is_terminal,
            timestamp_ms: Utc::now().timestamp_millis(),
        }
    }

    /// Create a `Terminal` event.
    pub fn terminal(
        environment_type: impl Into<String>,
        match_id: impl Into<String>,
        sequence: u64,
        action: serde_json::Value,
    ) -> Self {
        Self {
            event_id: Ulid::new().to_string(),
            event_type: UnifiedEventType::Terminal,
            environment_type: environment_type.into(),
            match_id: match_id.into(),
            turn: None,
            sequence,
            actor: None,
            action: Some(action),
            reasoning: None,
            is_terminal: true,
            timestamp_ms: Utc::now().timestamp_millis(),
        }
    }

    /// Create a `System` event (keepalive, catch-up markers).
    pub fn system(
        environment_type: impl Into<String>,
        match_id: impl Into<String>,
        sequence: u64,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_id: Ulid::new().to_string(),
            event_type: UnifiedEventType::System,
            environment_type: environment_type.into(),
            match_id: match_id.into(),
            turn: None,
            sequence,
            actor: None,
            action: Some(payload),
            reasoning: None,
            is_terminal: false,
            timestamp_ms: Utc::now().timestamp_millis(),
        }
    }
}
