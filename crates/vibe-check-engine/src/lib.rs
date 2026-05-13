//! In-process Vibe Check game engine.

use std::collections::HashMap;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use thiserror::Error;
use tracing::debug;

use vibe_check_protocol::{
    PlayerId, PlayerInfo, RoundResult, ScoringZone, SpectatorEvent, SpectrumCard, StealDirection,
    Target, TeamId, TeamState, TurnPhase, VibeCheckAction, VibeCheckState, ZoneConfig,
};

// ─── Errors ───

#[derive(Debug, Error)]
pub enum VibeCheckEngineError {
    #[error("invalid setup: {0}")]
    InvalidSetup(String),
    #[error("invalid action: {0}")]
    InvalidAction(String),
    #[error("not player's turn")]
    NotPlayersTurn,
    #[error("player not in match")]
    UnknownPlayer,
    #[error("action not allowed in this phase")]
    PhaseMismatch,
}

pub type Result<T> = std::result::Result<T, VibeCheckEngineError>;

// ─── Outcome ───

#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub events: Vec<SpectatorEvent>,
    pub phase_changed: bool,
}

// ─── Config ───

#[derive(Clone, Debug)]
pub struct VibeCheckGameConfig {
    pub player_count: usize,
    pub target_score: i32,
    pub zone_config: ZoneConfig,
}

impl Default for VibeCheckGameConfig {
    fn default() -> Self {
        Self {
            player_count: 4,
            target_score: 10,
            zone_config: ZoneConfig::default(),
        }
    }
}

// ─── Built-in spectrum cards ───

fn load_spectrum_cards() -> std::result::Result<Vec<SpectrumCard>, serde_json::Error> {
    let json = include_str!("../resources/spectrum_cards.json");
    serde_json::from_str(json)
}

// ─── Scoring helpers ───

pub fn compute_zone(guess: f64, target: f64, config: &ZoneConfig) -> ScoringZone {
    let distance = (guess - target).abs();
    if distance <= config.bullseye_half_width {
        ScoringZone::Bullseye
    } else if distance <= config.bullseye_half_width + config.near_half_width {
        ScoringZone::Near
    } else if distance
        <= config.bullseye_half_width + config.near_half_width + config.far_half_width
    {
        ScoringZone::Far
    } else {
        ScoringZone::Miss
    }
}

/// Returns whether the steal direction guess is correct.
/// Left is correct when target < active team's guess, Right when target >= active guess.
pub fn is_steal_correct(direction: StealDirection, target_pos: f64, active_guess: f64) -> bool {
    match direction {
        StealDirection::Left => target_pos < active_guess,
        StealDirection::Right => target_pos >= active_guess,
    }
}

fn clamp_position(pos: f64) -> f64 {
    pos.clamp(0.0, 1.0)
}

/// Majority vote for steal direction. Ties broken by Left (deterministic).
fn majority_steal_direction(steals: &HashMap<PlayerId, StealDirection>) -> StealDirection {
    let left_count = steals
        .values()
        .filter(|d| **d == StealDirection::Left)
        .count();
    let right_count = steals
        .values()
        .filter(|d| **d == StealDirection::Right)
        .count();
    if right_count > left_count {
        StealDirection::Right
    } else {
        StealDirection::Left
    }
}

// ─── Clue validation ───

#[derive(Debug, Error)]
pub enum ClueViolation {
    #[error("clue is empty")]
    Empty,
    #[error("clue exceeds 200 characters")]
    TooLong,
    #[error("clue exceeds 5 words")]
    TooManyWords,
    #[error("clue contains numbers")]
    ContainsNumbers,
    #[error("clue contains a spectrum endpoint word")]
    ContainsEndpointWord,
    #[error("clue contains a positional/location word")]
    ContainsPositionalWord,
}

const POSITIONAL_WORDS: &[&str] = &[
    "middle",
    "center",
    "centre",
    "halfway",
    "midpoint",
    "midway",
    "left",
    "right",
    "far",
    "near",
    "close",
    "extreme",
    "edge",
    "slight",
    "slightly",
    "mostly",
    "almost",
    "between",
    "half",
    "quarter",
    "third",
    "percent",
    "percentage",
];

fn validate_clue(
    clue: &str,
    spectrum: Option<&SpectrumCard>,
) -> std::result::Result<(), ClueViolation> {
    let trimmed = clue.trim();
    if trimmed.is_empty() {
        return Err(ClueViolation::Empty);
    }
    if trimmed.len() > 200 {
        return Err(ClueViolation::TooLong);
    }
    if trimmed.split_whitespace().count() > 5 {
        return Err(ClueViolation::TooManyWords);
    }
    if trimmed.chars().any(|c| c.is_ascii_digit()) {
        return Err(ClueViolation::ContainsNumbers);
    }
    let clue_lower = trimmed.to_lowercase();
    let clue_words: Vec<&str> = clue_lower.split_whitespace().collect();
    if clue_words
        .iter()
        .any(|w| POSITIONAL_WORDS.contains(&w.trim_matches(|c: char| !c.is_alphabetic())))
    {
        return Err(ClueViolation::ContainsPositionalWord);
    }
    if let Some(card) = spectrum {
        for endpoint in [&card.left_endpoint, &card.right_endpoint] {
            let endpoint_lower = endpoint.to_lowercase();
            let endpoint_words: Vec<&str> = endpoint_lower.split_whitespace().collect();
            for ew in &endpoint_words {
                if clue_words.iter().any(|cw| cw == ew) {
                    return Err(ClueViolation::ContainsEndpointWord);
                }
            }
        }
    }
    Ok(())
}

// ─── Game Engine ───

pub struct VibeCheckGame {
    state: VibeCheckState,
    rng: StdRng,
    deck: Vec<SpectrumCard>,
    deck_index: usize,
    event_history: Vec<SpectatorEvent>,
    config: VibeCheckGameConfig,
}

impl VibeCheckGame {
    pub fn new(player_count: usize, seed: u64) -> Result<Self> {
        let config = VibeCheckGameConfig {
            player_count,
            ..Default::default()
        };
        Self::new_with_config(config, seed)
    }

    pub fn new_with_config(config: VibeCheckGameConfig, seed: u64) -> Result<Self> {
        let player_count = config.player_count;
        if player_count < 4 {
            return Err(VibeCheckEngineError::InvalidSetup(
                "player_count must be at least 4".to_string(),
            ));
        }
        if player_count > 6 {
            return Err(VibeCheckEngineError::InvalidSetup(
                "player_count must be at most 6".to_string(),
            ));
        }
        if !player_count.is_multiple_of(2) {
            return Err(VibeCheckEngineError::InvalidSetup(
                "player_count must be even".to_string(),
            ));
        }

        let mut rng = StdRng::seed_from_u64(seed);
        let half = player_count / 2;

        // Build teams
        let team_a_ids: Vec<PlayerId> = (0..half as i32).collect();
        let team_b_ids: Vec<PlayerId> = (half as i32..player_count as i32).collect();

        // Non-starting team (B) begins with 1 point to compensate for going second
        let teams = vec![
            TeamState {
                team_id: 0,
                score: 0,
                player_ids: team_a_ids.clone(),
            },
            TeamState {
                team_id: 1,
                score: 1,
                player_ids: team_b_ids.clone(),
            },
        ];

        let mut players = Vec::new();
        for id in &team_a_ids {
            players.push(PlayerInfo {
                player_id: *id,
                team: 0,
                display_name: None,
            });
        }
        for id in &team_b_ids {
            players.push(PlayerInfo {
                player_id: *id,
                team: 1,
                display_name: None,
            });
        }

        // Shuffle deck
        let mut deck = load_spectrum_cards().map_err(|e| {
            VibeCheckEngineError::InvalidSetup(format!("failed to load spectrum cards: {e}"))
        })?;
        deck.shuffle(&mut rng);

        // Draw first card and set target
        let first_card = deck[0].clone();
        let target_pos: f64 = rng.gen();

        let cluegiver = team_a_ids[0];

        let state = VibeCheckState {
            round: 1,
            phase: TurnPhase::CluePhase {
                active_team: 0,
                cluegiver,
            },
            teams,
            players,
            spectrum: Some(first_card.clone()),
            target: Some(Target {
                position: target_pos,
            }),
            zone_config: config.zone_config.clone(),
            target_score: config.target_score,
            // Team A starts at rotation index 0 (cluegiver = team_a[0]).
            // Team B's rotation is initialized so that after the first
            // advance_to_next_round increment it wraps to index 0, giving
            // team_b[0] as their first cluegiver (symmetric with Team A).
            cluegiver_rotation: vec![0, half - 1],
            round_history: vec![],
            is_game_over: false,
        };

        let game_started = SpectatorEvent::GameStarted {
            teams: state.teams.clone(),
            players: state.players.clone(),
            target_score: config.target_score,
        };
        let round_started = SpectatorEvent::RoundStarted {
            round: 1,
            active_team: 0,
            cluegiver,
            spectrum: first_card,
        };
        let event_history = vec![game_started, round_started];

        Ok(Self {
            state,
            rng,
            deck,
            deck_index: 1,
            event_history,
            config,
        })
    }

    pub fn state(&self) -> &VibeCheckState {
        &self.state
    }

    pub fn config(&self) -> &VibeCheckGameConfig {
        &self.config
    }

    pub fn is_terminal(&self) -> bool {
        self.state.is_game_over
    }

    pub fn winner(&self) -> Option<TeamId> {
        match &self.state.phase {
            TurnPhase::GameOver { winner } => *winner,
            _ => None,
        }
    }

    pub fn initial_events(&self) -> Vec<SpectatorEvent> {
        self.event_history.clone()
    }

    pub fn push_external_event(&mut self, event: SpectatorEvent) {
        self.event_history.push(event);
    }

    // ─── Legal Actions ───

    pub fn legal_actions(&self, player: &PlayerId) -> Vec<VibeCheckAction> {
        if self.state.is_game_over {
            return vec![];
        }

        // Verify player exists
        if !self.state.players.iter().any(|p| p.player_id == *player) {
            return vec![];
        }

        match &self.state.phase {
            TurnPhase::CluePhase { cluegiver, .. } => {
                if *player == *cluegiver {
                    // Cluegiver can give a clue or forfeit
                    vec![
                        VibeCheckAction::GiveClue {
                            clue: String::new(),
                        },
                        VibeCheckAction::Forfeit,
                    ]
                } else {
                    vec![]
                }
            }
            TurnPhase::GuessPhase {
                active_team,
                cluegiver,
                pending_guesses,
                ..
            } => {
                // Only allow guessers who haven't submitted yet
                if pending_guesses.contains_key(player) {
                    return vec![];
                }
                let player_info = self.state.players.iter().find(|p| p.player_id == *player);
                match player_info {
                    Some(info) if info.team == *active_team && info.player_id != *cluegiver => {
                        vec![
                            VibeCheckAction::SubmitGuess { position: 0.5 },
                            VibeCheckAction::Forfeit,
                        ]
                    }
                    _ => vec![],
                }
            }
            TurnPhase::StealPhase {
                stealing_team,
                pending_steals,
                ..
            } => {
                // Only allow stealers who haven't submitted yet
                if pending_steals.contains_key(player) {
                    return vec![];
                }
                let player_info = self.state.players.iter().find(|p| p.player_id == *player);
                match player_info {
                    Some(info) if info.team == *stealing_team => {
                        vec![
                            VibeCheckAction::SubmitStealGuess {
                                direction: StealDirection::Left,
                            },
                            VibeCheckAction::SubmitStealGuess {
                                direction: StealDirection::Right,
                            },
                            VibeCheckAction::Forfeit,
                        ]
                    }
                    _ => vec![],
                }
            }
            TurnPhase::Resolving { .. } | TurnPhase::GameOver { .. } => vec![],
        }
    }

    // ─── Apply Action ───

    pub fn apply_action(
        &mut self,
        player: &PlayerId,
        action: &VibeCheckAction,
    ) -> Result<ApplyOutcome> {
        // Validate player exists
        if !self.state.players.iter().any(|p| p.player_id == *player) {
            return Err(VibeCheckEngineError::UnknownPlayer);
        }

        if self.state.is_game_over {
            return Err(VibeCheckEngineError::PhaseMismatch);
        }

        let mut events = Vec::new();
        let phase_changed;

        match &self.state.phase.clone() {
            TurnPhase::CluePhase {
                active_team,
                cluegiver,
            } => {
                if *player != *cluegiver {
                    return Err(VibeCheckEngineError::NotPlayersTurn);
                }

                match action {
                    VibeCheckAction::GiveClue { clue } => {
                        if let Err(violation) = validate_clue(clue, self.state.spectrum.as_ref()) {
                            debug!(
                                round = self.state.round,
                                cluegiver = *cluegiver,
                                ?violation,
                                "clue violation — skipping round with 0 points"
                            );
                            // Violation: 0 points, skip to next round
                            self.skip_round_violation(*active_team, *cluegiver, clue, &mut events);
                            phase_changed = true;
                        } else {
                            let trimmed_clue = clue.trim().to_string();
                            events.push(SpectatorEvent::ClueGiven {
                                round: self.state.round,
                                cluegiver: *cluegiver,
                                clue: trimmed_clue.clone(),
                            });
                            self.state.phase = TurnPhase::GuessPhase {
                                active_team: *active_team,
                                cluegiver: *cluegiver,
                                clue: trimmed_clue,
                                pending_guesses: HashMap::new(),
                            };
                            phase_changed = true;
                        }
                    }
                    VibeCheckAction::Forfeit => {
                        self.skip_round_violation(*active_team, *cluegiver, "forfeit", &mut events);
                        phase_changed = true;
                    }
                    _ => {
                        return Err(VibeCheckEngineError::InvalidAction(
                            "expected GiveClue or Forfeit in CluePhase".to_string(),
                        ));
                    }
                }
            }
            TurnPhase::GuessPhase {
                active_team,
                cluegiver,
                clue,
                pending_guesses,
            } => {
                // Must be a non-cluegiver on the active team
                let player_info = self
                    .state
                    .players
                    .iter()
                    .find(|p| p.player_id == *player)
                    .ok_or(VibeCheckEngineError::UnknownPlayer)?;

                if player_info.team != *active_team || *player == *cluegiver {
                    return Err(VibeCheckEngineError::NotPlayersTurn);
                }

                // Reject if player already submitted
                if pending_guesses.contains_key(player) {
                    return Err(VibeCheckEngineError::InvalidAction(
                        "player already submitted a guess".to_string(),
                    ));
                }

                // Count total guessers on this team (non-cluegiver)
                let total_guessers = self
                    .state
                    .teams
                    .iter()
                    .find(|t| t.team_id == *active_team)
                    .map(|t| {
                        t.player_ids
                            .iter()
                            .filter(|pid| **pid != *cluegiver)
                            .count()
                    })
                    .unwrap_or(0);

                let guess_position = match action {
                    VibeCheckAction::SubmitGuess { position } => clamp_position(*position),
                    // Forfeit contributes the spectrum midpoint (0.5) to the team
                    // average. This is intentional: a forfeiting player pushes
                    // the guess toward center rather than being excluded, which
                    // avoids giving an advantage to a team that lost a member.
                    VibeCheckAction::Forfeit => 0.5,
                    _ => {
                        return Err(VibeCheckEngineError::InvalidAction(
                            "expected SubmitGuess or Forfeit in GuessPhase".to_string(),
                        ));
                    }
                };

                let mut updated_guesses = pending_guesses.clone();
                updated_guesses.insert(*player, guess_position);

                events.push(SpectatorEvent::PlayerGuessSubmitted {
                    round: self.state.round,
                    player: *player,
                    position: guess_position,
                });

                if updated_guesses.len() >= total_guessers {
                    let avg_position: f64 =
                        updated_guesses.values().sum::<f64>() / updated_guesses.len() as f64;
                    let avg_clamped = clamp_position(avg_position);
                    let stealing_team = 1 - *active_team;

                    events.push(SpectatorEvent::GuessSubmitted {
                        round: self.state.round,
                        team: *active_team,
                        position: avg_clamped,
                    });

                    self.state.phase = TurnPhase::StealPhase {
                        active_team: *active_team,
                        stealing_team,
                        clue: clue.clone(),
                        active_guess: avg_clamped,
                        pending_steals: HashMap::new(),
                    };
                    phase_changed = true;
                } else {
                    self.state.phase = TurnPhase::GuessPhase {
                        active_team: *active_team,
                        cluegiver: *cluegiver,
                        clue: clue.clone(),
                        pending_guesses: updated_guesses,
                    };
                    phase_changed = false;
                }
            }
            TurnPhase::StealPhase {
                active_team,
                stealing_team,
                clue,
                active_guess,
                pending_steals,
            } => {
                let player_info = self
                    .state
                    .players
                    .iter()
                    .find(|p| p.player_id == *player)
                    .ok_or(VibeCheckEngineError::UnknownPlayer)?;

                if player_info.team != *stealing_team {
                    return Err(VibeCheckEngineError::NotPlayersTurn);
                }

                // Reject if player already submitted
                if pending_steals.contains_key(player) {
                    return Err(VibeCheckEngineError::InvalidAction(
                        "player already submitted a steal guess".to_string(),
                    ));
                }

                // Count total stealers on the stealing team
                let total_stealers = self
                    .state
                    .teams
                    .iter()
                    .find(|t| t.team_id == *stealing_team)
                    .map(|t| t.player_ids.len())
                    .unwrap_or(0);

                let steal_direction = match action {
                    VibeCheckAction::SubmitStealGuess { direction } => *direction,
                    VibeCheckAction::Forfeit => {
                        // Pick the guaranteed wrong direction relative to the guess
                        let target_pos = self
                            .state
                            .target
                            .as_ref()
                            .expect("target must be set during gameplay")
                            .position;
                        if target_pos < *active_guess {
                            StealDirection::Right
                        } else {
                            StealDirection::Left
                        }
                    }
                    _ => {
                        return Err(VibeCheckEngineError::InvalidAction(
                            "expected SubmitStealGuess or Forfeit in StealPhase".to_string(),
                        ));
                    }
                };

                let mut updated_steals = pending_steals.clone();
                updated_steals.insert(*player, steal_direction);

                events.push(SpectatorEvent::PlayerStealSubmitted {
                    round: self.state.round,
                    player: *player,
                    direction: steal_direction,
                });

                if updated_steals.len() >= total_stealers {
                    let final_direction = majority_steal_direction(&updated_steals);

                    events.push(SpectatorEvent::StealGuessSubmitted {
                        round: self.state.round,
                        team: *stealing_team,
                        direction: final_direction,
                    });

                    self.resolve_round(
                        *active_team,
                        *stealing_team,
                        clue,
                        *active_guess,
                        final_direction,
                        &mut events,
                    )?;
                    phase_changed = true;
                } else {
                    self.state.phase = TurnPhase::StealPhase {
                        active_team: *active_team,
                        stealing_team: *stealing_team,
                        clue: clue.clone(),
                        active_guess: *active_guess,
                        pending_steals: updated_steals,
                    };
                    phase_changed = false;
                }
            }
            TurnPhase::Resolving { .. } | TurnPhase::GameOver { .. } => {
                return Err(VibeCheckEngineError::PhaseMismatch);
            }
        }

        self.event_history.extend(events.iter().cloned());

        Ok(ApplyOutcome {
            events,
            phase_changed,
        })
    }

    // ─── Round resolution (auto-resolve, never stays in Resolving) ───

    fn resolve_round(
        &mut self,
        active_team: TeamId,
        stealing_team: TeamId,
        clue: &str,
        active_guess: f64,
        steal_direction: StealDirection,
        events: &mut Vec<SpectatorEvent>,
    ) -> Result<()> {
        let target_pos = self
            .state
            .target
            .as_ref()
            .expect("target must be set during gameplay")
            .position;

        let active_zone = compute_zone(active_guess, target_pos, &self.state.zone_config);
        // Bullseye negates steal: if active team hits Bullseye, steal is always incorrect
        let steal_correct = if active_zone == ScoringZone::Bullseye {
            false
        } else {
            is_steal_correct(steal_direction, target_pos, active_guess)
        };
        let steal_points = if steal_correct { 1 } else { 0 };

        let active_points = active_zone.points();

        // Get current cluegiver from rotation
        let cluegiver = self.current_cluegiver(active_team);

        events.push(SpectatorEvent::TargetRevealed {
            round: self.state.round,
            target_position: target_pos,
            active_zone: active_zone.clone(),
            steal_correct,
        });

        // Award points
        self.state
            .teams
            .get_mut(active_team as usize)
            .ok_or_else(|| {
                VibeCheckEngineError::InvalidAction(format!("invalid team id: {active_team}"))
            })?
            .score += active_points;
        self.state
            .teams
            .get_mut(stealing_team as usize)
            .ok_or_else(|| {
                VibeCheckEngineError::InvalidAction(format!("invalid team id: {stealing_team}"))
            })?
            .score += steal_points;

        let scores: Vec<(TeamId, i32)> = self
            .state
            .teams
            .iter()
            .map(|t| (t.team_id, t.score))
            .collect();

        events.push(SpectatorEvent::ScoreUpdate {
            round: self.state.round,
            active_team,
            active_points,
            steal_team: stealing_team,
            steal_points,
            scores: scores.clone(),
        });

        // Record round result
        let spectrum = self
            .state
            .spectrum
            .clone()
            .expect("spectrum must be set during gameplay");
        self.state.round_history.push(RoundResult {
            round: self.state.round,
            spectrum,
            target_position: target_pos,
            clue: clue.to_string(),
            cluegiver,
            active_team,
            active_guess,
            active_zone,
            active_points,
            steal_direction,
            steal_correct,
            steal_points,
        });

        // Check win condition
        let active_score = self.state.teams[active_team as usize].score;
        let steal_score = self.state.teams[stealing_team as usize].score;
        let target = self.state.target_score;

        if active_score >= target || steal_score >= target {
            let winner = if active_score >= target && steal_score >= target {
                // Both crossed: active team resolves first, wins if strictly ahead
                if active_score > steal_score {
                    Some(active_team)
                } else if steal_score > active_score {
                    Some(stealing_team)
                } else {
                    None // draw
                }
            } else if active_score >= target {
                Some(active_team)
            } else {
                Some(stealing_team)
            };

            let final_scores = scores;
            events.push(SpectatorEvent::GameOver {
                winner,
                final_scores,
            });

            self.state.phase = TurnPhase::GameOver { winner };
            self.state.is_game_over = true;
        } else {
            // Extra turn rule: if active team scored Bullseye (4pts) but is
            // still behind, the same team gets another turn.
            let next_team = if active_points == 4 && active_score < steal_score {
                active_team
            } else {
                stealing_team
            };
            self.advance_to_next_round(next_team, events);
        }

        Ok(())
    }

    fn skip_round_violation(
        &mut self,
        active_team: TeamId,
        cluegiver: PlayerId,
        clue: &str,
        events: &mut Vec<SpectatorEvent>,
    ) {
        let target_pos = self
            .state
            .target
            .as_ref()
            .expect("target must be set during gameplay")
            .position;

        let spectrum = self
            .state
            .spectrum
            .clone()
            .expect("spectrum must be set during gameplay");

        let stealing_team = 1 - active_team;

        // Record with 0 points — no real guesses happened.
        // Use a guaranteed-wrong direction for the forfeit steal (relative to
        // the dummy active_guess of 0.0).
        let dummy_guess = 0.0;
        let forfeit_direction = if target_pos < dummy_guess {
            StealDirection::Right
        } else {
            StealDirection::Left
        };
        self.state.round_history.push(RoundResult {
            round: self.state.round,
            spectrum,
            target_position: target_pos,
            clue: clue.to_string(),
            cluegiver,
            active_team,
            active_guess: 0.0,
            active_zone: ScoringZone::Miss,
            active_points: 0,
            steal_direction: forfeit_direction,
            steal_correct: false,
            steal_points: 0,
        });

        // Emit TargetRevealed before ScoreUpdate, matching the normal resolve flow.
        events.push(SpectatorEvent::TargetRevealed {
            round: self.state.round,
            target_position: target_pos,
            active_zone: ScoringZone::Miss,
            steal_correct: false,
        });

        let scores: Vec<(TeamId, i32)> = self
            .state
            .teams
            .iter()
            .map(|t| (t.team_id, t.score))
            .collect();

        events.push(SpectatorEvent::ScoreUpdate {
            round: self.state.round,
            active_team,
            active_points: 0,
            steal_team: stealing_team,
            steal_points: 0,
            scores,
        });

        // Next round (stealing_team becomes the new active team)
        self.advance_to_next_round(stealing_team, events);
    }

    fn advance_to_next_round(
        &mut self,
        next_active_team: TeamId,
        events: &mut Vec<SpectatorEvent>,
    ) {
        self.state.round += 1;

        // Rotate cluegiver for the next active team
        let team_players = &self.state.teams[next_active_team as usize].player_ids;
        let rotation_idx = &mut self.state.cluegiver_rotation[next_active_team as usize];
        *rotation_idx = (*rotation_idx + 1) % team_players.len();
        let next_cluegiver = team_players[*rotation_idx];

        // Draw next card
        let next_card = self.draw_next_card();

        // Set new target
        let target_pos: f64 = self.rng.gen();

        self.state.spectrum = Some(next_card.clone());
        self.state.target = Some(Target {
            position: target_pos,
        });
        self.state.phase = TurnPhase::CluePhase {
            active_team: next_active_team,
            cluegiver: next_cluegiver,
        };

        events.push(SpectatorEvent::RoundStarted {
            round: self.state.round,
            active_team: next_active_team,
            cluegiver: next_cluegiver,
            spectrum: next_card,
        });
    }

    fn draw_next_card(&mut self) -> SpectrumCard {
        if self.deck_index >= self.deck.len() {
            // Reshuffle
            self.deck.shuffle(&mut self.rng);
            self.deck_index = 0;
            debug!("deck exhausted, reshuffled");
        }
        let card = self.deck[self.deck_index].clone();
        self.deck_index += 1;
        card
    }

    fn current_cluegiver(&self, team: TeamId) -> PlayerId {
        let team_players = &self.state.teams[team as usize].player_ids;
        let rotation_idx = self.state.cluegiver_rotation[team as usize];
        team_players[rotation_idx]
    }
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: create a 4-player game with a known seed
    fn make_game() -> VibeCheckGame {
        VibeCheckGame::new(4, 42).unwrap()
    }

    fn make_game_with_target_score(target: i32) -> VibeCheckGame {
        let config = VibeCheckGameConfig {
            player_count: 4,
            target_score: target,
            ..Default::default()
        };
        VibeCheckGame::new_with_config(config, 42).unwrap()
    }

    // ─── 1. Game creation — correct team assignment, starting phase ───

    #[test]
    fn test_game_creation_4_players() {
        let game = make_game();
        let state = game.state();

        assert_eq!(state.round, 1);
        assert_eq!(state.teams.len(), 2);
        assert_eq!(state.teams[0].team_id, 0);
        assert_eq!(state.teams[0].player_ids, vec![0, 1]);
        assert_eq!(state.teams[1].team_id, 1);
        assert_eq!(state.teams[1].player_ids, vec![2, 3]);
        assert_eq!(state.players.len(), 4);
        assert!(state.spectrum.is_some());
        assert!(state.target.is_some());
        assert!(!state.is_game_over);

        // Should start in CluePhase with team 0's first player as cluegiver
        match &state.phase {
            TurnPhase::CluePhase {
                active_team,
                cluegiver,
            } => {
                assert_eq!(*active_team, 0);
                assert_eq!(*cluegiver, 0);
            }
            _ => panic!("expected CluePhase, got {:?}", state.phase),
        }
    }

    #[test]
    fn test_game_creation_6_players() {
        let game = VibeCheckGame::new(6, 99).unwrap();
        let state = game.state();
        assert_eq!(state.teams[0].player_ids, vec![0, 1, 2]);
        assert_eq!(state.teams[1].player_ids, vec![3, 4, 5]);
        assert_eq!(state.players.len(), 6);
    }

    #[test]
    fn test_game_creation_invalid_odd() {
        assert!(VibeCheckGame::new(3, 0).is_err());
    }

    #[test]
    fn test_game_creation_invalid_too_few() {
        assert!(VibeCheckGame::new(2, 0).is_err());
    }

    #[test]
    fn test_game_creation_invalid_too_many() {
        assert!(VibeCheckGame::new(8, 0).is_err());
    }

    // ─── 2. Full round flow — Clue → Guess → Steal → auto-resolve → next round ───

    #[test]
    fn test_full_round_flow() {
        let mut game = make_game();

        // CluePhase: player 0 (cluegiver) gives clue
        let outcome = game
            .apply_action(
                &0,
                &VibeCheckAction::GiveClue {
                    clue: "lukewarm".to_string(),
                },
            )
            .unwrap();
        assert!(outcome.phase_changed);
        assert!(matches!(game.state().phase, TurnPhase::GuessPhase { .. }));

        // GuessPhase: player 1 (guesser on team 0) submits guess
        let outcome = game
            .apply_action(&1, &VibeCheckAction::SubmitGuess { position: 0.5 })
            .unwrap();
        assert!(outcome.phase_changed);
        assert!(matches!(game.state().phase, TurnPhase::StealPhase { .. }));

        // StealPhase: player 2 (team 1) submits steal direction guess — first stealer
        let outcome = game
            .apply_action(
                &2,
                &VibeCheckAction::SubmitStealGuess {
                    direction: StealDirection::Right,
                },
            )
            .unwrap();
        assert!(
            !outcome.phase_changed,
            "first stealer should not resolve round"
        );

        // StealPhase: player 3 (team 1) submits steal direction guess — second stealer
        let outcome = game
            .apply_action(
                &3,
                &VibeCheckAction::SubmitStealGuess {
                    direction: StealDirection::Right,
                },
            )
            .unwrap();
        assert!(outcome.phase_changed, "second stealer should resolve round");

        // Should auto-resolve and advance to next round (not stay in Resolving)
        assert!(
            !matches!(game.state().phase, TurnPhase::Resolving { .. }),
            "should NOT stay in Resolving phase"
        );

        // Should be in CluePhase for round 2 (team 1's turn) or GameOver
        if !game.is_terminal() {
            assert_eq!(game.state().round, 2);
            match &game.state().phase {
                TurnPhase::CluePhase { active_team, .. } => {
                    assert_eq!(*active_team, 1);
                }
                _ => panic!(
                    "expected CluePhase for round 2, got {:?}",
                    game.state().phase
                ),
            }
        }

        // Verify round history was recorded
        assert_eq!(game.state().round_history.len(), 1);
    }

    // ─── 3. Scoring zones — boundary conditions ───

    #[test]
    fn test_scoring_zone_bullseye() {
        let config = ZoneConfig::default();
        // Exactly on target
        assert_eq!(compute_zone(0.5, 0.5, &config), ScoringZone::Bullseye);
        // Well within bullseye (0.03 away, bullseye_half_width = 0.04)
        assert_eq!(compute_zone(0.53, 0.5, &config), ScoringZone::Bullseye);
        assert_eq!(compute_zone(0.47, 0.5, &config), ScoringZone::Bullseye);
    }

    #[test]
    fn test_scoring_zone_near() {
        let config = ZoneConfig::default();
        // Just past bullseye edge (0.041 away → Near)
        assert_eq!(compute_zone(0.541, 0.5, &config), ScoringZone::Near);
        // At edge of near (0.12 = 0.04 + 0.08 away)
        assert_eq!(compute_zone(0.62, 0.5, &config), ScoringZone::Near);
    }

    #[test]
    fn test_scoring_zone_far() {
        let config = ZoneConfig::default();
        // Just past near edge
        assert_eq!(compute_zone(0.621, 0.5, &config), ScoringZone::Far);
        // At edge of far (0.24 = 0.04 + 0.08 + 0.12 away)
        assert_eq!(compute_zone(0.74, 0.5, &config), ScoringZone::Far);
    }

    #[test]
    fn test_scoring_zone_miss() {
        let config = ZoneConfig::default();
        // Past far edge
        assert_eq!(compute_zone(0.741, 0.5, &config), ScoringZone::Miss);
        assert_eq!(compute_zone(0.0, 0.5, &config), ScoringZone::Miss);
        assert_eq!(compute_zone(1.0, 0.5, &config), ScoringZone::Miss);
    }

    #[test]
    fn test_scoring_zone_boundary_exact() {
        let config = ZoneConfig::default();
        // Use target=0.0 to avoid floating-point addition on both sides.
        // compute_zone computes (guess - 0.0).abs() = guess exactly.
        // bullseye_half_width = 0.04
        assert_eq!(compute_zone(0.04, 0.0, &config), ScoringZone::Bullseye);
        // near boundary = 0.04 + 0.08 = 0.12
        assert_eq!(compute_zone(0.12, 0.0, &config), ScoringZone::Near);
        // far boundary = 0.04 + 0.08 + 0.12 = 0.24
        assert_eq!(compute_zone(0.24, 0.0, &config), ScoringZone::Far);
        // Just past far
        assert_eq!(compute_zone(0.25, 0.0, &config), ScoringZone::Miss);
    }

    // ─── 4. Steal scoring — direction relative to active team's guess ───

    #[test]
    fn test_is_steal_correct_relative_to_guess() {
        // active_guess = 0.5: target 0.3 is left → Left correct
        assert!(is_steal_correct(StealDirection::Left, 0.3, 0.5));
        // active_guess = 0.5: target 0.7 is right → Right correct
        assert!(is_steal_correct(StealDirection::Right, 0.7, 0.5));
        // active_guess = 0.5: target == guess → Right correct (>=)
        assert!(is_steal_correct(StealDirection::Right, 0.5, 0.5));
        // active_guess = 0.5: target == guess → Left wrong
        assert!(!is_steal_correct(StealDirection::Left, 0.5, 0.5));

        // active_guess = 0.3: target 0.2 left of guess → Left correct
        assert!(is_steal_correct(StealDirection::Left, 0.2, 0.3));
        // active_guess = 0.3: target 0.7 right of guess → Right correct
        assert!(is_steal_correct(StealDirection::Right, 0.7, 0.3));
        // active_guess = 0.3: target 0.4 right of guess → Right correct
        assert!(is_steal_correct(StealDirection::Right, 0.4, 0.3));
        // Wrong directions
        assert!(!is_steal_correct(StealDirection::Right, 0.2, 0.5));
        assert!(!is_steal_correct(StealDirection::Left, 0.8, 0.5));
    }

    #[test]
    fn test_bullseye_negates_steal() {
        // If active team hits bullseye, steal is always denied even if direction is correct.
        // This is tested through the full game engine flow.
        let config = VibeCheckGameConfig {
            player_count: 4,
            target_score: 100,
            zone_config: ZoneConfig::default(),
        };
        let mut game = VibeCheckGame::new_with_config(config, 42).unwrap();
        let target = game.state().target.as_ref().unwrap().position;

        // Give clue
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        // Guess exactly on target → Bullseye
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: target })
            .unwrap();
        // Steal with "correct" direction — should still get 0 due to bullseye negation
        let correct_direction = if target >= target {
            StealDirection::Right
        } else {
            StealDirection::Left
        };
        let outcome = game
            .apply_action(
                &2,
                &VibeCheckAction::SubmitStealGuess {
                    direction: correct_direction,
                },
            )
            .unwrap();
        assert!(
            !outcome.phase_changed,
            "first stealer should not resolve round"
        );
        let outcome = game
            .apply_action(
                &3,
                &VibeCheckAction::SubmitStealGuess {
                    direction: correct_direction,
                },
            )
            .unwrap();
        assert!(outcome.phase_changed, "second stealer should resolve round");

        let last = game.state().round_history.last().unwrap();
        assert_eq!(last.active_zone, ScoringZone::Bullseye);
        assert!(!last.steal_correct, "bullseye should negate steal");
        assert_eq!(last.steal_points, 0, "bullseye should negate steal points");
    }

    // ─── 5. Win condition — game ends at target_score ───

    #[test]
    fn test_win_condition() {
        // Use target_score of 4 so a single bullseye wins
        let mut game = make_game_with_target_score(4);

        let target = game.state().target.as_ref().unwrap().position;

        // Round 1: team 0 gets bullseye (4 pts), should win
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: target })
            .unwrap();
        // Steal direction doesn't matter — bullseye negates steal
        game.apply_action(
            &2,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();

        assert!(game.is_terminal());
        assert_eq!(game.winner(), Some(0));
    }

    // ─── 6. Simultaneous threshold — both teams cross ───

    #[test]
    fn test_simultaneous_threshold_active_wins_if_higher() {
        // Set up: both teams near target. Active team gets Near (3pts), steal
        // gets correct direction (1pt). Active total > steal total → active wins.
        let config = VibeCheckGameConfig {
            player_count: 4,
            target_score: 3,
            zone_config: ZoneConfig::default(),
        };
        let mut game = VibeCheckGame::new_with_config(config, 42).unwrap();

        // Set both teams to 2 points (1 away from target)
        game.state.teams[0].score = 2;
        game.state.teams[1].score = 2;

        let target = game.state().target.as_ref().unwrap().position;

        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        // Active team gets Near zone (3 pts → total 5)
        let near_dist = game.state().zone_config.bullseye_half_width + 0.01;
        let near_pos = (target + near_dist).clamp(0.0, 1.0);
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: near_pos })
            .unwrap();
        // Steal team guesses correct direction relative to active guess (1 pt → total 3)
        let correct_direction = if target >= near_pos {
            StealDirection::Right
        } else {
            StealDirection::Left
        };
        game.apply_action(
            &2,
            &VibeCheckAction::SubmitStealGuess {
                direction: correct_direction,
            },
        )
        .unwrap();
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: correct_direction,
            },
        )
        .unwrap();

        assert!(game.is_terminal());
        // Active team has 5, steal team has 3 → active team wins
        assert_eq!(game.winner(), Some(0));
    }

    #[test]
    fn test_simultaneous_threshold_draw() {
        let config = VibeCheckGameConfig {
            player_count: 4,
            target_score: 3,
            zone_config: ZoneConfig::default(),
        };
        let mut game = VibeCheckGame::new_with_config(config, 42).unwrap();

        // Set scores so both will reach target simultaneously:
        // Active gets Far (2 pts) → total 3, steal gets correct direction (1 pt) → total 3
        game.state.teams[0].score = 1;
        game.state.teams[1].score = 2;

        let target = game.state().target.as_ref().unwrap().position;

        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        // Active guess in Far zone (2 pts → total 3)
        let far_dist = game.state().zone_config.bullseye_half_width
            + game.state().zone_config.near_half_width
            + 0.01;
        let far_pos = (target + far_dist).clamp(0.0, 1.0);
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: far_pos })
            .unwrap();
        // Steal team guesses correct direction relative to active guess (1 pt → total 3)
        let correct_direction = if target >= far_pos {
            StealDirection::Right
        } else {
            StealDirection::Left
        };
        game.apply_action(
            &2,
            &VibeCheckAction::SubmitStealGuess {
                direction: correct_direction,
            },
        )
        .unwrap();
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: correct_direction,
            },
        )
        .unwrap();

        assert!(game.is_terminal());
        // Both at 3, equal → draw
        assert_eq!(game.winner(), None);
    }

    // ─── 7. Cluegiver rotation within teams ───

    #[test]
    fn test_cluegiver_rotation() {
        let mut game = VibeCheckGame::new(6, 42).unwrap();
        // 6 players: Team 0 = [0,1,2], Team 1 = [3,4,5]
        // Round 1: team 0 active, cluegiver = 0 (rotation[0] = 0)

        // Equalize scores to prevent the extra-turn rule from triggering
        game.state.teams[0].score = 50;
        game.state.teams[1].score = 50;

        // Complete round 1
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        // Team 0 guessers: players 1 and 2 (player 0 is cluegiver)
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: 0.5 })
            .unwrap();
        game.apply_action(&2, &VibeCheckAction::SubmitGuess { position: 0.5 })
            .unwrap();
        // Team 1 steals: all 3 players (3,4,5) must submit
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Right,
            },
        )
        .unwrap();
        game.apply_action(
            &4,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Right,
            },
        )
        .unwrap();
        game.apply_action(
            &5,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Right,
            },
        )
        .unwrap();

        if !game.is_terminal() {
            // Round 2: team 1 active, cluegiver should be team 1's first player (index 0 = player 3)
            // Rotation was initialized to len-1=2, advance wraps (2+1)%3 = 0 → team_b[0] = player 3
            match &game.state().phase {
                TurnPhase::CluePhase {
                    active_team,
                    cluegiver,
                } => {
                    assert_eq!(*active_team, 1);
                    assert_eq!(*cluegiver, 3); // symmetric with team 0: starts at index 0
                }
                _ => panic!("expected CluePhase"),
            }

            // Complete round 2
            game.apply_action(
                &3,
                &VibeCheckAction::GiveClue {
                    clue: "test2".to_string(),
                },
            )
            .unwrap();
            // Team 1 guessers: players 4 and 5 (player 3 is cluegiver)
            game.apply_action(&4, &VibeCheckAction::SubmitGuess { position: 0.5 })
                .unwrap();
            game.apply_action(&5, &VibeCheckAction::SubmitGuess { position: 0.5 })
                .unwrap();
            // Team 0 steals: all 3 players (0,1,2) must submit
            game.apply_action(
                &0,
                &VibeCheckAction::SubmitStealGuess {
                    direction: StealDirection::Right,
                },
            )
            .unwrap();
            game.apply_action(
                &1,
                &VibeCheckAction::SubmitStealGuess {
                    direction: StealDirection::Right,
                },
            )
            .unwrap();
            game.apply_action(
                &2,
                &VibeCheckAction::SubmitStealGuess {
                    direction: StealDirection::Right,
                },
            )
            .unwrap();

            if !game.is_terminal() {
                // Round 3: team 0 active again, cluegiver should rotate: idx 0→1 → player 1
                match &game.state().phase {
                    TurnPhase::CluePhase {
                        active_team,
                        cluegiver,
                    } => {
                        assert_eq!(*active_team, 0);
                        assert_eq!(*cluegiver, 1);
                    }
                    _ => panic!("expected CluePhase"),
                }
            }
        }
    }

    // ─── 8. Clue validation ───

    #[test]
    fn test_clue_validation_empty() {
        assert!(matches!(validate_clue("", None), Err(ClueViolation::Empty)));
        assert!(matches!(
            validate_clue("   ", None),
            Err(ClueViolation::Empty)
        ));
    }

    #[test]
    fn test_clue_validation_too_long() {
        let long_clue = "a".repeat(201);
        assert!(matches!(
            validate_clue(&long_clue, None),
            Err(ClueViolation::TooLong)
        ));
    }

    #[test]
    fn test_clue_validation_too_many_words() {
        assert!(matches!(
            validate_clue("one two three four five six", None),
            Err(ClueViolation::TooManyWords)
        ));
    }

    #[test]
    fn test_clue_validation_contains_numbers() {
        assert!(matches!(
            validate_clue("warm3", None),
            Err(ClueViolation::ContainsNumbers)
        ));
        assert!(matches!(
            validate_clue("7up", None),
            Err(ClueViolation::ContainsNumbers)
        ));
    }

    #[test]
    fn test_clue_validation_contains_endpoint_word() {
        let card = SpectrumCard {
            left_endpoint: "Hot".to_string(),
            right_endpoint: "Cold".to_string(),
            category: None,
        };
        assert!(matches!(
            validate_clue("hot", Some(&card)),
            Err(ClueViolation::ContainsEndpointWord)
        ));
        assert!(matches!(
            validate_clue("Cold", Some(&card)),
            Err(ClueViolation::ContainsEndpointWord)
        ));
        // Case-insensitive
        assert!(matches!(
            validate_clue("HOT", Some(&card)),
            Err(ClueViolation::ContainsEndpointWord)
        ));
        // Word embedded in phrase
        assert!(matches!(
            validate_clue("very hot", Some(&card)),
            Err(ClueViolation::ContainsEndpointWord)
        ));
        // Non-endpoint word is fine
        assert!(validate_clue("lukewarm", Some(&card)).is_ok());
    }

    #[test]
    fn test_clue_validation_ok() {
        assert!(validate_clue("lukewarm", None).is_ok());
        assert!(validate_clue("kind of warm", None).is_ok());
        assert!(validate_clue("one two three four five", None).is_ok());
    }

    #[test]
    fn test_empty_clue_skips_round() {
        let mut game = make_game();

        let outcome = game
            .apply_action(
                &0,
                &VibeCheckAction::GiveClue {
                    clue: String::new(),
                },
            )
            .unwrap();

        assert!(outcome.phase_changed);
        // Should skip to next round with 0 points
        assert_eq!(game.state().round, 2);
        assert_eq!(game.state().teams[0].score, 0);
        assert_eq!(game.state().round_history.len(), 1);
        assert_eq!(game.state().round_history[0].active_points, 0);

        // Verify TargetRevealed is emitted on violation skip (matching normal resolve flow)
        assert!(outcome
            .events
            .iter()
            .any(|e| matches!(e, SpectatorEvent::TargetRevealed { .. })));
    }

    #[test]
    fn test_too_many_words_clue_skips_round() {
        let mut game = make_game();

        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "one two three four five six".to_string(),
            },
        )
        .unwrap();

        assert_eq!(game.state().round, 2);
        assert_eq!(game.state().teams[0].score, 0);
    }

    // ─── 9. Invalid actions — wrong player, wrong phase ───

    #[test]
    fn test_wrong_player_in_clue_phase() {
        let mut game = make_game();
        // Player 1 is not the cluegiver
        let result = game.apply_action(
            &1,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_action_in_clue_phase() {
        let mut game = make_game();
        // Can't submit guess in clue phase
        let result = game.apply_action(&0, &VibeCheckAction::SubmitGuess { position: 0.5 });
        assert!(result.is_err());
    }

    #[test]
    fn test_cluegiver_cant_guess() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();

        // Player 0 is cluegiver, cannot guess
        let result = game.apply_action(&0, &VibeCheckAction::SubmitGuess { position: 0.5 });
        assert!(result.is_err());
    }

    #[test]
    fn test_opposing_team_cant_guess() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();

        // Player 2 is on team 1, can't guess for team 0
        let result = game.apply_action(&2, &VibeCheckAction::SubmitGuess { position: 0.5 });
        assert!(result.is_err());
    }

    #[test]
    fn test_active_team_cant_steal() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: 0.5 })
            .unwrap();

        // Player 0 is on team 0 (active), can't steal
        let result = game.apply_action(
            &0,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Right,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_player() {
        let mut game = make_game();
        let result = game.apply_action(
            &99,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_action_after_game_over() {
        let mut game = make_game_with_target_score(4);
        let target = game.state().target.as_ref().unwrap().position;

        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: target })
            .unwrap();
        // Bullseye negates steal, direction doesn't matter
        game.apply_action(
            &2,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();

        assert!(game.is_terminal());

        let result = game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        );
        assert!(result.is_err());
    }

    // ─── 10. Position clamping ───

    #[test]
    fn test_position_clamping_above() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();

        // Position > 1.0 should be clamped
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: 1.5 })
            .unwrap();

        match &game.state().phase {
            TurnPhase::StealPhase { active_guess, .. } => {
                assert!(*active_guess <= 1.0);
            }
            _ => panic!("expected StealPhase"),
        }
    }

    #[test]
    fn test_position_clamping_below() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();

        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: -0.5 })
            .unwrap();

        match &game.state().phase {
            TurnPhase::StealPhase { active_guess, .. } => {
                assert!(*active_guess >= 0.0);
            }
            _ => panic!("expected StealPhase"),
        }
    }

    // ─── 11. Deck exhaustion and reshuffle ───

    #[test]
    fn test_deck_reshuffle() {
        // We have 100 spectrum cards. Play 101 rounds to force a reshuffle.
        let config = VibeCheckGameConfig {
            player_count: 4,
            target_score: 10000, // Very high so game doesn't end
            zone_config: ZoneConfig::default(),
        };
        let mut game = VibeCheckGame::new_with_config(config, 42).unwrap();

        // Play many rounds
        for _ in 0..101 {
            if game.is_terminal() {
                break;
            }
            let phase = game.state().phase.clone();
            match phase {
                TurnPhase::CluePhase { cluegiver, .. } => {
                    game.apply_action(
                        &cluegiver,
                        &VibeCheckAction::GiveClue {
                            clue: "test".to_string(),
                        },
                    )
                    .unwrap();
                }
                _ => panic!("expected CluePhase at start of iteration"),
            }

            // Get the guessing player
            let phase = game.state().phase.clone();
            match phase {
                TurnPhase::GuessPhase {
                    active_team,
                    cluegiver,
                    ..
                } => {
                    let guesser = game.state().teams[active_team as usize]
                        .player_ids
                        .iter()
                        .find(|p| **p != cluegiver)
                        .copied()
                        .unwrap();
                    game.apply_action(&guesser, &VibeCheckAction::SubmitGuess { position: 0.5 })
                        .unwrap();
                }
                _ => panic!("expected GuessPhase"),
            }

            let phase = game.state().phase.clone();
            match phase {
                TurnPhase::StealPhase { stealing_team, .. } => {
                    let steal_players = game.state().teams[stealing_team as usize]
                        .player_ids
                        .clone();
                    for stealer in &steal_players {
                        game.apply_action(
                            stealer,
                            &VibeCheckAction::SubmitStealGuess {
                                direction: StealDirection::Right,
                            },
                        )
                        .unwrap();
                    }
                }
                _ => panic!("expected StealPhase"),
            }
        }

        // If we got here without panicking, deck reshuffle worked
        assert!(game.state().round_history.len() >= 100);
    }

    // ─── 12. Fog of war — target hidden from non-cluegivers ───

    #[test]
    fn test_fog_of_war_clue_phase() {
        let game = make_game();
        let state = game.state();

        // Cluegiver (player 0) should see target
        let filtered = state.filtered_for_player(0);
        assert!(filtered.target.is_some());

        // Others should not
        let filtered = state.filtered_for_player(1);
        assert!(filtered.target.is_none());
        let filtered = state.filtered_for_player(2);
        assert!(filtered.target.is_none());
    }

    #[test]
    fn test_fog_of_war_guess_phase() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();

        let state = game.state();
        let cluegiver = 0; // make_game seats player 0 as the opening Psychic
                           // The Psychic retains target visibility through GuessPhase (until
                           // the round resolves). Every other player — teammates and
                           // opponents alike — sees `target = None`.
        for pid in 0..4 {
            let filtered = state.filtered_for_player(pid);
            if pid == cluegiver {
                assert!(
                    filtered.target.is_some(),
                    "cluegiver should retain target through GuessPhase"
                );
            } else {
                assert!(
                    filtered.target.is_none(),
                    "player {pid} should not see target in GuessPhase"
                );
            }
        }
    }

    // ─── 13. Forfeit action ───

    #[test]
    fn test_forfeit_in_clue_phase() {
        let mut game = make_game();
        let outcome = game.apply_action(&0, &VibeCheckAction::Forfeit).unwrap();
        assert!(outcome.phase_changed);
        assert_eq!(game.state().round, 2);
        assert_eq!(game.state().teams[0].score, 0);
    }

    #[test]
    fn test_forfeit_in_guess_phase() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();

        let outcome = game.apply_action(&1, &VibeCheckAction::Forfeit).unwrap();
        assert!(outcome.phase_changed);
        assert!(matches!(game.state().phase, TurnPhase::StealPhase { .. }));

        // Both stealers on team 1 must submit
        let outcome = game
            .apply_action(
                &2,
                &VibeCheckAction::SubmitStealGuess {
                    direction: StealDirection::Right,
                },
            )
            .unwrap();
        assert!(!outcome.phase_changed);
        let outcome = game
            .apply_action(
                &3,
                &VibeCheckAction::SubmitStealGuess {
                    direction: StealDirection::Right,
                },
            )
            .unwrap();
        assert!(outcome.phase_changed);
        assert_eq!(game.state().round, 2);
        assert_eq!(game.state().teams[0].score, 0);
    }

    #[test]
    fn test_forfeit_in_steal_phase() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: 0.5 })
            .unwrap();

        // First stealer forfeits
        let outcome = game.apply_action(&2, &VibeCheckAction::Forfeit).unwrap();
        assert!(
            !outcome.phase_changed,
            "first stealer forfeit should not resolve round"
        );

        // Second stealer also forfeits
        let outcome = game.apply_action(&3, &VibeCheckAction::Forfeit).unwrap();
        assert!(
            outcome.phase_changed,
            "second stealer forfeit should resolve round"
        );
        // Auto-resolved with steal at opposite end from target (guaranteed miss)
        assert!(!matches!(game.state().phase, TurnPhase::Resolving { .. }));

        // Verify the steal team got 0 points (guaranteed wrong direction on forfeit)
        let last_round = game.state().round_history.last().unwrap();
        assert_eq!(last_round.steal_points, 0);
        assert!(!last_round.steal_correct);
    }

    // ─── 14. legal_actions returns correct actions per phase ───

    #[test]
    fn test_legal_actions_clue_phase() {
        let game = make_game();

        // Cluegiver has actions
        let actions = game.legal_actions(&0);
        assert_eq!(actions.len(), 2); // GiveClue + Forfeit

        // Non-cluegiver has no actions
        let actions = game.legal_actions(&1);
        assert!(actions.is_empty());

        // Opposing team has no actions
        let actions = game.legal_actions(&2);
        assert!(actions.is_empty());

        // Unknown player has no actions
        let actions = game.legal_actions(&99);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_legal_actions_guess_phase() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();

        // Guesser (team 0, not cluegiver) has actions
        let actions = game.legal_actions(&1);
        assert_eq!(actions.len(), 2); // SubmitGuess + Forfeit

        // Cluegiver has no actions
        let actions = game.legal_actions(&0);
        assert!(actions.is_empty());

        // Opposing team has no actions
        let actions = game.legal_actions(&2);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_legal_actions_steal_phase() {
        let mut game = make_game();
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: 0.5 })
            .unwrap();

        // Stealing team has actions: Left, Right, Forfeit
        let actions = game.legal_actions(&2);
        assert_eq!(actions.len(), 3);
        let actions = game.legal_actions(&3);
        assert_eq!(actions.len(), 3);

        // Active team has no actions
        let actions = game.legal_actions(&0);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_legal_actions_game_over() {
        let mut game = make_game_with_target_score(4);
        let target = game.state().target.as_ref().unwrap().position;

        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: target })
            .unwrap();
        // Bullseye negates steal, direction doesn't matter — both stealers must submit
        game.apply_action(
            &2,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();

        assert!(game.is_terminal());
        for pid in 0..4 {
            assert!(game.legal_actions(&pid).is_empty());
        }
    }

    // ─── 15. Deterministic replay — same seed = same game ───

    #[test]
    fn test_deterministic_replay() {
        fn play_round(game: &mut VibeCheckGame) {
            let phase = game.state().phase.clone();
            match phase {
                TurnPhase::CluePhase { cluegiver, .. } => {
                    game.apply_action(
                        &cluegiver,
                        &VibeCheckAction::GiveClue {
                            clue: "same".to_string(),
                        },
                    )
                    .unwrap();
                }
                _ => panic!("expected CluePhase"),
            }

            let phase = game.state().phase.clone();
            match phase {
                TurnPhase::GuessPhase {
                    active_team,
                    cluegiver,
                    ..
                } => {
                    let guesser = game.state().teams[active_team as usize]
                        .player_ids
                        .iter()
                        .find(|p| **p != cluegiver)
                        .copied()
                        .unwrap();
                    game.apply_action(&guesser, &VibeCheckAction::SubmitGuess { position: 0.5 })
                        .unwrap();
                }
                _ => panic!("expected GuessPhase"),
            }

            let phase = game.state().phase.clone();
            match phase {
                TurnPhase::StealPhase { stealing_team, .. } => {
                    let steal_players = game.state().teams[stealing_team as usize]
                        .player_ids
                        .clone();
                    for stealer in &steal_players {
                        game.apply_action(
                            stealer,
                            &VibeCheckAction::SubmitStealGuess {
                                direction: StealDirection::Right,
                            },
                        )
                        .unwrap();
                    }
                }
                _ => panic!("expected StealPhase"),
            }
        }

        let mut game1 = VibeCheckGame::new(4, 12345).unwrap();
        let mut game2 = VibeCheckGame::new(4, 12345).unwrap();

        // Verify initial state is identical
        assert_eq!(
            game1.state().target.as_ref().unwrap().position,
            game2.state().target.as_ref().unwrap().position
        );
        assert_eq!(
            game1.state().spectrum.as_ref().unwrap().left_endpoint,
            game2.state().spectrum.as_ref().unwrap().left_endpoint,
        );

        // Play 3 rounds
        for _ in 0..3 {
            if game1.is_terminal() || game2.is_terminal() {
                break;
            }
            play_round(&mut game1);
            play_round(&mut game2);

            // Scores should match
            assert_eq!(game1.state().teams[0].score, game2.state().teams[0].score);
            assert_eq!(game1.state().teams[1].score, game2.state().teams[1].score);
        }
    }

    // ─── Additional: spectator events emitted correctly ───

    #[test]
    fn test_initial_events() {
        let game = make_game();
        let events = game.initial_events();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], SpectatorEvent::GameStarted { .. }));
        assert!(matches!(events[1], SpectatorEvent::RoundStarted { .. }));
    }

    #[test]
    fn test_push_external_event() {
        let mut game = make_game();
        game.push_external_event(SpectatorEvent::AgentReasoning {
            player: 0,
            reasoning: "thinking...".to_string(),
        });

        let events = game.initial_events();
        assert_eq!(events.len(), 3);
        assert!(matches!(events[2], SpectatorEvent::AgentReasoning { .. }));
    }

    #[test]
    fn test_full_round_emits_correct_events() {
        let mut game = make_game();

        let o1 = game
            .apply_action(
                &0,
                &VibeCheckAction::GiveClue {
                    clue: "warm".to_string(),
                },
            )
            .unwrap();
        assert!(o1
            .events
            .iter()
            .any(|e| matches!(e, SpectatorEvent::ClueGiven { .. })));

        let o2 = game
            .apply_action(&1, &VibeCheckAction::SubmitGuess { position: 0.5 })
            .unwrap();
        assert!(o2
            .events
            .iter()
            .any(|e| matches!(e, SpectatorEvent::GuessSubmitted { .. })));

        let o3 = game
            .apply_action(
                &2,
                &VibeCheckAction::SubmitStealGuess {
                    direction: StealDirection::Right,
                },
            )
            .unwrap();
        // First stealer — should emit PlayerStealSubmitted but NOT resolve yet
        assert!(!o3.phase_changed, "first stealer should not resolve round");
        assert!(o3
            .events
            .iter()
            .any(|e| matches!(e, SpectatorEvent::PlayerStealSubmitted { .. })));

        let o4 = game
            .apply_action(
                &3,
                &VibeCheckAction::SubmitStealGuess {
                    direction: StealDirection::Right,
                },
            )
            .unwrap();
        // Second stealer resolves — should emit: StealGuessSubmitted, TargetRevealed, ScoreUpdate, and possibly RoundStarted or GameOver
        assert!(o4.phase_changed, "second stealer should resolve round");
        assert!(o4
            .events
            .iter()
            .any(|e| matches!(e, SpectatorEvent::StealGuessSubmitted { .. })));
        assert!(o4
            .events
            .iter()
            .any(|e| matches!(e, SpectatorEvent::TargetRevealed { .. })));
        assert!(o4
            .events
            .iter()
            .any(|e| matches!(e, SpectatorEvent::ScoreUpdate { .. })));
    }

    // ─── Config defaults ───

    #[test]
    fn test_config_default() {
        let config = VibeCheckGameConfig::default();
        assert_eq!(config.player_count, 4);
        assert_eq!(config.target_score, 10);
    }

    // ─── Steal team wins if only they cross threshold ───

    #[test]
    fn test_steal_team_wins() {
        let config = VibeCheckGameConfig {
            player_count: 4,
            target_score: 2,
            zone_config: ZoneConfig::default(),
        };
        let mut game = VibeCheckGame::new_with_config(config, 42).unwrap();

        // Set steal team (1) very close to winning, active team (0) at 0
        game.state.teams[1].score = 1;
        game.state.teams[0].score = 0;

        let target = game.state().target.as_ref().unwrap().position;

        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        // Active team misses completely (guess far from target)
        let miss_pos = if target > 0.5 { 0.0 } else { 1.0 };
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: miss_pos })
            .unwrap();
        // Steal team guesses correct direction relative to active guess (1 pt → total 2 ≥ target)
        let correct_direction = if target >= miss_pos {
            StealDirection::Right
        } else {
            StealDirection::Left
        };
        game.apply_action(
            &2,
            &VibeCheckAction::SubmitStealGuess {
                direction: correct_direction,
            },
        )
        .unwrap();
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: correct_direction,
            },
        )
        .unwrap();

        assert!(game.is_terminal());
        assert_eq!(game.winner(), Some(1));
    }

    // ─── Round history captures correct data ───

    #[test]
    fn test_round_history_data() {
        let mut game = make_game();
        let target = game.state().target.as_ref().unwrap().position;
        let spectrum = game.state().spectrum.clone().unwrap();

        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "my clue".to_string(),
            },
        )
        .unwrap();
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: 0.3 })
            .unwrap();
        game.apply_action(
            &2,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Right,
            },
        )
        .unwrap();
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Right,
            },
        )
        .unwrap();

        let history = &game.state().round_history;
        assert_eq!(history.len(), 1);
        let result = &history[0];
        assert_eq!(result.round, 1);
        assert_eq!(result.clue, "my clue");
        assert_eq!(result.cluegiver, 0);
        assert_eq!(result.active_team, 0);
        assert!((result.active_guess - 0.3).abs() < f64::EPSILON);
        assert_eq!(result.steal_direction, StealDirection::Right);
        assert!((result.target_position - target).abs() < f64::EPSILON);
        assert_eq!(result.spectrum, spectrum);
    }

    // ─── Auto-resolve never stays in Resolving ───

    #[test]
    fn test_never_stays_in_resolving() {
        let config = VibeCheckGameConfig {
            player_count: 4,
            target_score: 10000,
            zone_config: ZoneConfig::default(),
        };
        let mut game = VibeCheckGame::new_with_config(config, 42).unwrap();

        for _ in 0..10 {
            if game.is_terminal() {
                break;
            }
            let phase = game.state().phase.clone();
            match phase {
                TurnPhase::CluePhase { cluegiver, .. } => {
                    game.apply_action(
                        &cluegiver,
                        &VibeCheckAction::GiveClue {
                            clue: "test".to_string(),
                        },
                    )
                    .unwrap();
                }
                _ => panic!("expected CluePhase"),
            }

            let phase = game.state().phase.clone();
            match phase {
                TurnPhase::GuessPhase {
                    active_team,
                    cluegiver,
                    ..
                } => {
                    let guesser = game.state().teams[active_team as usize]
                        .player_ids
                        .iter()
                        .find(|p| **p != cluegiver)
                        .copied()
                        .unwrap();
                    game.apply_action(&guesser, &VibeCheckAction::SubmitGuess { position: 0.5 })
                        .unwrap();
                }
                _ => panic!("expected GuessPhase"),
            }

            let phase = game.state().phase.clone();
            match phase {
                TurnPhase::StealPhase { stealing_team, .. } => {
                    let steal_players = game.state().teams[stealing_team as usize]
                        .player_ids
                        .clone();
                    for stealer in &steal_players {
                        game.apply_action(
                            stealer,
                            &VibeCheckAction::SubmitStealGuess {
                                direction: StealDirection::Right,
                            },
                        )
                        .unwrap();
                    }
                }
                _ => panic!("expected StealPhase"),
            }

            // After steal, should NEVER be in Resolving
            assert!(
                !matches!(game.state().phase, TurnPhase::Resolving { .. }),
                "game should never stay in Resolving state after apply_action, round {}",
                game.state().round_history.len()
            );
        }
    }

    // ─── Extra turn: bullseye while behind → same team goes again ───

    #[test]
    fn test_extra_turn_when_behind_after_bullseye() {
        let config = VibeCheckGameConfig {
            player_count: 4,
            target_score: 100,
            zone_config: ZoneConfig::default(),
        };
        let mut game = VibeCheckGame::new_with_config(config, 42).unwrap();

        // Set team 0 behind team 1
        game.state.teams[0].score = 2;
        game.state.teams[1].score = 10;

        let target = game.state().target.as_ref().unwrap().position;

        // Round 1: team 0 active, gets bullseye while behind
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: target })
            .unwrap();
        game.apply_action(
            &2,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();

        // Team 0 got bullseye (4pts → 6) but still behind team 1 (10).
        // Extra turn rule: team 0 should go again.
        assert!(!game.is_terminal());
        match &game.state().phase {
            TurnPhase::CluePhase { active_team, .. } => {
                assert_eq!(
                    *active_team, 0,
                    "team 0 should get extra turn after bullseye while behind"
                );
            }
            other => panic!("expected CluePhase, got {other:?}"),
        }
    }

    #[test]
    fn test_no_extra_turn_when_ahead_after_bullseye() {
        let config = VibeCheckGameConfig {
            player_count: 4,
            target_score: 100,
            zone_config: ZoneConfig::default(),
        };
        let mut game = VibeCheckGame::new_with_config(config, 42).unwrap();

        // Set team 0 ahead of team 1
        game.state.teams[0].score = 10;
        game.state.teams[1].score = 2;

        let target = game.state().target.as_ref().unwrap().position;

        // Round 1: team 0 active, gets bullseye while ahead
        game.apply_action(
            &0,
            &VibeCheckAction::GiveClue {
                clue: "test".to_string(),
            },
        )
        .unwrap();
        game.apply_action(&1, &VibeCheckAction::SubmitGuess { position: target })
            .unwrap();
        game.apply_action(
            &2,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();
        game.apply_action(
            &3,
            &VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left,
            },
        )
        .unwrap();

        // Team 0 got bullseye (4pts → 14) and is ahead. Normal alternation.
        assert!(!game.is_terminal());
        match &game.state().phase {
            TurnPhase::CluePhase { active_team, .. } => {
                assert_eq!(
                    *active_team, 1,
                    "normal alternation when active team is ahead"
                );
            }
            other => panic!("expected CluePhase, got {other:?}"),
        }
    }

    // ─── Initial score balancing: team B starts at 1 ───

    #[test]
    fn test_initial_score_balancing() {
        let game = make_game();
        assert_eq!(game.state().teams[0].score, 0, "team A starts at 0");
        assert_eq!(game.state().teams[1].score, 1, "team B starts at 1");
    }
}
