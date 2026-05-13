use std::collections::HashMap;

use eval_runtime::{EnvironmentState, SequentialPhase, SequentialState};
use wordle_protocol::{
    word_list, ChatPhase, EngineError, LetterFeedback, PlayerId, TerminalReason, WordleAction,
    WordleConfig, WordleGame, WordlePhase,
};

fn names() -> HashMap<PlayerId, String> {
    [
        (0, "Alpha".to_string()),
        (1, "Beta".to_string()),
        (2, "Gamma".to_string()),
    ]
    .into_iter()
    .collect()
}

fn game_with_target(target: &str) -> WordleGame {
    WordleGame::new_with_target(
        vec![0, 1, 2],
        names(),
        WordleConfig::default(),
        target.to_string(),
    )
    .expect("game should initialize")
}

#[test]
fn rejects_duplicate_player_ids() {
    let result = WordleGame::new_with_target(
        vec![0, 0, 1],
        names(),
        WordleConfig::default(),
        "crane".to_string(),
    );
    assert!(result.is_err());
}

#[test]
fn rejects_non_ascii_or_non_alpha_target_words() {
    let result = WordleGame::new_with_target(
        vec![0, 1, 2],
        names(),
        WordleConfig::default(),
        "ééééé".to_string(),
    );
    assert!(result.is_err());

    let result = WordleGame::new_with_target(
        vec![0, 1, 2],
        names(),
        WordleConfig::default(),
        "ab1de".to_string(),
    );
    assert!(result.is_err());
}

fn send(msg: &str) -> WordleAction {
    WordleAction::SendMessage {
        message: msg.to_string(),
    }
}

fn guess(word: &str) -> WordleAction {
    WordleAction::Guess {
        word: word.to_string(),
    }
}

#[test]
fn default_config_matches_spec() {
    let cfg = WordleConfig::default();
    assert_eq!(cfg.max_guesses, 6);
    assert_eq!(cfg.max_message_chars, 200);
}

#[test]
fn word_selection_is_deterministic_and_valid_set_contains_both_lists() {
    let w1 = word_list::select_word(0);
    let w2 = word_list::select_word(0);
    assert_eq!(w1, w2);

    let valid = word_list::valid_word_set();
    assert!(valid.contains("crane"));
    assert!(valid.contains("adieu"));
}

#[test]
fn lobby_accepts_chat_and_first_guess_advances_phase() {
    // Lobby is optional pre-game chat. Silent players do not block the
    // match — the first guess from any player advances to Guessing.
    let mut game = game_with_target("crane");
    let state = game.full_state();
    assert_eq!(state.phase, WordlePhase::Lobby);
    assert_eq!(state.turn, 0);

    // In Lobby every player's legal-actions set includes both chat and guess.
    let actions = game.legal_actions(0);
    assert!(actions
        .iter()
        .any(|a| matches!(a, WordleAction::SendMessage { .. })));
    assert!(actions
        .iter()
        .any(|a| matches!(a, WordleAction::Guess { .. })));

    game.apply_action(0, &send("hello")).unwrap();
    // Chat keeps the phase in Lobby.
    assert_eq!(game.full_state().phase, WordlePhase::Lobby);

    // First guess transitions to Guessing — even though players 1 and 2 never spoke.
    game.apply_action(0, &guess("crane")).unwrap();
    let after = game.full_state();
    assert_eq!(after.phase, WordlePhase::Guessing);
    assert_eq!(after.turn, 1);
}

#[test]
fn feedback_handles_duplicate_letters_correctly() {
    let mut game = game_with_target("array");
    game.apply_action(0, &send("lobby")).unwrap();
    game.apply_action(1, &send("lobby")).unwrap();
    game.apply_action(2, &send("lobby")).unwrap();

    game.apply_action(0, &guess("rarer")).unwrap();

    let view = game.state_for_player(0).unwrap();
    let fb = &view.my_progress.guesses[0].feedback;
    assert_eq!(
        fb,
        &vec![
            LetterFeedback::Present,
            LetterFeedback::Present,
            LetterFeedback::Correct,
            LetterFeedback::Absent,
            LetterFeedback::Absent,
        ]
    );
}

#[test]
fn fog_of_war_hides_opponent_words_but_shows_counts_and_status() {
    let mut game = game_with_target("crane");
    game.apply_action(0, &send("go")).unwrap();
    game.apply_action(1, &send("go")).unwrap();
    game.apply_action(2, &send("go")).unwrap();

    game.apply_action(0, &guess("crane")).unwrap();
    game.apply_action(1, &guess("slate")).unwrap();
    game.apply_action(2, &guess("trace")).unwrap();

    let p1 = game.state_for_player(1).unwrap();
    assert!(p1.my_progress.guesses[0].word == "slate");
    assert_eq!(p1.opponents.len(), 2);

    let opp0 = p1.opponents.iter().find(|o| o.player_id == 0).unwrap();
    assert_eq!(opp0.guess_count, 1);
    assert!(opp0.solved);
}

#[test]
fn solved_player_must_send_win_message_during_guessing() {
    let mut game = game_with_target("crane");
    game.apply_action(0, &send("go")).unwrap();
    game.apply_action(1, &send("go")).unwrap();
    game.apply_action(2, &send("go")).unwrap();

    game.apply_action(0, &guess("crane")).unwrap();
    assert_eq!(game.legal_actions(0), vec![send("")]);
    game.apply_action(0, &send("got it")).unwrap();

    let full = game.full_state();
    assert!(full
        .chat_messages
        .iter()
        .any(|m| m.player_id == 0 && m.phase == ChatPhase::Win));
}

#[test]
fn all_guessers_guessing_advances_turn_and_enforces_once_per_turn() {
    let mut game = game_with_target("civic");
    game.apply_action(0, &send("go")).unwrap();
    game.apply_action(1, &send("go")).unwrap();
    game.apply_action(2, &send("go")).unwrap();

    game.apply_action(0, &guess("crane")).unwrap();
    assert!(game.apply_action(0, &guess("grape")).is_err());

    game.apply_action(1, &guess("grape")).unwrap();
    let s = game.full_state();
    assert_eq!(s.turn, 1);

    game.apply_action(2, &guess("joker")).unwrap();
    let s = game.full_state();
    assert_eq!(s.turn, 2);
}

#[test]
fn enters_banter_after_max_guesses_then_game_over_when_budget_exhausted() {
    // Banter ends once the total chat budget is exhausted (default
    // max_messages_per_chat_phase * player_count) — it no longer waits
    // for every player to speak.
    let mut game = game_with_target("civic");
    // Kick off Guessing directly.
    game.apply_action(0, &guess("crane")).unwrap();

    // Play out the remaining turns. Player 0 already used turn 1.
    for _ in 0..5 {
        // Remaining guessers this turn (p1, p2) finish their guess first,
        // then p0 starts the next turn's round of guesses.
        if !game.full_state().players[1].solved {
            game.apply_action(1, &guess("grape")).unwrap();
        }
        if !game.full_state().players[2].solved {
            game.apply_action(2, &guess("joker")).unwrap();
        }
        if !game.full_state().players[0].solved {
            game.apply_action(0, &guess("crane")).unwrap();
        }
    }
    // Finish p1 / p2's final guesses so all are eliminated.
    while !game.full_state().players[1].solved && !game.full_state().players[1].eliminated {
        game.apply_action(1, &guess("grape")).unwrap();
    }
    while !game.full_state().players[2].solved && !game.full_state().players[2].eliminated {
        game.apply_action(2, &guess("joker")).unwrap();
    }

    let s = game.full_state();
    assert_eq!(s.phase, WordlePhase::Banter);
    assert_eq!(s.terminal_reason, Some(TerminalReason::MaxGuessesExhausted));
    // Target is revealed at Banter so spectators learn the answer.
    assert_eq!(s.target_word.as_deref(), Some("civic"));

    // Exhaust the Banter budget: default is 3 per player * 3 players = 9 total.
    for _ in 0..3 {
        game.apply_action(0, &send("gg")).unwrap();
        game.apply_action(1, &send("gg")).unwrap();
        game.apply_action(2, &send("gg")).unwrap();
    }

    let terminal = game.full_state();
    assert_eq!(terminal.phase, WordlePhase::GameOver);
    assert!(terminal.is_terminal);
    assert!(game.legal_actions(0).is_empty());
}

#[test]
fn full_state_trait_impls_report_winner_and_phase() {
    let mut game = game_with_target("crane");
    game.apply_action(0, &send("go")).unwrap();
    game.apply_action(1, &send("go")).unwrap();
    game.apply_action(2, &send("go")).unwrap();

    game.apply_action(0, &guess("crane")).unwrap();
    game.apply_action(0, &send("won")).unwrap();
    game.apply_action(1, &guess("grape")).unwrap();
    game.apply_action(2, &guess("joker")).unwrap();

    for _ in 0..5 {
        game.apply_action(1, &guess("grape")).unwrap();
        game.apply_action(2, &guess("joker")).unwrap();
    }

    // Exhaust the Banter chat budget (default 3 per player * 3 players).
    for _ in 0..3 {
        game.apply_action(0, &send("banter")).unwrap();
        game.apply_action(1, &send("banter")).unwrap();
        game.apply_action(2, &send("banter")).unwrap();
    }

    let full = game.full_state();
    assert!(full.is_terminal());
    assert_eq!(full.current_phase(), "game_over");

    match full.sequential_phase() {
        SequentialPhase::GameOver { winner } => {
            assert_eq!(format!("{winner:?}"), "Player(0)");
        }
        other => panic!("expected game over, got {other:?}"),
    }
}

// -----------------------------------------------------------------------
// Target word: shared across players, hidden until match ends.
// -----------------------------------------------------------------------

#[test]
fn target_word_is_deterministic_from_seed() {
    // Two games seeded identically must produce the same target word.
    let pull_target = |seed: u64| -> String {
        let mut g = WordleGame::new(vec![0, 1, 2], names(), WordleConfig::default(), seed)
            .expect("game should initialize");
        // Exhaust to Banter so the target is revealed via full_state.
        for _ in 0..6 {
            let _ = g.apply_action(0, &guess("crane"));
            let _ = g.apply_action(1, &guess("grape"));
            let _ = g.apply_action(2, &guess("joker"));
        }
        g.full_state()
            .target_word
            .expect("target revealed at Banter")
    };

    assert_eq!(pull_target(0xC0FFEE), pull_target(0xC0FFEE));
    let word = pull_target(0xC0FFEE);
    assert_eq!(word.len(), 5);
    assert!(word.chars().all(|c| c.is_ascii_lowercase()));
}

#[test]
fn target_word_is_hidden_until_match_ends() {
    let mut game = game_with_target("crane");

    // Lobby: target not yet revealed.
    assert_eq!(game.full_state().target_word, None);
    assert_eq!(game.state_for_player(0).unwrap().revealed_target_word, None);

    // Lobby->Guessing transitions on the first guess (lobby-liveness change).
    game.apply_action(0, &guess("crane")).unwrap();

    // Guessing phase: still hidden.
    assert_eq!(game.full_state().phase, WordlePhase::Guessing);
    assert_eq!(game.full_state().target_word, None);
    assert_eq!(game.state_for_player(0).unwrap().revealed_target_word, None);

    // OpponentSummary doesn't carry a target_word field — schema-level guard.
    let p0_view = game.state_for_player(0).unwrap();
    let opp_field_names: Vec<_> = serde_json::to_value(&p0_view.opponents[0])
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert!(
        !opp_field_names.iter().any(|k| k == "target_word"),
        "OpponentSummary must not carry target_word"
    );
}

#[test]
fn evaluate_play_along_rejects_during_lobby() {
    let game = game_with_target("crane");
    let result = game.evaluate_play_along_guess("slate");
    assert!(
        matches!(result, Err(EngineError::InvalidPhase)),
        "play-along should be rejected during Lobby"
    );
}

#[test]
fn evaluate_play_along_works_during_guessing() {
    let mut game = game_with_target("crane");
    game.apply_action(0, &guess("slate")).unwrap();
    let (feedback, solved) = game
        .evaluate_play_along_guess("crane")
        .expect("guessing-phase play-along accepted");
    assert_eq!(feedback.len(), 5);
    assert!(solved);
}

#[test]
fn evaluate_play_along_works_after_match_ends() {
    // Drive the match to Banter, then to GameOver, and verify the
    // play-along endpoint accepts in BOTH phases — the user should be
    // able to keep playing after the agents have finished. Target is
    // already revealed via full_state at those phases so allowing
    // play-along leaks nothing.
    let mut game = game_with_target("civic");
    game.apply_action(0, &guess("crane")).unwrap();
    for _ in 0..5 {
        if !game.full_state().players[1].solved {
            game.apply_action(1, &guess("grape")).unwrap();
        }
        if !game.full_state().players[2].solved {
            game.apply_action(2, &guess("joker")).unwrap();
        }
        if !game.full_state().players[0].solved {
            game.apply_action(0, &guess("crane")).unwrap();
        }
    }
    while !game.full_state().players[1].solved && !game.full_state().players[1].eliminated {
        game.apply_action(1, &guess("grape")).unwrap();
    }
    while !game.full_state().players[2].solved && !game.full_state().players[2].eliminated {
        game.apply_action(2, &guess("joker")).unwrap();
    }
    assert_eq!(game.full_state().phase, WordlePhase::Banter);
    let (feedback, _) = game
        .evaluate_play_along_guess("slate")
        .expect("banter-phase play-along accepted");
    assert_eq!(feedback.len(), 5);

    // Exhaust banter budget to advance to GameOver and verify again.
    for _ in 0..3 {
        game.apply_action(0, &send("gg")).unwrap();
        game.apply_action(1, &send("gg")).unwrap();
        game.apply_action(2, &send("gg")).unwrap();
    }
    assert_eq!(game.full_state().phase, WordlePhase::GameOver);
    let (feedback, _) = game
        .evaluate_play_along_guess("slate")
        .expect("game-over-phase play-along accepted");
    assert_eq!(feedback.len(), 5);
}
