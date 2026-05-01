pub mod environment;

pub use environment::{
    EnvironmentAction, EnvironmentState, EnvironmentWinner, SequentialDecisionKind,
    SequentialPhase, SequentialState,
};

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

macro_rules! extensible_string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $wire:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
            Custom(String),
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let value = match self {
                    $(Self::$variant => $wire,)+
                    Self::Custom(value) => value,
                };
                serializer.serialize_str(value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Ok(match value.as_str() {
                    $($wire => Self::$variant,)+
                    _ => Self::Custom(value),
                })
            }
        }
    };
}

/// Verification status value indicating the step was skipped because the agent
/// is waiting (dependency not ready). Used by `StandardProviderHarness` and
/// checked by the sequential orchestrator to avoid treating a waiting skip as
/// an execution error.
pub const VERIFICATION_SKIPPED_WAITING: &str = "skipped_waiting";

extensible_string_enum! {
    pub enum ExecutionClass {
        StandardProviderHarness => "standard_provider_harness",
        ExternalAgentRuntime => "external_agent_runtime",
    }
}

extensible_string_enum! {
    pub enum BenchmarkLane {
        StructuredAction => "structured_action",
        ToolAgent => "tool_agent",
        CodeAgent => "code_agent",
        BrowserAgent => "browser_agent",
        VisionAgent => "vision_agent",
        ConversationalAgent => "conversational_agent",
        ResearchAgent => "research_agent",
    }
}

extensible_string_enum! {
    pub enum HarnessProfile {
        JsonSingleShot => "json_single_shot",
        HybridTools => "hybrid_tools",
        ToolFirst => "tool_first",
        LongRunningSession => "long_running_session",
    }
}

extensible_string_enum! {
    pub enum SessionMode {
        Stateless => "stateless",
        PerTurnSession => "per_turn_session",
        PersistentRunSession => "persistent_run_session",
    }
}

extensible_string_enum! {
    pub enum SessionContinuationStrategy {
        ServerPreviousResponseId => "server_previous_response_id",
        ClientReplay => "client_replay",
        ClientReplayWithPromptCaching => "client_replay_with_prompt_caching",
        ClientReplayWithThoughtSignatures => "client_replay_with_thought_signatures",
        ServerEncryptedResume => "server_encrypted_resume",
    }
}

extensible_string_enum! {
    pub enum SessionPhase {
        Working => "working",
        Final => "final",
    }
}

extensible_string_enum! {
    pub enum ToolAuthorityPolicy {
        EnvironmentOnly => "environment_only",
        ClientToolsAllowed => "client_tools_allowed",
        ProviderServerToolsAllowed => "provider_server_tools_allowed",
        Mixed => "mixed",
    }
}

extensible_string_enum! {
    pub enum ProviderSurfacePolicy {
        NativeOnly => "native_only",
        CompatibilityAllowed => "compatibility_allowed",
        CompatibilityOnly => "compatibility_only",
        Pinned => "pinned",
    }
}

extensible_string_enum! {
    pub enum ProviderFamily {
        FirstParty => "first_party",
        Aggregator => "aggregator",
    }
}

extensible_string_enum! {
    pub enum ApiSurface {
        OpenAiChatCompletions => "open_ai_chat_completions",
        OpenAiResponses => "open_ai_responses",
        OpenAiCompatibleChat => "open_ai_compatible_chat",
        AnthropicMessages => "anthropic_messages",
        GeminiDirect => "gemini_direct",
        GeminiOpenAiCompatibility => "gemini_open_ai_compatibility",
        OpenRouterChatCompletions => "open_router_chat_completions",
        OpenRouterResponsesBeta => "open_router_responses_beta",
        XaiChatCompletions => "xai_chat_completions",
        XaiResponses => "xai_responses",
    }
}

/// Canonical information about the current turn state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnInfo {
    /// Current turn number (1-based).
    pub turn_number: u32,

    /// Name of the current phase (environment-specific).
    pub phase: String,

    /// Players whose input is required to advance.
    pub active_players: Vec<String>,

    /// Whether the environment has reached a terminal state.
    pub is_terminal: bool,

    /// Classification of the current decision: `"active"`, `"reactive"`, or `"forced"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_kind: Option<String>,

    /// Opaque revision tag for stale-state detection and replay fidelity
    /// (e.g. `"turn:5:phase:main"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_revision: Option<String>,

    /// Remaining step budget in milliseconds. The shared runtime enforces this
    /// when present and also records it for eval telemetry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_deadline_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyStatus {
    Ready,
    Waiting {
        reason: String,
    },
    Blocked {
        reason: String,
        missing: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingContextSeverity {
    Blocking,
    Recoverable,
    Advisory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingContextIssue {
    pub key: String,
    pub message: String,
    pub severity: MissingContextSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputResponseMode {
    JsonText,
    ToolLoopTextFinal,
    ToolCalls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputContractSpec {
    pub response_mode: OutputResponseMode,
    pub expected_tool_behavior: String,
    pub completion_condition: String,
    pub allow_empty_final_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSessionState {
    pub phase: SessionPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_content_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signature_state: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_resume_state: Option<serde_json::Value>,
    #[serde(default)]
    pub replay_transcript: Vec<serde_json::Value>,
}

impl Default for RuntimeSessionState {
    fn default() -> Self {
        Self {
            phase: SessionPhase::Working,
            previous_response_id: None,
            compaction_state: None,
            continuity_id: None,
            cached_content_id: None,
            thought_signature_state: None,
            encrypted_resume_state: None,
            replay_transcript: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationPolicy {
    pub require_legal_action_match: bool,
    pub require_grounding: bool,
    pub require_format_validation: bool,
    pub require_side_effect_safety: bool,
    pub max_retries: u32,
}

impl Default for VerificationPolicy {
    fn default() -> Self {
        Self {
            require_legal_action_match: true,
            require_grounding: true,
            require_format_validation: true,
            require_side_effect_safety: true,
            max_retries: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPolicy {
    pub max_retries: u32,
    pub allow_empty_result_recovery: bool,
    pub allow_reprompt_on_invalid_action: bool,
}

impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 1,
            allow_empty_result_recovery: true,
            allow_reprompt_on_invalid_action: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptPolicyTemplate {
    pub version: String,
    pub include_output_contract: bool,
    pub include_dependency_checks: bool,
    pub include_tool_persistence: bool,
    pub include_completeness_contract: bool,
    pub include_empty_result_recovery: bool,
    pub include_verification_loop: bool,
    pub include_missing_context_gating: bool,
}

impl Default for PromptPolicyTemplate {
    fn default() -> Self {
        Self {
            version: "standard-provider-v1".to_string(),
            include_output_contract: true,
            include_dependency_checks: true,
            include_tool_persistence: true,
            include_completeness_contract: true,
            include_empty_result_recovery: true,
            include_verification_loop: true,
            include_missing_context_gating: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessRunPolicy {
    pub profile: HarnessProfile,
    pub tool_authority_policy: ToolAuthorityPolicy,
    pub provider_surface_policy: ProviderSurfacePolicy,
    pub session_strategy: Option<SessionContinuationStrategy>,
    pub reasoning_effort: Option<String>,
    pub verbosity: Option<String>,
    pub image_detail: Option<String>,
    pub max_steps: u32,
    pub max_tool_iterations: u32,
    pub require_verification: bool,
    pub require_completeness_tracking: bool,
    pub allow_empty_result_recovery: bool,
    pub session_mode: SessionMode,
    pub verification_policy: VerificationPolicy,
    pub recovery_policy: RecoveryPolicy,
    pub prompt_policy: PromptPolicyTemplate,
}

impl Default for HarnessRunPolicy {
    fn default() -> Self {
        Self {
            profile: HarnessProfile::ToolFirst,
            tool_authority_policy: ToolAuthorityPolicy::EnvironmentOnly,
            provider_surface_policy: ProviderSurfacePolicy::NativeOnly,
            session_strategy: None,
            reasoning_effort: None,
            verbosity: None,
            image_detail: None,
            max_steps: 1,
            max_tool_iterations: 8,
            require_verification: true,
            require_completeness_tracking: true,
            allow_empty_result_recovery: true,
            session_mode: SessionMode::Stateless,
            verification_policy: VerificationPolicy::default(),
            recovery_policy: RecoveryPolicy::default(),
            prompt_policy: PromptPolicyTemplate::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRunSpec {
    pub run_id: String,
    pub suite_id: String,
    pub suite_version: String,
    pub task_id: String,
    pub task_version: String,
    pub trial_group_id: String,
    pub trial_index: u32,
    pub trial_count: u32,
    pub dataset_snapshot_id: String,
    pub environment_type: String,
    pub benchmark_lane: BenchmarkLane,
    pub execution_class: ExecutionClass,
    pub seed: u64,
    pub max_steps: u32,
    pub wall_clock_budget_ms: u64,
    pub run_policy: HarnessRunPolicy,
    pub competitor_id: String,
    pub competitor_version: String,
    pub api_surface: ApiSurface,
    pub provider_family: ProviderFamily,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutcome {
    pub accepted_actions: serde_json::Value,
    pub rejected_actions: serde_json::Value,
    pub is_terminal: bool,
    pub blocked: bool,
    pub degraded: bool,
}

impl Default for StepOutcome {
    fn default() -> Self {
        Self {
            accepted_actions: serde_json::Value::Array(Vec::new()),
            rejected_actions: serde_json::Value::Array(Vec::new()),
            is_terminal: false,
            blocked: false,
            degraded: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalTraceEventType {
    RunStarted,
    TurnInfoRead,
    ObservationRead,
    RulesRead,
    LegalActionsRead,
    LlmRequestStarted,
    LlmResponseReceived,
    LlmRequestFailed,
    ToolCallStarted,
    ToolResultReceived,
    ToolCallFailed,
    ActionAttempted,
    ActionResult,
    VerificationCompleted,
    RecoveryAttempted,
    SessionStateUpdated,
    RunFinished,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalTraceEvent {
    pub run_id: String,
    pub step_index: u32,
    pub event_type: EvalTraceEventType,
    pub timestamp_ms: u64,
    pub execution_class: ExecutionClass,
    pub benchmark_lane: BenchmarkLane,
    pub environment_type: String,
    pub competitor_id: String,
    pub competitor_version: String,
    pub provider_family: Option<ProviderFamily>,
    pub api_surface: Option<ApiSurface>,
    pub harness_profile: Option<HarnessProfile>,
    pub request_index: Option<u32>,
    pub tool_sequence_number: Option<u32>,
    pub payload: serde_json::Value,
}

impl EvalTraceEvent {
    pub fn now(
        spec: &EvalRunSpec,
        step_index: u32,
        event_type: EvalTraceEventType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            run_id: spec.run_id.clone(),
            step_index,
            event_type,
            timestamp_ms: current_timestamp_ms(),
            execution_class: spec.execution_class.clone(),
            benchmark_lane: spec.benchmark_lane.clone(),
            environment_type: spec.environment_type.clone(),
            competitor_id: spec.competitor_id.clone(),
            competitor_version: spec.competitor_version.clone(),
            provider_family: Some(spec.provider_family.clone()),
            api_surface: Some(spec.api_surface.clone()),
            harness_profile: Some(spec.run_policy.profile.clone()),
            request_index: None,
            tool_sequence_number: None,
            payload,
        }
    }
}

pub trait TraceSink: Send + Sync {
    fn emit(&self, event: EvalTraceEvent);
}

#[derive(Debug, Default)]
pub struct NoopTraceSink;

impl TraceSink for NoopTraceSink {
    fn emit(&self, _event: EvalTraceEvent) {}
}

#[derive(Debug, Default)]
pub struct InMemoryTraceSink {
    events: Mutex<Vec<EvalTraceEvent>>,
}

impl InMemoryTraceSink {
    pub fn take(&self) -> Vec<EvalTraceEvent> {
        match self.events.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            Err(e) => {
                tracing::warn!("InMemoryTraceSink: mutex poisoned in take(): {e}");
                Vec::new()
            }
        }
    }

    pub fn events(&self) -> Vec<EvalTraceEvent> {
        match self.events.lock() {
            Ok(guard) => guard.clone(),
            Err(e) => {
                tracing::warn!("InMemoryTraceSink: mutex poisoned in events(): {e}");
                Vec::new()
            }
        }
    }
}

impl TraceSink for InMemoryTraceSink {
    fn emit(&self, event: EvalTraceEvent) {
        match self.events.lock() {
            Ok(mut guard) => guard.push(event),
            Err(e) => {
                tracing::warn!("InMemoryTraceSink: mutex poisoned in emit(), dropping event: {e}");
            }
        }
    }
}

/// Compute a deterministic step index from turn number and sequence within that turn.
///
/// Formula: `(turn - 1) * 1000 + sequence`, where turn is 1-based and sequence is
/// the action offset within the turn (typically 1 for the first decision).
pub const fn compute_step_index(turn: u32, sequence: u32) -> i32 {
    (turn.saturating_sub(1) * 1000 + sequence) as i32
}

/// Number of variants in [`EvalTraceEventType`].
pub const EVAL_TRACE_EVENT_TYPE_COUNT: usize = 17;

#[derive(Debug, thiserror::Error)]
pub enum EvalRuntimeError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("runtime error: {0}")]
    Runtime(String),
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_trace_event_covers_all_required_event_types() {
        let event_types = [
            EvalTraceEventType::RunStarted,
            EvalTraceEventType::TurnInfoRead,
            EvalTraceEventType::ObservationRead,
            EvalTraceEventType::RulesRead,
            EvalTraceEventType::LegalActionsRead,
            EvalTraceEventType::LlmRequestStarted,
            EvalTraceEventType::LlmResponseReceived,
            EvalTraceEventType::LlmRequestFailed,
            EvalTraceEventType::ToolCallStarted,
            EvalTraceEventType::ToolResultReceived,
            EvalTraceEventType::ToolCallFailed,
            EvalTraceEventType::ActionAttempted,
            EvalTraceEventType::ActionResult,
            EvalTraceEventType::VerificationCompleted,
            EvalTraceEventType::RecoveryAttempted,
            EvalTraceEventType::SessionStateUpdated,
            EvalTraceEventType::RunFinished,
        ];

        assert_eq!(event_types.len(), EVAL_TRACE_EVENT_TYPE_COUNT);
    }

    #[test]
    fn eval_run_spec_serializes_with_required_dimensions() {
        let spec = EvalRunSpec {
            run_id: "run_123".to_string(),
            suite_id: "suite".to_string(),
            suite_version: "v1".to_string(),
            task_id: "task".to_string(),
            task_version: "v2".to_string(),
            trial_group_id: "trial-group".to_string(),
            trial_index: 0,
            trial_count: 1,
            dataset_snapshot_id: "snapshot".to_string(),
            environment_type: "freeciv".to_string(),
            benchmark_lane: BenchmarkLane::ToolAgent,
            execution_class: ExecutionClass::StandardProviderHarness,
            seed: 42,
            max_steps: 4,
            wall_clock_budget_ms: 60_000,
            run_policy: HarnessRunPolicy::default(),
            competitor_id: "openai:gpt-5.4".to_string(),
            competitor_version: "2026-03-01".to_string(),
            api_surface: ApiSurface::OpenAiResponses,
            provider_family: ProviderFamily::FirstParty,
        };

        let json = serde_json::to_value(spec).expect("serialize");
        assert_eq!(json["benchmark_lane"], "tool_agent");
        assert_eq!(json["execution_class"], "standard_provider_harness");
        assert_eq!(json["api_surface"], "open_ai_responses");
        assert_eq!(json["provider_family"], "first_party");
    }
}
