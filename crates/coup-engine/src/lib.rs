//! In-process Coup game engine.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use thiserror::Error;
use tracing::{debug, warn};

use coup_protocol::{
    ActionHistoryEntry, ActionId, Card, CoupAction, CoupState, PendingAction, PlayerId,
    PlayerPublicInfo, PlayerState, Role, SpectatorEvent, TurnPhase,
};

#[derive(Debug, Error)]
pub enum CoupEngineError {
    #[error("invalid setup: {0}")]
    InvalidSetup(String),
    #[error("invalid action: {0}")]
    InvalidAction(String),
    #[error("not player's turn")]
    NotPlayersTurn,
    #[error("player not in match")]
    UnknownPlayer,
    #[error("player eliminated")]
    PlayerEliminated,
    #[error("action not allowed in this phase")]
    PhaseMismatch,
    #[error("challenge not allowed")]
    ChallengeNotAllowed,
    #[error("block not allowed")]
    BlockNotAllowed,
    #[error("illegal target")]
    InvalidTarget,
}

pub type Result<T> = std::result::Result<T, CoupEngineError>;

#[derive(Debug, Clone)]
pub struct CoupGameConfig {
    pub reaction_timeout: Duration,
}

impl Default for CoupGameConfig {
    fn default() -> Self {
        Self {
            reaction_timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub events: Vec<SpectatorEvent>,
}

#[derive(Debug, Clone, Copy)]
enum ChallengeKind {
    Action,
    Block,
}

#[derive(Debug, Clone, Copy)]
struct ChallengeOutcome {
    kind: ChallengeKind,
    challenger: PlayerId,
    success: bool,
}

#[derive(Debug, Clone)]
struct RevealContext {
    player: PlayerId,
    required_role: Role,
    outcome: ChallengeOutcome,
}

#[derive(Debug, Clone, Copy)]
enum PendingResolution {
    AfterChallenge(ChallengeOutcome),
    AfterActionLoss,
}

pub struct CoupGame {
    state: CoupState,
    deck: Vec<Role>,
    rng: StdRng,
    next_action_id: ActionId,
    config: CoupGameConfig,
    pending_resolution: Option<PendingResolution>,
    pending_reveal: Option<RevealContext>,
    /// Accumulated spectator events for replay / catchup on reconnect.
    /// Bounded by game length (~100 turns max, ~5 events/turn ≈ 500 entries).
    event_history: Vec<SpectatorEvent>,
}

impl CoupGame {
    pub fn new(player_count: usize, seed: u64) -> Result<Self> {
        Self::new_with_config(player_count, seed, CoupGameConfig::default())
    }

    pub fn new_with_config(player_count: usize, seed: u64, config: CoupGameConfig) -> Result<Self> {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut deck = Vec::with_capacity(15);
        for role in Role::all_roles() {
            for _ in 0..3 {
                deck.push(*role);
            }
        }
        deck.shuffle(&mut rng);

        let mut players = HashMap::new();
        for player_id in 0..player_count as i32 {
            let first = deck.pop().ok_or_else(|| {
                CoupEngineError::InvalidSetup("deck exhausted during setup".to_string())
            })?;
            let second = deck.pop().ok_or_else(|| {
                CoupEngineError::InvalidSetup("deck exhausted during setup".to_string())
            })?;
            let cards = vec![
                Card {
                    role: first,
                    revealed: false,
                },
                Card {
                    role: second,
                    revealed: false,
                },
            ];
            players.insert(
                player_id,
                PlayerState {
                    coins: 2,
                    cards,
                    eliminated: false,
                },
            );
        }

        let state = CoupState {
            turn_number: 1,
            current_phase: TurnPhase::AwaitingAction,
            active_player: 0,
            players,
            pending_action: None,
            action_history: Vec::new(),
            deck_count: deck.len(),
        };

        Ok(Self {
            state,
            deck,
            rng,
            next_action_id: 1,
            config,
            pending_resolution: None,
            pending_reveal: None,
            event_history: Vec::new(),
        })
    }

    pub fn state(&self) -> &CoupState {
        &self.state
    }

    fn player(&self, player: PlayerId) -> Result<&PlayerState> {
        self.state
            .players
            .get(&player)
            .ok_or(CoupEngineError::UnknownPlayer)
    }

    fn player_mut(&mut self, player: PlayerId) -> Result<&mut PlayerState> {
        self.state
            .players
            .get_mut(&player)
            .ok_or(CoupEngineError::UnknownPlayer)
    }

    pub fn state_mut(&mut self) -> &mut CoupState {
        &mut self.state
    }

    pub fn legal_actions(&self, player: &PlayerId) -> Vec<CoupAction> {
        let Some(player_state) = self.state.players.get(player) else {
            return vec![];
        };
        if player_state.eliminated {
            return vec![];
        }

        match &self.state.current_phase {
            TurnPhase::AwaitingAction => {
                if *player != self.state.active_player {
                    return vec![];
                }
                let mut actions = vec![CoupAction::Income, CoupAction::ForeignAid];
                let coins = player_state.coins;
                let alive_targets: Vec<PlayerId> = self
                    .state
                    .players
                    .iter()
                    .filter(|(pid, p)| **pid != *player && !p.eliminated)
                    .map(|(pid, _)| *pid)
                    .collect();

                if coins >= 7 {
                    for target in &alive_targets {
                        actions.push(CoupAction::Coup { target: *target });
                    }
                }

                if coins >= 3 {
                    for target in &alive_targets {
                        actions.push(CoupAction::Assassinate { target: *target });
                    }
                }

                for target in &alive_targets {
                    actions.push(CoupAction::Steal { target: *target });
                }

                actions.push(CoupAction::Tax);
                actions.push(CoupAction::Exchange);

                if coins >= 10 {
                    actions.retain(|a| matches!(a, CoupAction::Coup { .. }));
                }

                actions
            }
            TurnPhase::ChallengeWindow { waiting_on, .. } => {
                if !waiting_on.contains(player) {
                    return vec![];
                }
                let mut actions = vec![CoupAction::Pass];
                if let Some(pending) = &self.state.pending_action {
                    if can_challenge(&pending.action) {
                        actions.push(CoupAction::Challenge {
                            action_id: pending.id,
                        });
                    }
                }
                actions
            }
            TurnPhase::BlockWindow { waiting_on, .. } => {
                if !waiting_on.contains(player) {
                    return vec![];
                }
                let mut actions = vec![CoupAction::Pass];
                if let Some(pending) = &self.state.pending_action {
                    actions.extend(legal_block_actions(pending));
                }
                actions
            }
            TurnPhase::BlockChallengeWindow { waiting_on, .. } => {
                if !waiting_on.contains(player) {
                    return vec![];
                }
                let mut actions = vec![CoupAction::Pass];
                if let Some(pending) = &self.state.pending_action {
                    if pending.blocked_by.is_some() {
                        actions.push(CoupAction::Challenge {
                            action_id: pending.id,
                        });
                    }
                }
                actions
            }
            TurnPhase::RevealingCard {
                player: reveal_player,
                required_role,
            } => {
                if player != reveal_player {
                    return vec![];
                }
                let mut actions = Vec::new();
                for (idx, card) in player_state.cards.iter().enumerate() {
                    if !card.revealed && card.role == *required_role {
                        actions.push(CoupAction::RevealCard { card_index: idx });
                    }
                }
                actions
            }
            TurnPhase::SelectingCardToLose {
                player: loss_player,
            } => {
                if player != loss_player {
                    return vec![];
                }
                let mut actions = Vec::new();
                for (idx, card) in player_state.cards.iter().enumerate() {
                    if !card.revealed {
                        actions.push(CoupAction::SelectCardToLose { card_index: idx });
                    }
                }
                actions
            }
            TurnPhase::ExchangeSelection {
                player: exchange_player,
            } => {
                if player != exchange_player {
                    return vec![];
                }
                let Some(pending) = &self.state.pending_action else {
                    return vec![];
                };
                let unrevealed_indices: Vec<usize> = player_state
                    .cards
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| !card.revealed)
                    .map(|(idx, _)| idx)
                    .collect();
                let draw_count = pending.exchange_draw.len();
                let total_choices = unrevealed_indices.len() + draw_count;
                if unrevealed_indices.is_empty() || total_choices == 0 {
                    return vec![];
                }
                let keep_count = unrevealed_indices.len();
                let mut actions = Vec::new();
                let indices: Vec<usize> = (0..total_choices).collect();
                combinations(&indices, keep_count, &mut Vec::new(), &mut actions);
                actions
            }
            TurnPhase::ActionResolving | TurnPhase::GameOver { .. } => vec![],
        }
    }

    pub fn apply_action(&mut self, player: &PlayerId, action: &CoupAction) -> Result<ApplyOutcome> {
        let mut events = Vec::new();
        if !self.state.players.contains_key(player) {
            return Err(CoupEngineError::UnknownPlayer);
        }
        if self.state.players.get(player).is_some_and(|p| p.eliminated) {
            return Err(CoupEngineError::PlayerEliminated);
        }

        match &self.state.current_phase {
            TurnPhase::AwaitingAction => {
                self.apply_active_action(*player, action, &mut events)?;
            }
            TurnPhase::ChallengeWindow { .. } => {
                self.apply_challenge_window_action(*player, action, &mut events)?;
            }
            TurnPhase::BlockWindow { .. } => {
                self.apply_block_window_action(*player, action, &mut events)?;
            }
            TurnPhase::BlockChallengeWindow { .. } => {
                self.apply_block_challenge_action(*player, action, &mut events)?;
            }
            TurnPhase::RevealingCard { .. } => {
                self.apply_reveal_action(*player, action, &mut events)?;
            }
            TurnPhase::SelectingCardToLose { .. } => {
                self.apply_select_loss_action(*player, action, &mut events)?;
            }
            TurnPhase::ExchangeSelection { .. } => {
                self.apply_exchange_selection(*player, action, &mut events)?;
            }
            TurnPhase::ActionResolving | TurnPhase::GameOver { .. } => {
                return Err(CoupEngineError::PhaseMismatch);
            }
        }

        self.state.deck_count = self.deck.len();
        self.check_game_over(&mut events);

        self.event_history.extend(events.iter().cloned());

        Ok(ApplyOutcome { events })
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.state.current_phase, TurnPhase::GameOver { .. })
    }

    pub fn winner(&self) -> Option<PlayerId> {
        match &self.state.current_phase {
            TurnPhase::GameOver { winner } => Some(*winner),
            _ => None,
        }
    }

    pub fn initial_events(&self) -> Vec<SpectatorEvent> {
        let players = self
            .state
            .players
            .iter()
            .map(|(id, p)| PlayerPublicInfo {
                player_id: *id,
                eliminated: p.eliminated,
            })
            .collect();
        let mut events = vec![SpectatorEvent::GameStarted { players }];
        events.extend(self.event_history.iter().cloned());
        events
    }

    /// Push an externally-created event (e.g. AgentReasoning) into the event
    /// history so that reconnecting spectators see it.
    pub fn push_external_event(&mut self, event: SpectatorEvent) {
        self.event_history.push(event);
    }

    fn apply_active_action(
        &mut self,
        player: PlayerId,
        action: &CoupAction,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        if player != self.state.active_player {
            return Err(CoupEngineError::NotPlayersTurn);
        }
        if matches!(action, CoupAction::Forfeit) {
            self.eliminate_player(player, events);
            self.advance_turn(events);
            return Ok(());
        }
        if action.is_reactive() || action.is_resolution() {
            return Err(CoupEngineError::InvalidAction(
                "reactive/resolution action not allowed on active turn".to_string(),
            ));
        }
        let coins = self.player(player)?.coins;
        if coins >= 10 && !matches!(action, CoupAction::Coup { .. }) {
            return Err(CoupEngineError::InvalidAction(
                "must coup when holding 10+ coins".to_string(),
            ));
        }

        match action {
            CoupAction::Income => {
                self.player_mut(player)?.coins += 1;
                self.record_history(player, action.clone(), "income".to_string());
                events.push(SpectatorEvent::ActionDeclared {
                    player,
                    action: action.clone(),
                });
                self.advance_turn(events);
            }
            CoupAction::ForeignAid => {
                let pending = self.build_pending_action(player, action.clone(), None, None);
                self.state.pending_action = Some(pending);
                events.push(SpectatorEvent::ActionDeclared {
                    player,
                    action: action.clone(),
                });
                self.state.current_phase = TurnPhase::BlockWindow {
                    waiting_on: self.waiting_on_all_except(player),
                    deadline: self.reaction_deadline(),
                };
            }
            CoupAction::Coup { target } => {
                self.ensure_target_valid(player, *target)?;
                if coins < 7 {
                    return Err(CoupEngineError::InvalidAction(
                        "not enough coins to coup".to_string(),
                    ));
                }
                self.player_mut(player)?.coins -= 7;
                self.state.pending_action =
                    Some(self.build_pending_action(player, action.clone(), Some(*target), None));
                events.push(SpectatorEvent::ActionDeclared {
                    player,
                    action: action.clone(),
                });
                self.enter_card_loss_phase(*target, PendingResolution::AfterActionLoss, events)?;
            }
            CoupAction::Tax => {
                let pending =
                    self.build_pending_action(player, action.clone(), None, Some(Role::Duke));
                self.state.pending_action = Some(pending);
                events.push(SpectatorEvent::ActionDeclared {
                    player,
                    action: action.clone(),
                });
                self.state.current_phase = TurnPhase::ChallengeWindow {
                    waiting_on: self.waiting_on_all_except(player),
                    deadline: self.reaction_deadline(),
                };
            }
            CoupAction::Assassinate { target } => {
                self.ensure_target_valid(player, *target)?;
                if coins < 3 {
                    return Err(CoupEngineError::InvalidAction(
                        "not enough coins to assassinate".to_string(),
                    ));
                }
                self.player_mut(player)?.coins -= 3;
                let pending = self.build_pending_action(
                    player,
                    action.clone(),
                    Some(*target),
                    Some(Role::Assassin),
                );
                self.state.pending_action = Some(pending);
                events.push(SpectatorEvent::ActionDeclared {
                    player,
                    action: action.clone(),
                });
                self.state.current_phase = TurnPhase::ChallengeWindow {
                    waiting_on: self.waiting_on_all_except(player),
                    deadline: self.reaction_deadline(),
                };
            }
            CoupAction::Steal { target } => {
                self.ensure_target_valid(player, *target)?;
                let pending = self.build_pending_action(
                    player,
                    action.clone(),
                    Some(*target),
                    Some(Role::Captain),
                );
                self.state.pending_action = Some(pending);
                events.push(SpectatorEvent::ActionDeclared {
                    player,
                    action: action.clone(),
                });
                self.state.current_phase = TurnPhase::ChallengeWindow {
                    waiting_on: self.waiting_on_all_except(player),
                    deadline: self.reaction_deadline(),
                };
            }
            CoupAction::Exchange => {
                let pending =
                    self.build_pending_action(player, action.clone(), None, Some(Role::Ambassador));
                self.state.pending_action = Some(pending);
                events.push(SpectatorEvent::ActionDeclared {
                    player,
                    action: action.clone(),
                });
                self.state.current_phase = TurnPhase::ChallengeWindow {
                    waiting_on: self.waiting_on_all_except(player),
                    deadline: self.reaction_deadline(),
                };
            }
            _ => {
                return Err(CoupEngineError::InvalidAction(
                    "unsupported action in active phase".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn apply_challenge_window_action(
        &mut self,
        player: PlayerId,
        action: &CoupAction,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        let TurnPhase::ChallengeWindow { waiting_on, .. } = &mut self.state.current_phase else {
            return Err(CoupEngineError::PhaseMismatch);
        };
        if !waiting_on.contains(&player) {
            return Err(CoupEngineError::InvalidAction(
                "not in waiting list".to_string(),
            ));
        }
        match action {
            CoupAction::Pass => {
                waiting_on.retain(|pid| *pid != player);
                if waiting_on.is_empty() {
                    self.advance_after_challenge_no_challenge(events);
                }
                Ok(())
            }
            CoupAction::Challenge { action_id } => {
                let (actor, claimed) = {
                    let pending = self
                        .state
                        .pending_action
                        .as_mut()
                        .ok_or(CoupEngineError::PhaseMismatch)?;
                    if pending.id != *action_id {
                        return Err(CoupEngineError::InvalidAction(
                            "action_id mismatch".to_string(),
                        ));
                    }
                    if pending.challenged_by.is_some() {
                        return Err(CoupEngineError::InvalidAction(
                            "action already challenged".to_string(),
                        ));
                    }
                    if !can_challenge(&pending.action) {
                        return Err(CoupEngineError::ChallengeNotAllowed);
                    }
                    pending.challenged_by = Some(player);
                    events.push(SpectatorEvent::ChallengeIssued {
                        challenger: player,
                        against: pending.actor,
                    });
                    (pending.actor, pending.claimed_role)
                };
                let claimed = claimed.ok_or(CoupEngineError::ChallengeNotAllowed)?;
                if self.player_has_unrevealed_role(actor, claimed) {
                    let outcome = ChallengeOutcome {
                        kind: ChallengeKind::Action,
                        challenger: player,
                        success: false,
                    };
                    self.pending_reveal = Some(RevealContext {
                        player: actor,
                        required_role: claimed,
                        outcome,
                    });
                    self.state.current_phase = TurnPhase::RevealingCard {
                        player: actor,
                        required_role: claimed,
                    };
                } else {
                    let outcome = ChallengeOutcome {
                        kind: ChallengeKind::Action,
                        challenger: player,
                        success: true,
                    };
                    self.enter_card_loss_phase(
                        actor,
                        PendingResolution::AfterChallenge(outcome),
                        events,
                    )?;
                }
                Ok(())
            }
            _ => Err(CoupEngineError::InvalidAction(
                "unsupported action in challenge window".to_string(),
            )),
        }
    }

    fn apply_block_window_action(
        &mut self,
        player: PlayerId,
        action: &CoupAction,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        let TurnPhase::BlockWindow { waiting_on, .. } = &mut self.state.current_phase else {
            return Err(CoupEngineError::PhaseMismatch);
        };
        if !waiting_on.contains(&player) {
            return Err(CoupEngineError::InvalidAction(
                "not in waiting list".to_string(),
            ));
        }
        match action {
            CoupAction::Pass => {
                waiting_on.retain(|pid| *pid != player);
                if waiting_on.is_empty() {
                    self.resolve_pending_action(events)?;
                }
                Ok(())
            }
            CoupAction::Block {
                action_id,
                claimed_role,
            } => {
                let pending = self
                    .state
                    .pending_action
                    .as_mut()
                    .ok_or(CoupEngineError::PhaseMismatch)?;
                if pending.id != *action_id {
                    return Err(CoupEngineError::InvalidAction(
                        "action_id mismatch".to_string(),
                    ));
                }
                if pending.blocked_by.is_some() {
                    return Err(CoupEngineError::InvalidAction(
                        "action already blocked".to_string(),
                    ));
                }
                if !can_block(&pending.action, *claimed_role) {
                    return Err(CoupEngineError::BlockNotAllowed);
                }
                pending.blocked_by = Some(player);
                pending.block_claimed_role = Some(*claimed_role);
                events.push(SpectatorEvent::BlockDeclared {
                    blocker: player,
                    role: *claimed_role,
                });
                self.state.current_phase = TurnPhase::BlockChallengeWindow {
                    waiting_on: self.waiting_on_all_except(player),
                    deadline: self.reaction_deadline(),
                };
                Ok(())
            }
            _ => Err(CoupEngineError::InvalidAction(
                "unsupported action in block window".to_string(),
            )),
        }
    }

    fn apply_block_challenge_action(
        &mut self,
        player: PlayerId,
        action: &CoupAction,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        let TurnPhase::BlockChallengeWindow { waiting_on, .. } = &mut self.state.current_phase
        else {
            return Err(CoupEngineError::PhaseMismatch);
        };
        if !waiting_on.contains(&player) {
            return Err(CoupEngineError::InvalidAction(
                "not in waiting list".to_string(),
            ));
        }
        match action {
            CoupAction::Pass => {
                waiting_on.retain(|pid| *pid != player);
                if waiting_on.is_empty() {
                    // no challenge -> block stands, cancel action
                    self.state.pending_action = None;
                    self.advance_turn(events);
                }
                Ok(())
            }
            CoupAction::Challenge { action_id } => {
                let pending = self
                    .state
                    .pending_action
                    .as_mut()
                    .ok_or(CoupEngineError::PhaseMismatch)?;
                if pending.id != *action_id {
                    return Err(CoupEngineError::InvalidAction(
                        "action_id mismatch".to_string(),
                    ));
                }
                let blocker = pending.blocked_by.ok_or(CoupEngineError::BlockNotAllowed)?;
                let claimed = pending
                    .block_claimed_role
                    .ok_or(CoupEngineError::BlockNotAllowed)?;
                events.push(SpectatorEvent::ChallengeIssued {
                    challenger: player,
                    against: blocker,
                });
                if self.player_has_unrevealed_role(blocker, claimed) {
                    let outcome = ChallengeOutcome {
                        kind: ChallengeKind::Block,
                        challenger: player,
                        success: false,
                    };
                    self.pending_reveal = Some(RevealContext {
                        player: blocker,
                        required_role: claimed,
                        outcome,
                    });
                    self.state.current_phase = TurnPhase::RevealingCard {
                        player: blocker,
                        required_role: claimed,
                    };
                } else {
                    let outcome = ChallengeOutcome {
                        kind: ChallengeKind::Block,
                        challenger: player,
                        success: true,
                    };
                    self.enter_card_loss_phase(
                        blocker,
                        PendingResolution::AfterChallenge(outcome),
                        events,
                    )?;
                }
                Ok(())
            }
            _ => Err(CoupEngineError::InvalidAction(
                "unsupported action in block challenge window".to_string(),
            )),
        }
    }

    fn apply_reveal_action(
        &mut self,
        player: PlayerId,
        action: &CoupAction,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        let TurnPhase::RevealingCard {
            player: reveal_player,
            required_role,
        } = &self.state.current_phase
        else {
            return Err(CoupEngineError::PhaseMismatch);
        };
        if player != *reveal_player {
            return Err(CoupEngineError::InvalidAction(
                "not reveal player".to_string(),
            ));
        }
        let CoupAction::RevealCard { card_index } = action else {
            return Err(CoupEngineError::InvalidAction(
                "expected reveal_card".to_string(),
            ));
        };
        let Some(player_state) = self.state.players.get_mut(&player) else {
            return Err(CoupEngineError::UnknownPlayer);
        };
        let card = player_state
            .cards
            .get(*card_index)
            .ok_or_else(|| CoupEngineError::InvalidAction("card_index out of range".to_string()))?
            .clone();
        if card.revealed || card.role != *required_role {
            return Err(CoupEngineError::InvalidAction(
                "invalid card revealed".to_string(),
            ));
        }

        // Remove card and return it to deck, then draw replacement if available.
        let revealed_role = card.role;
        player_state.cards.remove(*card_index);
        self.deck.push(revealed_role);
        self.deck.shuffle(&mut self.rng);
        if let Some(role) = self.deck.pop() {
            player_state.cards.push(Card {
                role,
                revealed: false,
            });
        } else {
            warn!("Deck empty during reveal replacement");
        }
        events.push(SpectatorEvent::CardRevealed {
            player,
            role: revealed_role,
        });

        let reveal_context = self
            .pending_reveal
            .take()
            .ok_or(CoupEngineError::PhaseMismatch)?;
        if reveal_context.player != player || reveal_context.required_role != *required_role {
            return Err(CoupEngineError::PhaseMismatch);
        }
        self.enter_card_loss_phase(
            reveal_context.outcome.challenger,
            PendingResolution::AfterChallenge(reveal_context.outcome),
            events,
        )?;
        Ok(())
    }

    fn apply_select_loss_action(
        &mut self,
        player: PlayerId,
        action: &CoupAction,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        let TurnPhase::SelectingCardToLose {
            player: loss_player,
        } = &self.state.current_phase
        else {
            return Err(CoupEngineError::PhaseMismatch);
        };
        if player != *loss_player {
            return Err(CoupEngineError::InvalidAction(
                "not loss player".to_string(),
            ));
        }
        let CoupAction::SelectCardToLose { card_index } = action else {
            return Err(CoupEngineError::InvalidAction(
                "expected select_card_to_lose".to_string(),
            ));
        };
        let Some(player_state) = self.state.players.get_mut(&player) else {
            return Err(CoupEngineError::UnknownPlayer);
        };
        let card = player_state
            .cards
            .get_mut(*card_index)
            .ok_or_else(|| CoupEngineError::InvalidAction("card_index out of range".to_string()))?;
        if card.revealed {
            return Err(CoupEngineError::InvalidAction(
                "card already revealed".to_string(),
            ));
        }
        card.revealed = true;
        events.push(SpectatorEvent::InfluenceLost {
            player,
            role: card.role,
        });

        if player_state.cards.iter().all(|c| c.revealed) {
            self.eliminate_player(player, events);
        }

        match self.pending_resolution.take() {
            Some(PendingResolution::AfterChallenge(outcome)) => {
                self.resolve_after_challenge(outcome, events)?;
            }
            Some(PendingResolution::AfterActionLoss) => {
                self.state.pending_action = None;
                self.advance_turn(events);
            }
            None => {
                // Default: just advance turn
                self.state.pending_action = None;
                self.advance_turn(events);
            }
        }

        Ok(())
    }

    fn apply_exchange_selection(
        &mut self,
        player: PlayerId,
        action: &CoupAction,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        let TurnPhase::ExchangeSelection {
            player: exchange_player,
        } = &self.state.current_phase
        else {
            return Err(CoupEngineError::PhaseMismatch);
        };
        if player != *exchange_player {
            return Err(CoupEngineError::InvalidAction(
                "not exchange player".to_string(),
            ));
        }
        let CoupAction::ExchangeSelection { keep_indices } = action else {
            return Err(CoupEngineError::InvalidAction(
                "expected exchange_selection".to_string(),
            ));
        };
        let Some(pending) = self.state.pending_action.clone() else {
            return Err(CoupEngineError::PhaseMismatch);
        };
        let player_state = self
            .state
            .players
            .get_mut(&player)
            .ok_or(CoupEngineError::UnknownPlayer)?;

        let unrevealed_indices: Vec<usize> = player_state
            .cards
            .iter()
            .enumerate()
            .filter(|(_, card)| !card.revealed)
            .map(|(idx, _)| idx)
            .collect();
        let keep_count = unrevealed_indices.len();
        let draw_roles = pending.exchange_draw.clone();
        let total_choices = keep_count + draw_roles.len();
        if keep_indices.len() != keep_count {
            return Err(CoupEngineError::InvalidAction(
                "keep_indices length mismatch".to_string(),
            ));
        }
        if keep_indices.iter().any(|i| *i >= total_choices) {
            return Err(CoupEngineError::InvalidAction(
                "keep_indices out of range".to_string(),
            ));
        }
        let mut unique_indices = keep_indices.clone();
        unique_indices.sort_unstable();
        unique_indices.dedup();
        if unique_indices.len() != keep_indices.len() {
            return Err(CoupEngineError::InvalidAction(
                "duplicate keep_indices".to_string(),
            ));
        }
        let mut kept_unrevealed_cards = Vec::new();
        let mut choice_pool: Vec<Card> = Vec::new();
        for idx in &unrevealed_indices {
            if let Some(card) = player_state.cards.get(*idx) {
                choice_pool.push(card.clone());
            }
        }
        for role in draw_roles {
            choice_pool.push(Card {
                role,
                revealed: false,
            });
        }

        for idx in keep_indices {
            kept_unrevealed_cards.push(
                choice_pool.get(*idx).cloned().ok_or_else(|| {
                    CoupEngineError::InvalidAction("invalid keep index".to_string())
                })?,
            );
        }

        // Return unkept drawn cards to deck
        for (idx, card) in choice_pool.into_iter().enumerate() {
            if !keep_indices.contains(&idx) {
                self.deck.push(card.role);
            }
        }
        self.deck.shuffle(&mut self.rng);

        // Rebuild player's cards: keep revealed cards, replace unrevealed with kept selection
        let mut new_cards = Vec::new();
        for card in &player_state.cards {
            if card.revealed {
                new_cards.push(card.clone());
            }
        }
        new_cards.extend(kept_unrevealed_cards);
        player_state.cards = new_cards;

        self.state.pending_action = None;
        self.advance_turn(events);

        Ok(())
    }

    fn resolve_pending_action(&mut self, events: &mut Vec<SpectatorEvent>) -> Result<()> {
        let Some(pending) = self.state.pending_action.clone() else {
            return Ok(());
        };
        match pending.action {
            CoupAction::ForeignAid => {
                self.add_coins(pending.actor, 2);
                self.record_history(pending.actor, pending.action, "foreign_aid".to_string());
                self.state.pending_action = None;
                self.advance_turn(events);
            }
            CoupAction::Tax => {
                self.add_coins(pending.actor, 3);
                self.record_history(pending.actor, pending.action, "tax".to_string());
                self.state.pending_action = None;
                self.advance_turn(events);
            }
            CoupAction::Assassinate { target } => {
                self.record_history(pending.actor, pending.action, "assassinate".to_string());
                self.enter_card_loss_phase(target, PendingResolution::AfterActionLoss, events)?;
            }
            CoupAction::Steal { target } => {
                let stolen = self.steal_coins(pending.actor, target);
                self.record_history(pending.actor, pending.action, format!("steal:{stolen}"));
                self.state.pending_action = None;
                self.advance_turn(events);
            }
            CoupAction::Exchange => {
                let draw = self.draw_roles(2);
                if let Some(pending_mut) = self.state.pending_action.as_mut() {
                    pending_mut.exchange_draw = draw;
                }
                self.state.current_phase = TurnPhase::ExchangeSelection {
                    player: pending.actor,
                };
            }
            _ => {
                self.state.pending_action = None;
                self.advance_turn(events);
            }
        }
        Ok(())
    }

    fn resolve_after_challenge(
        &mut self,
        outcome: ChallengeOutcome,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        match outcome.kind {
            ChallengeKind::Action => {
                if outcome.success {
                    // challenged failed, action canceled
                    self.state.pending_action = None;
                    self.advance_turn(events);
                } else {
                    // challenge failed, continue to block or resolve
                    self.advance_after_challenge_no_challenge(events);
                }
            }
            ChallengeKind::Block => {
                if outcome.success {
                    // block failed, resolve action
                    if let Some(pending) = self.state.pending_action.as_mut() {
                        pending.blocked_by = None;
                        pending.block_claimed_role = None;
                    }
                    self.resolve_pending_action(events)?;
                } else {
                    // block stands, cancel action
                    self.state.pending_action = None;
                    self.advance_turn(events);
                }
            }
        }
        Ok(())
    }

    fn advance_after_challenge_no_challenge(&mut self, events: &mut Vec<SpectatorEvent>) {
        let Some(pending) = &self.state.pending_action else {
            self.advance_turn(events);
            return;
        };
        if is_blockable(&pending.action) {
            let waiting_on = match pending.action {
                // Per official rules: "any other player may Block it by claiming
                // to have the proper character." All blockable actions allow any
                // non-actor alive player to attempt a block.
                CoupAction::ForeignAid
                | CoupAction::Assassinate { .. }
                | CoupAction::Steal { .. } => self.waiting_on_all_except(pending.actor),
                _ => vec![],
            };
            if waiting_on.is_empty() {
                let _ = self.resolve_pending_action(events);
            } else {
                self.state.current_phase = TurnPhase::BlockWindow {
                    waiting_on,
                    deadline: self.reaction_deadline(),
                };
            }
        } else {
            let _ = self.resolve_pending_action(events);
        }
    }

    fn add_coins(&mut self, player: PlayerId, amount: i32) {
        if let Some(p) = self.state.players.get_mut(&player) {
            p.coins += amount;
        }
    }

    fn steal_coins(&mut self, actor: PlayerId, target: PlayerId) -> i32 {
        let mut stolen = 0;
        if let Some(target_state) = self.state.players.get_mut(&target) {
            stolen = target_state.coins.min(2);
            target_state.coins -= stolen;
        }
        if let Some(actor_state) = self.state.players.get_mut(&actor) {
            actor_state.coins += stolen;
        }
        stolen
    }

    fn build_pending_action(
        &mut self,
        actor: PlayerId,
        action: CoupAction,
        target: Option<PlayerId>,
        claimed_role: Option<Role>,
    ) -> PendingAction {
        let id = self.next_action_id;
        self.next_action_id += 1;
        PendingAction {
            id,
            actor,
            action,
            target,
            claimed_role,
            challenged_by: None,
            blocked_by: None,
            block_claimed_role: None,
            exchange_draw: Vec::new(),
        }
    }

    fn reaction_deadline(&self) -> DateTime<Utc> {
        Utc::now()
            + ChronoDuration::from_std(self.config.reaction_timeout)
                .unwrap_or_else(|_| ChronoDuration::seconds(30))
    }

    fn waiting_on_all_except(&self, player: PlayerId) -> Vec<PlayerId> {
        self.state
            .players
            .iter()
            .filter(|(pid, p)| **pid != player && !p.eliminated)
            .map(|(pid, _)| *pid)
            .collect()
    }

    fn ensure_target_valid(&self, actor: PlayerId, target: PlayerId) -> Result<()> {
        if actor == target {
            return Err(CoupEngineError::InvalidTarget);
        }
        let Some(target_state) = self.state.players.get(&target) else {
            return Err(CoupEngineError::InvalidTarget);
        };
        if target_state.eliminated {
            return Err(CoupEngineError::InvalidTarget);
        }
        Ok(())
    }

    fn player_has_unrevealed_role(&self, player: PlayerId, role: Role) -> bool {
        self.state
            .players
            .get(&player)
            .map(|p| p.cards.iter().any(|c| !c.revealed && c.role == role))
            .unwrap_or(false)
    }

    fn draw_roles(&mut self, count: usize) -> Vec<Role> {
        let mut roles = Vec::new();
        for _ in 0..count {
            if let Some(role) = self.deck.pop() {
                roles.push(role);
            } else {
                debug!("Deck empty during draw");
                break;
            }
        }
        roles
    }

    fn record_history(&mut self, actor: PlayerId, action: CoupAction, outcome: String) {
        self.state.action_history.push(ActionHistoryEntry {
            turn: self.state.turn_number,
            actor,
            action,
            outcome,
            timestamp: Utc::now(),
        });
    }

    fn enter_card_loss_phase(
        &mut self,
        player: PlayerId,
        resolution: PendingResolution,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        let has_unrevealed = self
            .state
            .players
            .get(&player)
            .map(|p| p.cards.iter().any(|c| !c.revealed))
            .unwrap_or(false);

        if has_unrevealed {
            self.pending_resolution = Some(resolution);
            self.state.current_phase = TurnPhase::SelectingCardToLose { player };
        } else {
            let is_eliminated = self
                .state
                .players
                .get(&player)
                .map(|p| p.eliminated)
                .unwrap_or(true);
            if !is_eliminated {
                self.eliminate_player(player, events);
            }
            match resolution {
                PendingResolution::AfterChallenge(outcome) => {
                    self.resolve_after_challenge(outcome, events)?;
                }
                PendingResolution::AfterActionLoss => {
                    self.state.pending_action = None;
                    self.advance_turn(events);
                }
            }
        }
        Ok(())
    }

    fn eliminate_player(&mut self, player: PlayerId, events: &mut Vec<SpectatorEvent>) {
        if let Some(p) = self.state.players.get_mut(&player) {
            p.eliminated = true;
            events.push(SpectatorEvent::PlayerEliminated { player });
        }
    }

    fn advance_turn(&mut self, events: &mut Vec<SpectatorEvent>) {
        if let Some(next) = self.next_active_player(self.state.active_player) {
            self.state.active_player = next;
            self.state.turn_number += 1;
            self.state.current_phase = TurnPhase::AwaitingAction;
            self.state.pending_action = None;
            events.push(SpectatorEvent::TurnAdvanced {
                turn: self.state.turn_number,
                active_player: next,
            });
        }
    }

    fn next_active_player(&self, current: PlayerId) -> Option<PlayerId> {
        let mut ids: Vec<PlayerId> = self.state.players.keys().copied().collect();
        ids.sort();
        if ids.is_empty() {
            return None;
        }
        let start_index = ids.iter().position(|id| *id == current).unwrap_or(0);
        for offset in 1..=ids.len() {
            let idx = (start_index + offset) % ids.len();
            if let Some(pid) = ids.get(idx) {
                if let Some(state) = self.state.players.get(pid) {
                    if !state.eliminated {
                        return Some(*pid);
                    }
                }
            }
        }
        None
    }

    fn check_game_over(&mut self, events: &mut Vec<SpectatorEvent>) {
        let alive: Vec<PlayerId> = self
            .state
            .players
            .iter()
            .filter(|(_, p)| !p.eliminated)
            .map(|(pid, _)| *pid)
            .collect();
        if alive.len() == 1 {
            let winner = alive[0];
            self.state.current_phase = TurnPhase::GameOver { winner };
            events.push(SpectatorEvent::GameOver { winner });
        }
    }
}

pub fn can_challenge(action: &CoupAction) -> bool {
    matches!(
        action,
        CoupAction::Tax
            | CoupAction::Assassinate { .. }
            | CoupAction::Steal { .. }
            | CoupAction::Exchange
    )
}

pub fn can_block(action: &CoupAction, blocker_role: Role) -> bool {
    match action {
        CoupAction::ForeignAid => blocker_role == Role::Duke,
        CoupAction::Assassinate { .. } => blocker_role == Role::Contessa,
        CoupAction::Steal { .. } => matches!(blocker_role, Role::Captain | Role::Ambassador),
        _ => false,
    }
}

fn is_blockable(action: &CoupAction) -> bool {
    matches!(
        action,
        CoupAction::ForeignAid | CoupAction::Assassinate { .. } | CoupAction::Steal { .. }
    )
}

fn legal_block_actions(pending: &PendingAction) -> Vec<CoupAction> {
    let mut actions = Vec::new();
    for role in Role::all_roles() {
        if can_block(&pending.action, *role) {
            actions.push(CoupAction::Block {
                action_id: pending.id,
                claimed_role: *role,
            });
        }
    }
    actions
}

fn combinations(indices: &[usize], k: usize, current: &mut Vec<usize>, out: &mut Vec<CoupAction>) {
    if current.len() == k {
        out.push(CoupAction::ExchangeSelection {
            keep_indices: current.clone(),
        });
        return;
    }
    if indices.is_empty() {
        return;
    }
    if let Some((first, rest)) = indices.split_first() {
        current.push(*first);
        combinations(rest, k, current, out);
        current.pop();
        combinations(rest, k, current, out);
    }
}
