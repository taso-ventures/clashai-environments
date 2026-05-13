//! Red Button environment engine — deterministic state machine.
//!
//! [`RedButtonGame`] owns the authoritative match state and drives all
//! state transitions.  It has no I/O; callers are responsible for
//! serialising/broadcasting the returned [`SpectatorEvent`] list.

use std::collections::HashMap;

use chrono::Utc;
use thiserror::Error;

use crate::{
    ConfigSummary, PlayerId, PlayerPublicInfo, RedButtonAction, RedButtonConfig, RedButtonRole,
    RedButtonState, SpectatorEvent, SpokenMessage, TerminalReason, TurnActor, TurnInfo,
};

// =====================
// Errors
// =====================

/// Errors produced by [`RedButtonGame`] state-machine operations.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("wrong actor: expected {expected:?}, got {got:?}")]
    WrongActor {
        expected: RedButtonRole,
        got: RedButtonRole,
    },

    #[error("unknown player id: {0}")]
    UnknownPlayer(PlayerId),

    #[error("action not legal in current state: {0}")]
    IllegalAction(String),

    #[error("match already terminated")]
    AlreadyTerminated,

    #[error("message too long: {len} chars (max {max})")]
    MessageTooLong { len: usize, max: usize },

    #[error("empty message not allowed")]
    EmptyMessage,

    #[error("setup error: {0}")]
    InvalidSetup(String),
}

pub type EngineResult<T> = std::result::Result<T, EngineError>;

// =====================
// Environment State Machine
// =====================

/// Authoritative Red Button match state machine.
pub struct RedButtonGame {
    match_id: String,
    config: RedButtonConfig,

    /// Maps each player to their role.
    player_roles: HashMap<PlayerId, RedButtonRole>,

    /// Maps each player to their display name.
    player_names: HashMap<PlayerId, String>,

    /// Full conversation history (both spoken roles).
    conversation_history: Vec<SpokenMessage>,

    /// Current round (1-based).  One round = Persuader turn + Resistor turn.
    round: u32,

    /// Which role acts next within the current round.
    current_actor: TurnActor,

    /// Whether the Resistor has pressed the button.
    button_pressed: bool,

    /// Whether the match has terminated.
    is_terminal: bool,

    /// Winning role once terminal.
    winner_role: Option<RedButtonRole>,

    /// Why the match ended.
    terminal_reason: Option<TerminalReason>,

    /// Total individual actor turns completed.
    total_actor_turns: u32,
}

impl RedButtonGame {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Create a new match.
    ///
    /// Exactly 2 player IDs must be provided.  Player IDs are assigned roles
    /// deterministically: the player with the smaller ID becomes the
    /// Persuader, the larger ID becomes the Resistor.
    pub fn new(
        match_id: impl Into<String>,
        players: Vec<PlayerId>,
        player_names: HashMap<PlayerId, String>,
        config: RedButtonConfig,
    ) -> EngineResult<Self> {
        if players.len() != 2 {
            return Err(EngineError::InvalidSetup(format!(
                "exactly 2 players required, got {}",
                players.len()
            )));
        }
        if config.max_turns == 0 {
            return Err(EngineError::InvalidSetup(
                "max_turns must be > 0".to_string(),
            ));
        }

        let mut sorted = players.clone();
        sorted.sort();
        let mut player_roles = HashMap::new();
        player_roles.insert(sorted[0], RedButtonRole::Persuader);
        player_roles.insert(sorted[1], RedButtonRole::Resistor);

        Ok(Self {
            match_id: match_id.into(),
            config,
            player_roles,
            player_names,
            conversation_history: Vec::new(),
            round: 1,
            current_actor: TurnActor::Persuader,
            button_pressed: false,
            is_terminal: false,
            winner_role: None,
            terminal_reason: None,
            total_actor_turns: 0,
        })
    }

    // -----------------------------------------------------------------------
    // State Queries
    // -----------------------------------------------------------------------

    /// Return the environment state visible to a specific player.
    ///
    /// Red Button has no hidden information beyond internal reasoning; both
    /// players see the full conversation history.  This method exists to
    /// satisfy the `Environment` interface.
    pub fn state_for_player(&self, _player_id: PlayerId) -> EngineResult<RedButtonState> {
        Ok(self.build_state())
    }

    /// Return the full (unfiltered) environment state for admin/eval visibility.
    pub fn full_state(&self) -> RedButtonState {
        self.build_state()
    }

    /// Return the legal actions for a given player.
    ///
    /// Returns an empty list when it is not the player's turn or the match
    /// has ended.
    pub fn legal_actions(&self, player_id: PlayerId) -> Vec<RedButtonAction> {
        if self.is_terminal {
            return vec![];
        }
        let Some(role) = self.player_roles.get(&player_id) else {
            return vec![];
        };
        match (role, &self.current_actor) {
            (RedButtonRole::Persuader, TurnActor::Persuader) => {
                vec![RedButtonAction::Speak {
                    message: String::new(),
                }]
            }
            (RedButtonRole::Resistor, TurnActor::Resistor) => {
                vec![
                    RedButtonAction::IgnoreOtherAgent,
                    RedButtonAction::RespondToOtherAgent {
                        message: String::new(),
                    },
                    RedButtonAction::PressButton,
                ]
            }
            _ => vec![],
        }
    }

    /// Whether the match has reached a terminal state.
    pub fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    /// Return the winning player ID when the match is over.
    pub fn winner(&self) -> Option<PlayerId> {
        let role = self.winner_role.as_ref()?;
        self.player_id_for_role(role)
    }

    // -----------------------------------------------------------------------
    // Action Application
    // -----------------------------------------------------------------------

    /// Apply an action submitted by `player_id`.
    ///
    /// Validates actor identity, action legality, and message constraints.
    /// Returns the list of [`SpectatorEvent`]s to broadcast on success.
    pub fn apply_action(
        &mut self,
        player_id: PlayerId,
        action: &RedButtonAction,
    ) -> EngineResult<Vec<SpectatorEvent>> {
        if self.is_terminal {
            return Err(EngineError::AlreadyTerminated);
        }

        let role = self
            .player_roles
            .get(&player_id)
            .copied()
            .ok_or(EngineError::UnknownPlayer(player_id))?;

        // Validate it is this player's turn.
        let expected_role = match self.current_actor {
            TurnActor::Persuader => RedButtonRole::Persuader,
            TurnActor::Resistor => RedButtonRole::Resistor,
        };
        if role != expected_role {
            return Err(EngineError::WrongActor {
                expected: expected_role,
                got: role,
            });
        }

        // Enforce per-role action legality. Owning the turn is not enough —
        // the Persuader can only `Speak`; the Resistor can only respond,
        // ignore, or press.
        let action_legal_for_role = matches!(
            (role, action),
            (RedButtonRole::Persuader, RedButtonAction::Speak { .. })
                | (
                    RedButtonRole::Resistor,
                    RedButtonAction::IgnoreOtherAgent
                        | RedButtonAction::RespondToOtherAgent { .. }
                        | RedButtonAction::PressButton,
                )
        );
        if !action_legal_for_role {
            return Err(EngineError::IllegalAction(format!(
                "action not legal for {role:?}"
            )));
        }

        self.total_actor_turns += 1;
        let round = self.round;
        let now_ms = Utc::now().timestamp_millis();

        let mut events = Vec::new();

        match action {
            // ------------------------------------------------------------------
            // Persuader: Speak
            // ------------------------------------------------------------------
            RedButtonAction::Speak { message } => {
                // Role is guaranteed to be Persuader by the WrongActor check above.
                self.validate_message(message)?;

                let spoken = SpokenMessage {
                    turn: round,
                    speaker: RedButtonRole::Persuader,
                    player_id,
                    text: message.clone(),
                    timestamp_ms: now_ms,
                };
                self.conversation_history.push(spoken.clone());

                events.push(SpectatorEvent::MessageSpoken {
                    turn: round,
                    speaker_role: RedButtonRole::Persuader,
                    player: player_id,
                    message: message.clone(),
                });
                events.push(SpectatorEvent::ActionTaken {
                    turn: round,
                    actor_role: RedButtonRole::Persuader,
                    player: player_id,
                    action_type: "speak".to_string(),
                });

                // Hand off to Resistor within the same round.
                self.current_actor = TurnActor::Resistor;
                events.push(SpectatorEvent::TurnAdvanced {
                    round,
                    actor: TurnActor::Resistor,
                });
            }

            // ------------------------------------------------------------------
            // Resistor: Ignore
            // ------------------------------------------------------------------
            RedButtonAction::IgnoreOtherAgent => {
                // Role is guaranteed to be Resistor by the WrongActor check above.
                events.push(SpectatorEvent::ActionTaken {
                    turn: round,
                    actor_role: RedButtonRole::Resistor,
                    player: player_id,
                    action_type: "ignore_other_agent".to_string(),
                });

                self.advance_round_or_terminate(&mut events, round);
            }

            // ------------------------------------------------------------------
            // Resistor: Respond
            // ------------------------------------------------------------------
            RedButtonAction::RespondToOtherAgent { message } => {
                // Role is guaranteed to be Resistor by the WrongActor check above.
                self.validate_message(message)?;

                let spoken = SpokenMessage {
                    turn: round,
                    speaker: RedButtonRole::Resistor,
                    player_id,
                    text: message.clone(),
                    timestamp_ms: now_ms,
                };
                self.conversation_history.push(spoken);

                events.push(SpectatorEvent::MessageSpoken {
                    turn: round,
                    speaker_role: RedButtonRole::Resistor,
                    player: player_id,
                    message: message.clone(),
                });
                events.push(SpectatorEvent::ActionTaken {
                    turn: round,
                    actor_role: RedButtonRole::Resistor,
                    player: player_id,
                    action_type: "respond_to_other_agent".to_string(),
                });

                self.advance_round_or_terminate(&mut events, round);
            }

            // ------------------------------------------------------------------
            // Resistor: Press Button
            // ------------------------------------------------------------------
            RedButtonAction::PressButton => {
                // Role is guaranteed to be Resistor by the WrongActor check above.
                self.button_pressed = true;

                let persuader_id = self
                    .player_id_for_role(&RedButtonRole::Persuader)
                    .expect("engine invariant: Persuader role assigned at match start");

                events.push(SpectatorEvent::ButtonPressed {
                    turn: round,
                    player: player_id,
                });
                events.push(SpectatorEvent::ActionTaken {
                    turn: round,
                    actor_role: RedButtonRole::Resistor,
                    player: player_id,
                    action_type: "press_button".to_string(),
                });

                self.is_terminal = true;
                self.winner_role = Some(RedButtonRole::Persuader);
                self.terminal_reason = Some(TerminalReason::ButtonPressed);

                events.push(SpectatorEvent::GameOver {
                    winner_role: RedButtonRole::Persuader,
                    winner_player: persuader_id,
                    terminal_reason: TerminalReason::ButtonPressed,
                    total_turns: self.total_actor_turns,
                });
            }
        }

        Ok(events)
    }

    // -----------------------------------------------------------------------
    // Initialization Event
    // -----------------------------------------------------------------------

    /// Produce the [`SpectatorEvent::GameStarted`] event for broadcast at match start.
    pub fn game_started_event(&self) -> SpectatorEvent {
        let players: Vec<PlayerPublicInfo> = self
            .player_roles
            .iter()
            .map(|(&pid, role)| PlayerPublicInfo {
                player_id: pid,
                role: *role,
                display_name: self
                    .player_names
                    .get(&pid)
                    .cloned()
                    .unwrap_or_else(|| format!("Player {pid}")),
            })
            .collect();

        SpectatorEvent::GameStarted {
            players,
            config_summary: ConfigSummary {
                max_turns: self.config.max_turns,
                max_message_chars: self.config.max_message_chars,
            },
        }
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    /// Expose the config for use by the environment-server service.
    pub fn config(&self) -> &RedButtonConfig {
        &self.config
    }

    pub fn match_id(&self) -> &str {
        &self.match_id
    }

    pub fn player_roles(&self) -> &HashMap<PlayerId, RedButtonRole> {
        &self.player_roles
    }

    pub fn player_names(&self) -> &HashMap<PlayerId, String> {
        &self.player_names
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn build_state(&self) -> RedButtonState {
        let most_recent_message = self.conversation_history.last().cloned();
        let actor = self.current_actor;
        let round = self.round;

        RedButtonState {
            match_id: self.match_id.clone(),
            turn_info: TurnInfo { round, actor },
            conversation_history: self.conversation_history.clone(),
            most_recent_message,
            button_pressed: self.button_pressed,
            is_terminal: self.is_terminal,
            winner_role: self.winner_role,
            terminal_reason: self.terminal_reason,
            player_roles: self.player_roles.clone(),
        }
    }

    /// Called by Resistor actions that do not press the button.
    ///
    /// Either advances to the next round or terminates if max_turns reached.
    fn advance_round_or_terminate(&mut self, events: &mut Vec<SpectatorEvent>, round: u32) {
        if round >= self.config.max_turns {
            // Resistor wins: max rounds elapsed.
            let resistor_id = self
                .player_id_for_role(&RedButtonRole::Resistor)
                .unwrap_or(1);

            self.is_terminal = true;
            self.winner_role = Some(RedButtonRole::Resistor);
            self.terminal_reason = Some(TerminalReason::MaxTurns);

            events.push(SpectatorEvent::GameOver {
                winner_role: RedButtonRole::Resistor,
                winner_player: resistor_id,
                terminal_reason: TerminalReason::MaxTurns,
                total_turns: self.total_actor_turns,
            });
        } else {
            self.round += 1;
            self.current_actor = TurnActor::Persuader;

            events.push(SpectatorEvent::TurnAdvanced {
                round: self.round,
                actor: TurnActor::Persuader,
            });
        }
    }

    /// Validate a spoken message against config constraints.
    fn validate_message(&self, message: &str) -> EngineResult<()> {
        if !self.config.allow_empty_speak && message.is_empty() {
            return Err(EngineError::EmptyMessage);
        }
        let len = message.chars().count();
        let max = self.config.max_message_chars as usize;
        if len > max {
            return Err(EngineError::MessageTooLong { len, max });
        }
        Ok(())
    }

    fn player_id_for_role(&self, role: &RedButtonRole) -> Option<PlayerId> {
        self.player_roles
            .iter()
            .find(|(_, r)| *r == role)
            .map(|(&pid, _)| pid)
    }
}
