use poker_protocol::engine::PokerMatch;
use poker_protocol::{
    BettingRound, MatchPhase, PokerAction, BIG_BLIND, INITIAL_STACK, MAX_HANDS, SMALL_BLIND,
};

#[test]
fn test_new_match_starts_hand_1() {
    let game = PokerMatch::new(42).unwrap();
    let state = game.state();
    assert_eq!(state.hand_number, 1);
    assert_eq!(state.max_hands, MAX_HANDS);
    assert_eq!(state.profits, [0, 0]);
    assert!(matches!(state.phase, MatchPhase::Playing));
    assert!(state.current_hand.is_some());
}

#[test]
fn test_blinds_posted_correctly() {
    let game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let hand = state.current_hand.unwrap();

    // Hand 1: button=0 (SB), player 1 is BB
    assert_eq!(hand.button, 0);
    assert_eq!(hand.pot, SMALL_BLIND + BIG_BLIND);

    // SB (player 0) has INITIAL_STACK - SMALL_BLIND
    assert_eq!(hand.stacks[0], INITIAL_STACK - SMALL_BLIND);
    // BB (player 1) has INITIAL_STACK - BIG_BLIND
    assert_eq!(hand.stacks[1], INITIAL_STACK - BIG_BLIND);
}

#[test]
fn test_preflop_sb_acts_first() {
    let game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let hand = state.current_hand.unwrap();

    // In HU, SB (button) acts first preflop
    assert_eq!(hand.action_on, hand.button);
}

#[test]
fn test_fold_wins_pot() {
    let mut game = PokerMatch::new(42).unwrap();

    // SB (player 0 on hand 1) folds preflop
    let state = game.state();
    let sb = state.button;
    game.apply_action(sb, &PokerAction::Fold).unwrap();

    // Hand should be finished, BB wins the SB
    let state = game.state();
    assert_eq!(state.hand_number, 2); // moved to next hand
    assert_eq!(state.hand_history.len(), 1);

    let result = &state.hand_history[0];
    assert_eq!(result.winner, Some(1 - sb));
    assert!(!result.showdown);
    // BB wins SB's blind
    assert_eq!(result.profits[sb as usize], -SMALL_BLIND);
    assert_eq!(result.profits[(1 - sb) as usize], SMALL_BLIND);
}

#[test]
fn test_check_check_advances_street() {
    let mut game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let sb = state.button;
    let bb = 1 - sb;

    // SB calls preflop (limps)
    game.apply_action(sb, &PokerAction::Call).unwrap();
    // BB checks
    game.apply_action(bb, &PokerAction::Check).unwrap();

    // Should be on the flop now
    let state = game.state();
    let hand = state.current_hand.unwrap();
    assert_eq!(hand.round, BettingRound::Flop);
    assert_eq!(hand.community.len(), 3);
    // Postflop: BB acts first in HU
    assert_eq!(hand.action_on, bb);
}

#[test]
fn test_call_call_through_streets() {
    let mut game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let sb = state.button;
    let bb = 1 - sb;

    // Preflop: SB calls, BB checks
    game.apply_action(sb, &PokerAction::Call).unwrap();
    game.apply_action(bb, &PokerAction::Check).unwrap();

    // Flop: BB checks, SB checks
    game.apply_action(bb, &PokerAction::Check).unwrap();
    game.apply_action(sb, &PokerAction::Check).unwrap();

    let state = game.state();
    let hand = state.current_hand.unwrap();
    assert_eq!(hand.round, BettingRound::Turn);
    assert_eq!(hand.community.len(), 4);

    // Turn: BB checks, SB checks
    game.apply_action(bb, &PokerAction::Check).unwrap();
    game.apply_action(sb, &PokerAction::Check).unwrap();

    let state = game.state();
    let hand = state.current_hand.unwrap();
    assert_eq!(hand.round, BettingRound::River);
    assert_eq!(hand.community.len(), 5);

    // River: BB checks, SB checks -> showdown
    game.apply_action(bb, &PokerAction::Check).unwrap();
    game.apply_action(sb, &PokerAction::Check).unwrap();

    // Hand should be over, moved to hand 2
    let state = game.state();
    assert_eq!(state.hand_number, 2);
    assert_eq!(state.hand_history.len(), 1);

    let result = &state.hand_history[0];
    assert!(result.showdown);
    assert_eq!(result.pot, BIG_BLIND * 2); // 2 each from blinds, SB called to match
}

#[test]
fn test_raise_and_call() {
    let mut game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let sb = state.button;
    let bb = 1 - sb;

    // Preflop: SB raises to 6
    game.apply_action(sb, &PokerAction::Raise { amount: 6 })
        .unwrap();
    // BB calls
    game.apply_action(bb, &PokerAction::Call).unwrap();

    let state = game.state();
    let hand = state.current_hand.unwrap();
    assert_eq!(hand.round, BettingRound::Flop);
    assert_eq!(hand.pot, 12); // 6 from each
}

#[test]
fn test_legal_actions_preflop_sb() {
    let game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let sb = state.button;

    let legal = game.legal_actions(sb);
    assert!(legal.can_fold);
    assert!(!legal.can_check); // facing BB
    assert!(legal.can_call);
    assert_eq!(legal.call_amount, BIG_BLIND - SMALL_BLIND); // 1 to call
    assert!(legal.can_raise);
    assert_eq!(legal.min_raise, BIG_BLIND * 2); // min raise to 4
}

#[test]
fn test_legal_actions_after_limp() {
    let mut game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let sb = state.button;
    let bb = 1 - sb;

    // SB limps
    game.apply_action(sb, &PokerAction::Call).unwrap();

    let legal = game.legal_actions(bb);
    assert!(!legal.can_fold); // no bet to face (bets are equal)
    assert!(legal.can_check);
    assert!(!legal.can_call);
    assert!(legal.can_raise);
}

#[test]
fn test_no_actions_for_wrong_player() {
    let game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let bb = 1 - state.button;

    // BB shouldn't have actions when it's SB's turn
    let legal = game.legal_actions(bb);
    assert!(!legal.can_fold);
    assert!(!legal.can_check);
    assert!(!legal.can_call);
    assert!(!legal.can_raise);
}

#[test]
fn test_all_in_deals_remaining_community() {
    let mut game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let sb = state.button;
    let bb = 1 - sb;

    // SB goes all-in preflop
    let max_raise = game.legal_actions(sb).max_raise;
    game.apply_action(sb, &PokerAction::Raise { amount: max_raise })
        .unwrap();
    // BB calls
    game.apply_action(bb, &PokerAction::Call).unwrap();

    // Hand should be complete with full board
    let state = game.state();
    assert_eq!(state.hand_number, 2);
    let result = &state.hand_history[0];
    assert_eq!(result.community.len(), 5);
    assert!(result.showdown);
    assert_eq!(result.pot, INITIAL_STACK * 2);
}

#[test]
fn test_player_view_hides_opponent_cards() {
    let game = PokerMatch::new(42).unwrap();
    let full_state = game.state();
    let full_hand = full_state.current_hand.unwrap();

    let p0_view = game.state_for_player(0);
    let p0_hand = p0_view.current_hand.unwrap();

    // Player 0 should see their own cards
    assert_eq!(p0_hand.your_cards, full_hand.hole_cards[0]);

    // Player 1's view should show different cards
    let p1_view = game.state_for_player(1);
    let p1_hand = p1_view.current_hand.unwrap();
    assert_eq!(p1_hand.your_cards, full_hand.hole_cards[1]);

    // Neither player view exposes opponent hole cards (struct doesn't have them)
    // This is enforced by PlayerHandView not containing opponent cards.
}

#[test]
fn test_button_alternates() {
    let mut game = PokerMatch::new(42).unwrap();

    let state = game.state();
    let h1_button = state.button;

    // Fold to advance to hand 2
    game.apply_action(h1_button, &PokerAction::Fold).unwrap();

    let state = game.state();
    assert_eq!(state.hand_number, 2);
    assert_ne!(state.button, h1_button);
}

#[test]
fn test_match_completes_after_max_hands() {
    let mut game = PokerMatch::new(42).unwrap();

    // Play MAX_HANDS hands by folding SB each time
    for _ in 0..MAX_HANDS {
        if game.is_terminal() {
            break;
        }
        let sb = game.state().button;
        game.apply_action(sb, &PokerAction::Fold).unwrap();
    }

    assert!(game.is_terminal());
    let state = game.state();
    assert!(matches!(state.phase, MatchPhase::Completed));
    assert_eq!(state.hand_history.len(), MAX_HANDS as usize);
}

#[test]
fn test_profits_accumulate() {
    let mut game = PokerMatch::new(42).unwrap();

    // Fold SB twice
    let sb1 = game.state().button;
    game.apply_action(sb1, &PokerAction::Fold).unwrap();

    let sb2 = game.state().button;
    game.apply_action(sb2, &PokerAction::Fold).unwrap();

    let state = game.state();
    // Each player folded SB once, losing SMALL_BLIND each time
    // and won BB once, gaining SMALL_BLIND each time
    // So net profit = 0 for both (SB loss from folding = SB gain from opponent folding)
    assert_eq!(state.profits[0], 0);
    assert_eq!(state.profits[1], 0);
}

#[test]
fn test_invalid_raise_amount_rejected() {
    let mut game = PokerMatch::new(42).unwrap();
    let sb = game.state().button;

    // Try raising to 1 (below min raise of 4)
    let result = game.apply_action(sb, &PokerAction::Raise { amount: 1 });
    assert!(result.is_err());
}

#[test]
fn test_action_after_match_ends_rejected() {
    let mut game = PokerMatch::new(42).unwrap();

    // Play to completion
    for _ in 0..MAX_HANDS {
        if game.is_terminal() {
            break;
        }
        let sb = game.state().button;
        game.apply_action(sb, &PokerAction::Fold).unwrap();
    }

    let result = game.apply_action(0, &PokerAction::Fold);
    assert!(result.is_err());
}

#[test]
fn test_reraise_min_raise_calculation() {
    let mut game = PokerMatch::new(42).unwrap();
    let state = game.state();
    let sb = state.button;
    let bb = 1 - sb;

    // SB raises to 6 (raise by 4, from BB of 2)
    game.apply_action(sb, &PokerAction::Raise { amount: 6 })
        .unwrap();

    // BB's legal actions should show min re-raise to 10 (6 + 4)
    let legal = game.legal_actions(bb);
    assert!(legal.can_raise);
    assert_eq!(legal.min_raise, 10); // 6 + 4 (last raise increment)
    assert_eq!(legal.call_amount, 4); // 6 - 2 (BB already posted)
}

#[test]
fn test_sequential_state_trait() {
    use eval_runtime::{EnvironmentState, SequentialPhase, SequentialState};

    let game = PokerMatch::new(42).unwrap();
    let state = game.state();

    assert_eq!(state.turn_number(), 1);
    assert!(!state.is_terminal());

    match state.sequential_phase() {
        SequentialPhase::Decision { players, .. } => {
            assert_eq!(players.len(), 1);
            assert_eq!(players[0], state.button); // SB acts first preflop
        }
        other => panic!("expected Decision, got {other:?}"),
    }
}

#[test]
fn test_serialization() {
    let game = PokerMatch::new(42).unwrap();
    let state = game.state();

    // Should serialize/deserialize without errors
    let json = serde_json::to_string(&state).unwrap();
    let _: poker_protocol::MatchState = serde_json::from_str(&json).unwrap();

    let view = game.state_for_player(0);
    let json = serde_json::to_string(&view).unwrap();
    let _: poker_protocol::PlayerMatchView = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_deterministic_hands() {
    // Two matches with same seed should produce identical results
    let mut game1 = PokerMatch::new(123).unwrap();
    let mut game2 = PokerMatch::new(123).unwrap();

    // Play a few hands identically
    for _ in 0..5 {
        let sb = game1.state().button;
        game1.apply_action(sb, &PokerAction::Fold).unwrap();
        let sb = game2.state().button;
        game2.apply_action(sb, &PokerAction::Fold).unwrap();
    }

    assert_eq!(game1.state().profits, game2.state().profits);
    assert_eq!(game1.state().hand_number, game2.state().hand_number);
}
