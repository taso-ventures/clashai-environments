//! Protocol types and shared API models for the Vibe Check environment.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use eval_runtime::{
    EnvironmentAction, EnvironmentState, EnvironmentWinner, SequentialDecisionKind,
    SequentialPhase, SequentialState,
};

// ─── Core Identifiers ───

pub type PlayerId = i32;
pub type TeamId = i32; // 0 = Team A, 1 = Team B

/// Direction guess for the steal phase: is the target to the left or right
/// of the active team's guess position?
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StealDirection {
    Left,
    Right,
}

// ─── Spectrum & Target ───

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SpectrumCard {
    pub left_endpoint: String,
    pub right_endpoint: String,
    #[serde(default)]
    pub category: Option<String>,
}

/// Target position on the spectrum [0.0, 1.0].
/// 0.0 = fully left endpoint, 1.0 = fully right endpoint.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Target {
    pub position: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ScoringZone {
    Bullseye, // 4 pts
    Near,     // 3 pts
    Far,      // 2 pts
    Miss,     // 0 pts
}

impl ScoringZone {
    pub fn points(&self) -> i32 {
        match self {
            ScoringZone::Bullseye => 4,
            ScoringZone::Near => 3,
            ScoringZone::Far => 2,
            ScoringZone::Miss => 0,
        }
    }
}

// ─── Scoring Zone Configuration ───

/// Defines the outer radius of each scoring zone band, measured from the
/// target position along the `[0.0, 1.0]` spectrum.
///
/// IMPORTANT: these are **cumulative outer radii**, not independent band
/// widths. `bullseye_half_width` is the bullseye radius; `near_half_width`
/// is the outer edge of the Near band (which is the annulus between the
/// bullseye and this radius); `far_half_width` is the outer edge of the
/// Far band. Any guess outside `far_half_width` scores `Miss`.
///
/// Invariants: `0.0 < bullseye_half_width < near_half_width < far_half_width ≤ 0.5`.
/// The defaults below expand by 0.04 each band (bullseye ±0.04 → near
/// ±0.08 → far ±0.12).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ZoneConfig {
    /// Bullseye radius from the target.
    pub bullseye_half_width: f64,
    /// Outer radius of the Near band.
    pub near_half_width: f64,
    /// Outer radius of the Far band. Beyond this is Miss.
    pub far_half_width: f64,
}

impl Default for ZoneConfig {
    fn default() -> Self {
        Self {
            bullseye_half_width: 0.04,
            near_half_width: 0.08,
            far_half_width: 0.12,
        }
    }
}

// ─── Player & Team State ───

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlayerInfo {
    pub player_id: PlayerId,
    pub team: TeamId,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TeamState {
    pub team_id: TeamId,
    pub score: i32,
    pub player_ids: Vec<PlayerId>,
}

// ─── Turn Phase State Machine ───

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TurnPhase {
    /// Waiting for Cluegiver to submit a clue.
    /// Only the Cluegiver for the active team can see the target.
    CluePhase {
        active_team: TeamId,
        cluegiver: PlayerId,
    },

    /// Active team guessers discuss and submit a guess position.
    /// All guessers must submit before the phase advances (consensus).
    GuessPhase {
        active_team: TeamId,
        cluegiver: PlayerId,
        clue: String,
        /// Tracks each guesser's submitted position. Phase advances when all guessers have submitted.
        #[serde(default)]
        pending_guesses: HashMap<PlayerId, f64>,
    },

    /// Opposing team submits a steal guess direction.
    /// All stealers must submit before the phase advances (consensus).
    StealPhase {
        active_team: TeamId,
        stealing_team: TeamId,
        clue: String,
        active_guess: f64,
        /// Tracks each stealer's submitted direction. Phase advances when all stealers have submitted.
        #[serde(default)]
        pending_steals: HashMap<PlayerId, StealDirection>,
    },

    /// Scoring resolution — deterministic, no player input needed.
    Resolving {
        active_team: TeamId,
        stealing_team: TeamId,
        clue: String,
        active_guess: f64,
        steal_direction: StealDirection,
    },

    /// Game is over. `None` if draw (both teams cross threshold simultaneously
    /// and tie on score).
    GameOver { winner: Option<TeamId> },
}

impl TurnPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            TurnPhase::CluePhase { .. } => "clue_phase",
            TurnPhase::GuessPhase { .. } => "guess_phase",
            TurnPhase::StealPhase { .. } => "steal_phase",
            TurnPhase::Resolving { .. } => "resolving",
            TurnPhase::GameOver { .. } => "game_over",
        }
    }
}

// ─── Actions ───

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "action_type")]
pub enum VibeCheckAction {
    /// Cluegiver submits a clue word/phrase.
    GiveClue { clue: String },

    /// Active team submits their guess position on the spectrum.
    SubmitGuess { position: f64 },

    /// Opposing team guesses whether the target is left or right of the active team's guess.
    SubmitStealGuess { direction: StealDirection },

    /// Player forfeits (orchestrator-only, for timeout fallback).
    Forfeit,
}

impl EnvironmentAction for VibeCheckAction {
    fn action_type(&self) -> &str {
        match self {
            VibeCheckAction::GiveClue { .. } => "give_clue",
            VibeCheckAction::SubmitGuess { .. } => "submit_guess",
            VibeCheckAction::SubmitStealGuess { .. } => "submit_steal_guess",
            VibeCheckAction::Forfeit => "forfeit",
        }
    }
}

// ─── Game State ───

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VibeCheckState {
    /// Current round number (1-indexed).
    pub round: u32,

    /// Current turn phase.
    pub phase: TurnPhase,

    /// Team states with scores.
    pub teams: Vec<TeamState>,

    /// All players with team assignments.
    pub players: Vec<PlayerInfo>,

    /// Current spectrum card (visible to all).
    pub spectrum: Option<SpectrumCard>,

    /// Target position — only visible to the Cluegiver in CluePhase,
    /// and to all after Resolving.
    /// Set to None in fog-of-war filtered states for non-Cluegivers.
    pub target: Option<Target>,

    /// Scoring zone config (visible to all for reasoning).
    pub zone_config: ZoneConfig,

    /// Target score to win.
    pub target_score: i32,

    /// Cluegiver rotation index per team (tracks who gives clue next).
    pub cluegiver_rotation: Vec<usize>,

    /// History of completed rounds for context.
    pub round_history: Vec<RoundResult>,

    /// Whether the game has ended.
    pub is_game_over: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RoundResult {
    pub round: u32,
    pub spectrum: SpectrumCard,
    pub target_position: f64,
    pub clue: String,
    pub cluegiver: PlayerId,
    pub active_team: TeamId,
    pub active_guess: f64,
    pub active_zone: ScoringZone,
    pub active_points: i32,
    pub steal_direction: StealDirection,
    pub steal_correct: bool,
    pub steal_points: i32,
}

impl VibeCheckState {
    /// Returns a filtered copy of the state appropriate for the given player.
    /// - Cluegiver in CluePhase: sees target position
    /// - All others: target is hidden until Resolving
    pub fn filtered_for_player(&self, player_id: PlayerId) -> Self {
        let mut filtered = self.clone();

        match &self.phase {
            TurnPhase::CluePhase { cluegiver, .. } => {
                if player_id != *cluegiver {
                    filtered.target = None;
                }
            }
            TurnPhase::GuessPhase {
                active_team,
                cluegiver,
                clue,
                ..
            } => {
                // The active Psychic (cluegiver) has already seen the target
                // during CluePhase and continues to see it until scoring.
                // All other players (teammates and opponents) cannot see it.
                if player_id != *cluegiver {
                    filtered.target = None;
                }
                // Strip pending_guesses to prevent info leakage across
                // teams / cluegiver leaking through serialized state.
                filtered.phase = TurnPhase::GuessPhase {
                    active_team: *active_team,
                    cluegiver: *cluegiver,
                    clue: clue.clone(),
                    pending_guesses: HashMap::new(),
                };
            }
            TurnPhase::StealPhase {
                active_team,
                stealing_team,
                clue,
                active_guess,
                ..
            } => {
                // Psychic retains target visibility through the steal
                // window until the round resolves.
                let cluegiver_id = self.cluegiver_for_team(*active_team);
                if Some(player_id) != cluegiver_id {
                    filtered.target = None;
                }
                // Strip pending_steals — forfeit direction reveals target position
                filtered.phase = TurnPhase::StealPhase {
                    active_team: *active_team,
                    stealing_team: *stealing_team,
                    clue: clue.clone(),
                    active_guess: *active_guess,
                    pending_steals: HashMap::new(),
                };
            }
            // After resolving or game over, target is public (in round_history)
            TurnPhase::Resolving { .. } | TurnPhase::GameOver { .. } => {}
        }

        filtered
    }

    /// Returns the current cluegiver id for the given team, if one is
    /// active. Used by fog-of-war filtering when the phase variant does
    /// not itself carry the cluegiver (e.g. `StealPhase`).
    fn cluegiver_for_team(&self, team: TeamId) -> Option<PlayerId> {
        match &self.phase {
            TurnPhase::CluePhase {
                active_team,
                cluegiver,
            }
            | TurnPhase::GuessPhase {
                active_team,
                cluegiver,
                ..
            } => (*active_team == team).then_some(*cluegiver),
            TurnPhase::StealPhase { active_team, .. } => {
                if *active_team != team {
                    return None;
                }
                // Resolve via cluegiver_rotation: it records the *next*
                // cluegiver index per team, so the current round's
                // cluegiver is the index one behind (mod team size).
                let team_state = self.teams.iter().find(|t| t.team_id == team)?;
                let team_idx = self.teams.iter().position(|t| t.team_id == team)?;
                let next_idx = *self.cluegiver_rotation.get(team_idx)?;
                let team_size = team_state.player_ids.len();
                if team_size == 0 {
                    return None;
                }
                let current_idx = (next_idx + team_size - 1) % team_size;
                team_state.player_ids.get(current_idx).copied()
            }
            _ => None,
        }
    }
}

impl EnvironmentState for VibeCheckState {
    type PlayerId = PlayerId;

    fn turn_number(&self) -> u32 {
        self.round
    }

    fn current_phase(&self) -> &str {
        self.phase.as_str()
    }

    fn player_ids(&self) -> Vec<PlayerId> {
        self.players.iter().map(|p| p.player_id).collect()
    }

    fn is_terminal(&self) -> bool {
        self.is_game_over
    }
}

impl SequentialState for VibeCheckState {
    fn sequential_phase(&self) -> SequentialPhase<PlayerId> {
        match &self.phase {
            TurnPhase::CluePhase { cluegiver, .. } => SequentialPhase::Decision {
                kind: SequentialDecisionKind::Active,
                players: vec![*cluegiver],
                deadline: None,
            },
            TurnPhase::GuessPhase {
                active_team,
                cluegiver,
                pending_guesses,
                ..
            } => {
                // Only guessers who haven't submitted yet are listed as active.
                let guessers: Vec<PlayerId> = self
                    .teams
                    .iter()
                    .find(|t| t.team_id == *active_team)
                    .map(|t| {
                        t.player_ids
                            .iter()
                            .filter(|pid| **pid != *cluegiver && !pending_guesses.contains_key(pid))
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default();
                SequentialPhase::Decision {
                    kind: SequentialDecisionKind::Active,
                    players: guessers,
                    deadline: None,
                }
            }
            TurnPhase::StealPhase {
                stealing_team,
                pending_steals,
                ..
            } => {
                // Only stealers who haven't submitted yet are listed as active.
                let stealers: Vec<PlayerId> = self
                    .teams
                    .iter()
                    .find(|t| t.team_id == *stealing_team)
                    .map(|t| {
                        t.player_ids
                            .iter()
                            .filter(|pid| !pending_steals.contains_key(pid))
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default();
                SequentialPhase::Decision {
                    kind: SequentialDecisionKind::Reactive,
                    players: stealers,
                    deadline: None,
                }
            }
            TurnPhase::Resolving { .. } => SequentialPhase::Resolving,
            TurnPhase::GameOver {
                winner: Some(team_id),
            } => {
                let team_players: Vec<PlayerId> = self
                    .teams
                    .iter()
                    .filter(|t| t.team_id == *team_id)
                    .flat_map(|t| t.player_ids.iter().copied())
                    .collect();
                SequentialPhase::GameOver {
                    winner: EnvironmentWinner::Team(team_players),
                }
            }
            TurnPhase::GameOver { winner: None } => SequentialPhase::GameOver {
                winner: EnvironmentWinner::Draw,
            },
        }
    }
}

// ─── Spectator Events ───

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpectatorEvent {
    GameStarted {
        teams: Vec<TeamState>,
        players: Vec<PlayerInfo>,
        target_score: i32,
    },
    RoundStarted {
        round: u32,
        active_team: TeamId,
        cluegiver: PlayerId,
        spectrum: SpectrumCard,
    },
    ClueGiven {
        round: u32,
        cluegiver: PlayerId,
        clue: String,
    },
    AgentReasoning {
        player: PlayerId,
        reasoning: String,
    },
    PlayerGuessSubmitted {
        round: u32,
        player: PlayerId,
        position: f64,
    },
    GuessSubmitted {
        round: u32,
        team: TeamId,
        position: f64,
    },
    PlayerStealSubmitted {
        round: u32,
        player: PlayerId,
        direction: StealDirection,
    },
    StealGuessSubmitted {
        round: u32,
        team: TeamId,
        direction: StealDirection,
    },
    TargetRevealed {
        round: u32,
        target_position: f64,
        active_zone: ScoringZone,
        steal_correct: bool,
    },
    ScoreUpdate {
        round: u32,
        active_team: TeamId,
        active_points: i32,
        steal_team: TeamId,
        steal_points: i32,
        scores: Vec<(TeamId, i32)>,
    },
    GameOver {
        winner: Option<TeamId>,
        final_scores: Vec<(TeamId, i32)>,
    },
}

// ─── Service API Types ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMatchRequest {
    pub player_count: usize,
    pub target_score: Option<i32>,
    pub seed: Option<u64>,
    pub player_names: Option<Vec<String>>,
    /// Optional caller-provided match ID. When supplied the service uses this
    /// instead of generating a new ULID, letting callers correlate match IDs
    /// across systems.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMatchResponse {
    pub match_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spectator_url: Option<String>,
    pub players: Vec<PlayerInfo>,
    pub teams: Vec<TeamState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitActionRequest {
    pub player_id: PlayerId,
    pub action: VibeCheckAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitActionResponse {
    pub success: bool,
    pub events: Vec<SpectatorEvent>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStatusResponse {
    pub match_id: String,
    pub status: String,
    pub phase: String,
    pub round: u32,
    pub scores: Vec<(TeamId, i32)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner_team: Option<TeamId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Helper: build a minimal VibeCheckState for testing ───

    fn make_test_state(phase: TurnPhase) -> VibeCheckState {
        VibeCheckState {
            round: 1,
            phase,
            teams: vec![
                TeamState {
                    team_id: 0,
                    score: 0,
                    player_ids: vec![0, 1],
                },
                TeamState {
                    team_id: 1,
                    score: 0,
                    player_ids: vec![2, 3],
                },
            ],
            players: vec![
                PlayerInfo {
                    player_id: 0,
                    team: 0,
                    display_name: None,
                },
                PlayerInfo {
                    player_id: 1,
                    team: 0,
                    display_name: None,
                },
                PlayerInfo {
                    player_id: 2,
                    team: 1,
                    display_name: None,
                },
                PlayerInfo {
                    player_id: 3,
                    team: 1,
                    display_name: None,
                },
            ],
            spectrum: Some(SpectrumCard {
                left_endpoint: "Hot".to_string(),
                right_endpoint: "Cold".to_string(),
                category: None,
            }),
            target: Some(Target { position: 0.72 }),
            zone_config: ZoneConfig::default(),
            target_score: 10,
            cluegiver_rotation: vec![0, 0],
            round_history: vec![],
            is_game_over: false,
        }
    }

    // ─── 1. Serde round-trip for VibeCheckAction (all variants) ───

    #[test]
    fn test_serde_roundtrip_action_give_clue() {
        let action = VibeCheckAction::GiveClue {
            clue: "lukewarm".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: VibeCheckAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_action_submit_guess() {
        let action = VibeCheckAction::SubmitGuess { position: 0.65 };
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: VibeCheckAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_action_submit_steal_guess() {
        let action = VibeCheckAction::SubmitStealGuess {
            direction: StealDirection::Left,
        };
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: VibeCheckAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_action_forfeit() {
        let action = VibeCheckAction::Forfeit;
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: VibeCheckAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }

    // ─── 2. Serde round-trip for TurnPhase (all variants) ───

    #[test]
    fn test_serde_roundtrip_turn_phase_clue() {
        let phase = TurnPhase::CluePhase {
            active_team: 0,
            cluegiver: 1,
        };
        let json = serde_json::to_string(&phase).unwrap();
        let deserialized: TurnPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_turn_phase_guess() {
        let phase = TurnPhase::GuessPhase {
            active_team: 0,
            cluegiver: 1,
            clue: "warmth".to_string(),
            pending_guesses: HashMap::new(),
        };
        let json = serde_json::to_string(&phase).unwrap();
        let deserialized: TurnPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_turn_phase_steal() {
        let phase = TurnPhase::StealPhase {
            active_team: 0,
            stealing_team: 1,
            clue: "warmth".to_string(),
            active_guess: 0.65,
            pending_steals: HashMap::new(),
        };
        let json = serde_json::to_string(&phase).unwrap();
        let deserialized: TurnPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_turn_phase_resolving() {
        let phase = TurnPhase::Resolving {
            active_team: 0,
            stealing_team: 1,
            clue: "warmth".to_string(),
            active_guess: 0.65,
            steal_direction: StealDirection::Right,
        };
        let json = serde_json::to_string(&phase).unwrap();
        let deserialized: TurnPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_turn_phase_game_over_winner() {
        let phase = TurnPhase::GameOver { winner: Some(0) };
        let json = serde_json::to_string(&phase).unwrap();
        let deserialized: TurnPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_turn_phase_game_over_draw() {
        let phase = TurnPhase::GameOver { winner: None };
        let json = serde_json::to_string(&phase).unwrap();
        let deserialized: TurnPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, deserialized);
    }

    // ─── 3. Serde round-trip for SpectatorEvent (all variants) ───

    #[test]
    fn test_serde_roundtrip_spectator_game_started() {
        let event = SpectatorEvent::GameStarted {
            teams: vec![TeamState {
                team_id: 0,
                score: 0,
                player_ids: vec![0, 1],
            }],
            players: vec![PlayerInfo {
                player_id: 0,
                team: 0,
                display_name: Some("Agent-0".to_string()),
            }],
            target_score: 10,
        };
        let json = serde_json::to_string(&event).unwrap();
        let _: SpectatorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_serde_roundtrip_spectator_round_started() {
        let event = SpectatorEvent::RoundStarted {
            round: 1,
            active_team: 0,
            cluegiver: 0,
            spectrum: SpectrumCard {
                left_endpoint: "Hot".to_string(),
                right_endpoint: "Cold".to_string(),
                category: None,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let _: SpectatorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_serde_roundtrip_spectator_clue_given() {
        let event = SpectatorEvent::ClueGiven {
            round: 1,
            cluegiver: 0,
            clue: "lukewarm".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let _: SpectatorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_serde_roundtrip_spectator_agent_reasoning() {
        let event = SpectatorEvent::AgentReasoning {
            player: 1,
            reasoning: "I think lukewarm means around 0.3".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let _: SpectatorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_serde_roundtrip_spectator_guess_submitted() {
        let event = SpectatorEvent::GuessSubmitted {
            round: 1,
            team: 0,
            position: 0.65,
        };
        let json = serde_json::to_string(&event).unwrap();
        let _: SpectatorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_serde_roundtrip_spectator_steal_guess_submitted() {
        let event = SpectatorEvent::StealGuessSubmitted {
            round: 1,
            team: 1,
            direction: StealDirection::Left,
        };
        let json = serde_json::to_string(&event).unwrap();
        let _: SpectatorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_serde_roundtrip_spectator_target_revealed() {
        let event = SpectatorEvent::TargetRevealed {
            round: 1,
            target_position: 0.72,
            active_zone: ScoringZone::Bullseye,
            steal_correct: true,
        };
        let json = serde_json::to_string(&event).unwrap();
        let _: SpectatorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_serde_roundtrip_spectator_score_update() {
        let event = SpectatorEvent::ScoreUpdate {
            round: 1,
            active_team: 0,
            active_points: 4,
            steal_team: 1,
            steal_points: 1,
            scores: vec![(0, 4), (1, 1)],
        };
        let json = serde_json::to_string(&event).unwrap();
        let _: SpectatorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_serde_roundtrip_spectator_game_over() {
        let event = SpectatorEvent::GameOver {
            winner: Some(0),
            final_scores: vec![(0, 10), (1, 7)],
        };
        let json = serde_json::to_string(&event).unwrap();
        let _: SpectatorEvent = serde_json::from_str(&json).unwrap();
    }

    // ─── 4. Tagged union deserialization — verify action_type tag works ───

    #[test]
    fn test_tagged_union_give_clue() {
        let json = r#"{"action_type":"give_clue","clue":"lukewarm"}"#;
        let action: VibeCheckAction = serde_json::from_str(json).unwrap();
        assert_eq!(
            action,
            VibeCheckAction::GiveClue {
                clue: "lukewarm".to_string()
            }
        );
    }

    #[test]
    fn test_tagged_union_submit_guess() {
        let json = r#"{"action_type":"submit_guess","position":0.65}"#;
        let action: VibeCheckAction = serde_json::from_str(json).unwrap();
        assert_eq!(action, VibeCheckAction::SubmitGuess { position: 0.65 });
    }

    #[test]
    fn test_tagged_union_submit_steal_guess() {
        let json = r#"{"action_type":"submit_steal_guess","direction":"left"}"#;
        let action: VibeCheckAction = serde_json::from_str(json).unwrap();
        assert_eq!(
            action,
            VibeCheckAction::SubmitStealGuess {
                direction: StealDirection::Left
            }
        );
    }

    #[test]
    fn test_tagged_union_forfeit() {
        let json = r#"{"action_type":"forfeit"}"#;
        let action: VibeCheckAction = serde_json::from_str(json).unwrap();
        assert_eq!(action, VibeCheckAction::Forfeit);
    }

    // ─── 5. filtered_for_player — cluegiver sees target, others don't ───

    #[test]
    fn test_filtered_for_player_cluegiver_sees_target() {
        let state = make_test_state(TurnPhase::CluePhase {
            active_team: 0,
            cluegiver: 0,
        });

        let filtered = state.filtered_for_player(0);
        assert!(filtered.target.is_some());
        assert_eq!(filtered.target.unwrap().position, 0.72);
    }

    #[test]
    fn test_filtered_for_player_non_cluegiver_hidden() {
        let state = make_test_state(TurnPhase::CluePhase {
            active_team: 0,
            cluegiver: 0,
        });

        // Teammate (non-cluegiver) should not see target
        let filtered = state.filtered_for_player(1);
        assert!(filtered.target.is_none());

        // Opposing team should not see target
        let filtered = state.filtered_for_player(2);
        assert!(filtered.target.is_none());
    }

    #[test]
    fn test_filtered_for_player_guess_phase_psychic_retains_target() {
        let state = make_test_state(TurnPhase::GuessPhase {
            active_team: 0,
            cluegiver: 0,
            clue: "lukewarm".to_string(),
            pending_guesses: HashMap::new(),
        });

        // The Psychic (cluegiver) saw the target during CluePhase and
        // retains visibility through GuessPhase until the round resolves.
        let filtered = state.filtered_for_player(0);
        assert!(filtered.target.is_some());

        // All other players (teammates and opponents) cannot see it.
        let filtered = state.filtered_for_player(1);
        assert!(filtered.target.is_none());
    }

    #[test]
    fn test_filtered_for_player_steal_phase_hides_target() {
        let state = make_test_state(TurnPhase::StealPhase {
            active_team: 0,
            stealing_team: 1,
            clue: "lukewarm".to_string(),
            active_guess: 0.65,
            pending_steals: HashMap::new(),
        });

        let filtered = state.filtered_for_player(2);
        assert!(filtered.target.is_none());
    }

    // ─── 6. filtered_for_player — after resolving, all see target ───

    #[test]
    fn test_filtered_for_player_resolving_shows_target() {
        let state = make_test_state(TurnPhase::Resolving {
            active_team: 0,
            stealing_team: 1,
            clue: "lukewarm".to_string(),
            active_guess: 0.65,
            steal_direction: StealDirection::Right,
        });

        for pid in 0..4 {
            let filtered = state.filtered_for_player(pid);
            assert!(
                filtered.target.is_some(),
                "player {pid} should see target during Resolving"
            );
        }
    }

    #[test]
    fn test_filtered_for_player_game_over_shows_target() {
        let state = make_test_state(TurnPhase::GameOver { winner: Some(0) });

        for pid in 0..4 {
            let filtered = state.filtered_for_player(pid);
            assert!(
                filtered.target.is_some(),
                "player {pid} should see target after GameOver"
            );
        }
    }

    // ─── 7. SequentialState — each phase maps correctly ───

    #[test]
    fn test_sequential_phase_clue_phase() {
        let state = make_test_state(TurnPhase::CluePhase {
            active_team: 0,
            cluegiver: 0,
        });

        let phase = state.sequential_phase();
        assert_eq!(
            phase,
            SequentialPhase::Decision {
                kind: SequentialDecisionKind::Active,
                players: vec![0],
                deadline: None,
            }
        );
    }

    #[test]
    fn test_sequential_phase_guess_phase() {
        let state = make_test_state(TurnPhase::GuessPhase {
            active_team: 0,
            cluegiver: 0,
            clue: "lukewarm".to_string(),
            pending_guesses: HashMap::new(),
        });

        let phase = state.sequential_phase();
        // Guessers = team 0 players excluding cluegiver 0 → [1]
        assert_eq!(
            phase,
            SequentialPhase::Decision {
                kind: SequentialDecisionKind::Active,
                players: vec![1],
                deadline: None,
            }
        );
    }

    #[test]
    fn test_sequential_phase_steal_phase() {
        let state = make_test_state(TurnPhase::StealPhase {
            active_team: 0,
            stealing_team: 1,
            clue: "lukewarm".to_string(),
            active_guess: 0.65,
            pending_steals: HashMap::new(),
        });

        let phase = state.sequential_phase();
        // Stealers = team 1 players → [2, 3]
        assert_eq!(
            phase,
            SequentialPhase::Decision {
                kind: SequentialDecisionKind::Reactive,
                players: vec![2, 3],
                deadline: None,
            }
        );
    }

    #[test]
    fn test_sequential_phase_resolving() {
        let state = make_test_state(TurnPhase::Resolving {
            active_team: 0,
            stealing_team: 1,
            clue: "lukewarm".to_string(),
            active_guess: 0.65,
            steal_direction: StealDirection::Right,
        });

        assert_eq!(state.sequential_phase(), SequentialPhase::Resolving);
    }

    #[test]
    fn test_sequential_phase_game_over_with_winner() {
        let state = make_test_state(TurnPhase::GameOver { winner: Some(0) });

        // Team 0 wins → EnvironmentWinner::Team with team 0 members [0, 1]
        assert_eq!(
            state.sequential_phase(),
            SequentialPhase::GameOver {
                winner: EnvironmentWinner::Team(vec![0, 1]),
            }
        );
    }

    #[test]
    fn test_sequential_phase_game_over_team1_winner_maps_to_team() {
        // Team 1 wins → EnvironmentWinner::Team with team 1 members [2, 3]
        let state = make_test_state(TurnPhase::GameOver { winner: Some(1) });

        assert_eq!(
            state.sequential_phase(),
            SequentialPhase::GameOver {
                winner: EnvironmentWinner::Team(vec![2, 3]),
            }
        );
    }

    #[test]
    fn test_sequential_phase_game_over_draw() {
        let state = make_test_state(TurnPhase::GameOver { winner: None });

        assert_eq!(
            state.sequential_phase(),
            SequentialPhase::GameOver {
                winner: EnvironmentWinner::Draw,
            }
        );
    }

    // ─── Regression: team winners contain all team members ───

    #[test]
    fn test_game_over_team0_all_members_present() {
        let state = make_test_state(TurnPhase::GameOver { winner: Some(0) });
        match state.sequential_phase() {
            SequentialPhase::GameOver {
                winner: EnvironmentWinner::Team(members),
            } => {
                assert!(members.contains(&0), "team 0 member 0 missing");
                assert!(members.contains(&1), "team 0 member 1 missing");
                assert_eq!(members.len(), 2);
            }
            other => panic!("expected Team winner, got: {other:?}"),
        }
    }

    #[test]
    fn test_game_over_team1_all_members_present() {
        let state = make_test_state(TurnPhase::GameOver { winner: Some(1) });
        match state.sequential_phase() {
            SequentialPhase::GameOver {
                winner: EnvironmentWinner::Team(members),
            } => {
                assert!(members.contains(&2), "team 1 member 2 missing");
                assert!(members.contains(&3), "team 1 member 3 missing");
                assert_eq!(members.len(), 2);
            }
            other => panic!("expected Team winner, got: {other:?}"),
        }
    }

    #[test]
    fn test_game_over_draw_is_draw_variant() {
        let state = make_test_state(TurnPhase::GameOver { winner: None });
        assert!(matches!(
            state.sequential_phase(),
            SequentialPhase::GameOver {
                winner: EnvironmentWinner::Draw
            }
        ));
    }

    // ─── 8. EnvironmentAction — action_type() returns correct strings ───

    #[test]
    fn test_action_type_give_clue() {
        let action = VibeCheckAction::GiveClue {
            clue: "test".to_string(),
        };
        assert_eq!(EnvironmentAction::action_type(&action), "give_clue");
    }

    #[test]
    fn test_action_type_submit_guess() {
        let action = VibeCheckAction::SubmitGuess { position: 0.5 };
        assert_eq!(EnvironmentAction::action_type(&action), "submit_guess");
    }

    #[test]
    fn test_action_type_submit_steal_guess() {
        let action = VibeCheckAction::SubmitStealGuess {
            direction: StealDirection::Left,
        };
        assert_eq!(
            EnvironmentAction::action_type(&action),
            "submit_steal_guess"
        );
    }

    #[test]
    fn test_action_type_forfeit() {
        let action = VibeCheckAction::Forfeit;
        assert_eq!(EnvironmentAction::action_type(&action), "forfeit");
    }

    // ─── 9. ScoringZone::points() returns correct values ───

    #[test]
    fn test_scoring_zone_points() {
        assert_eq!(ScoringZone::Bullseye.points(), 4);
        assert_eq!(ScoringZone::Near.points(), 3);
        assert_eq!(ScoringZone::Far.points(), 2);
        assert_eq!(ScoringZone::Miss.points(), 0);
    }

    // ─── 10. ZoneConfig::default() returns correct values ───

    #[test]
    fn test_zone_config_default() {
        let config = ZoneConfig::default();
        assert!((config.bullseye_half_width - 0.04).abs() < f64::EPSILON);
        assert!((config.near_half_width - 0.08).abs() < f64::EPSILON);
        assert!((config.far_half_width - 0.12).abs() < f64::EPSILON);
    }

    // ─── EnvironmentState trait methods ───

    #[test]
    fn test_environment_state_turn_number() {
        let state = make_test_state(TurnPhase::CluePhase {
            active_team: 0,
            cluegiver: 0,
        });
        assert_eq!(state.turn_number(), 1);
    }

    #[test]
    fn test_environment_state_current_phase() {
        let state = make_test_state(TurnPhase::CluePhase {
            active_team: 0,
            cluegiver: 0,
        });
        assert_eq!(state.current_phase(), "clue_phase");
    }

    #[test]
    fn test_environment_state_player_ids() {
        let state = make_test_state(TurnPhase::CluePhase {
            active_team: 0,
            cluegiver: 0,
        });
        assert_eq!(state.player_ids(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_environment_state_is_terminal_false() {
        let state = make_test_state(TurnPhase::CluePhase {
            active_team: 0,
            cluegiver: 0,
        });
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_environment_state_is_terminal_true() {
        let mut state = make_test_state(TurnPhase::GameOver { winner: Some(0) });
        state.is_game_over = true;
        assert!(state.is_terminal());
    }
}
