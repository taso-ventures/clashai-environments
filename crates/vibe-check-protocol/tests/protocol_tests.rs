use std::collections::HashMap;

use eval_runtime::{
    EnvironmentAction, EnvironmentState, EnvironmentWinner, SequentialDecisionKind,
    SequentialPhase, SequentialState,
};
use vibe_check_protocol::{
    PlayerInfo, ScoringZone, SpectatorEvent, SpectrumCard, StealDirection, Target, TeamState,
    TurnPhase, VibeCheckAction, VibeCheckState, ZoneConfig,
};

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
