use coup_engine::{CoupEngineError, CoupGame};
use coup_protocol::{Card, CoupAction, Role, SpectatorEvent, TurnPhase};

fn build_game(player_count: usize) -> CoupGame {
    CoupGame::new(player_count, 42).expect("game should initialize")
}

fn set_player_cards(game: &mut CoupGame, player: i32, roles: &[Role]) {
    let state = game.state_mut();
    let player_state = state.players.get_mut(&player).expect("player exists");
    player_state.cards = roles
        .iter()
        .map(|role| Card {
            role: *role,
            revealed: false,
        })
        .collect();
}

#[test]
fn test_setup_deals_two_cards_each_and_deck_count() {
    let game = build_game(3);
    let state = game.state();

    for player in state.players.values() {
        assert_eq!(player.cards.len(), 2);
        assert_eq!(player.coins, 2);
    }
    assert_eq!(state.deck_count, 15 - 2 * 3);
}

#[test]
fn test_forced_coup_at_ten_coins() {
    let mut game = build_game(3);
    let state = game.state_mut();
    state.active_player = 0;
    state.players.get_mut(&0).expect("player exists").coins = 10;

    let result = game.apply_action(&0, &CoupAction::Income);
    assert!(matches!(result, Err(CoupEngineError::InvalidAction(_))));
}

#[test]
fn test_challenge_success_removes_influence_from_actor() {
    let mut game = build_game(3);
    set_player_cards(&mut game, 0, &[Role::Assassin, Role::Captain]);
    set_player_cards(&mut game, 1, &[Role::Duke, Role::Captain]);

    let _ = game.apply_action(&0, &CoupAction::Tax).expect("tax ok");
    let action_id = game.state().pending_action.as_ref().expect("pending").id;

    let _ = game
        .apply_action(&1, &CoupAction::Challenge { action_id })
        .expect("challenge ok");

    let state = game.state();
    assert!(matches!(
        state.current_phase,
        TurnPhase::SelectingCardToLose { player: 0 }
    ));
}

#[test]
fn test_challenge_failure_forces_challenger_loss() {
    let mut game = build_game(3);
    set_player_cards(&mut game, 0, &[Role::Duke, Role::Captain]);
    set_player_cards(&mut game, 1, &[Role::Assassin, Role::Captain]);

    let _ = game.apply_action(&0, &CoupAction::Tax).expect("tax ok");
    let action_id = game.state().pending_action.as_ref().expect("pending").id;

    let _ = game
        .apply_action(&1, &CoupAction::Challenge { action_id })
        .expect("challenge ok");

    let state = game.state();
    assert!(matches!(
        state.current_phase,
        TurnPhase::RevealingCard {
            player: 0,
            required_role: Role::Duke
        }
    ));
}

#[test]
fn test_block_and_block_challenge_flow() {
    let mut game = build_game(3);
    set_player_cards(&mut game, 1, &[Role::Assassin, Role::Captain]);

    let _ = game
        .apply_action(&0, &CoupAction::ForeignAid)
        .expect("foreign aid ok");
    let action_id = game.state().pending_action.as_ref().expect("pending").id;

    let _ = game
        .apply_action(
            &1,
            &CoupAction::Block {
                action_id,
                claimed_role: Role::Duke,
            },
        )
        .expect("block ok");

    let state = game.state();
    assert!(matches!(
        state.current_phase,
        TurnPhase::BlockChallengeWindow { .. }
    ));

    let _ = game
        .apply_action(&2, &CoupAction::Challenge { action_id })
        .expect("challenge ok");

    let state = game.state();
    assert!(matches!(
        state.current_phase,
        TurnPhase::SelectingCardToLose { player: 1 }
    ));
}

#[test]
fn test_exchange_draw_and_selection() {
    let mut game = build_game(3);
    set_player_cards(&mut game, 0, &[Role::Ambassador, Role::Captain]);

    let _ = game
        .apply_action(&0, &CoupAction::Exchange)
        .expect("exchange ok");
    let _ = game.apply_action(&1, &CoupAction::Pass).expect("pass ok");
    let _ = game.apply_action(&2, &CoupAction::Pass).expect("pass ok");

    let state = game.state();
    assert!(matches!(
        state.current_phase,
        TurnPhase::ExchangeSelection { player: 0 }
    ));
    let actions = game.legal_actions(&0);
    let selection = actions
        .iter()
        .find(|a| matches!(a, CoupAction::ExchangeSelection { .. }))
        .cloned()
        .expect("selection action");

    let _ = game.apply_action(&0, &selection).expect("selection ok");
    let state = game.state();
    assert!(!matches!(
        state.current_phase,
        TurnPhase::ExchangeSelection { .. }
    ));
}

#[test]
fn test_elimination_and_winner() {
    let mut game = build_game(3);
    {
        let state = game.state_mut();
        state.players.get_mut(&2).expect("player exists").eliminated = true;
        let player_state = state.players.get_mut(&1).expect("player exists");
        player_state.cards = vec![
            Card {
                role: Role::Duke,
                revealed: true,
            },
            Card {
                role: Role::Assassin,
                revealed: false,
            },
        ];
        state.current_phase = TurnPhase::SelectingCardToLose { player: 1 };
        state.pending_action = None;
    }

    let _ = game
        .apply_action(&1, &CoupAction::SelectCardToLose { card_index: 1 })
        .expect("select loss ok");

    let state = game.state();
    assert!(matches!(
        state.current_phase,
        TurnPhase::GameOver { winner: 0 }
    ));
}

/// Reproduce the match stall bug: Assassinate target blocks with Contessa,
/// block is challenged successfully, blocker loses last card (eliminated),
/// then Assassinate should auto-resolve (target already eliminated) instead
/// of entering stuck SelectingCardToLose.
#[test]
fn test_auto_skip_card_loss_for_eliminated_player() {
    let mut game = build_game(3);

    // Setup: Player 0 is active, Player 1 has one unrevealed card (vulnerable)
    set_player_cards(&mut game, 0, &[Role::Assassin, Role::Captain]);
    // Player 1: one revealed (already lost), one unrevealed — no Contessa
    {
        let state = game.state_mut();
        let p1 = state.players.get_mut(&1).expect("player exists");
        p1.cards = vec![
            Card {
                role: Role::Duke,
                revealed: true,
            },
            Card {
                role: Role::Captain,
                revealed: false,
            },
        ];
        state.active_player = 0;
        state.players.get_mut(&0).expect("player exists").coins = 3;
    }

    // Player 0 declares Assassinate targeting Player 1
    game.apply_action(&0, &CoupAction::Assassinate { target: 1 })
        .expect("assassinate ok");

    // Challenge window: all pass
    game.apply_action(&1, &CoupAction::Pass).expect("pass ok");
    game.apply_action(&2, &CoupAction::Pass).expect("pass ok");

    // Block window: Player 1 blocks with Contessa (bluff)
    let action_id = game.state().pending_action.as_ref().expect("pending").id;
    game.apply_action(
        &1,
        &CoupAction::Block {
            action_id,
            claimed_role: Role::Contessa,
        },
    )
    .expect("block ok");

    // Block challenge window: Player 0 challenges the block
    game.apply_action(&0, &CoupAction::Challenge { action_id })
        .expect("challenge ok");

    // Player 1 doesn't have Contessa → challenge succeeds.
    // Player 1 enters SelectingCardToLose with only 1 unrevealed card.
    // After losing it, Player 1 is eliminated.
    // Then the Assassinate should auto-resolve (target already eliminated)
    // instead of entering stuck SelectingCardToLose for the target.
    let state = game.state();

    // Player 1 should be in SelectingCardToLose (they have 1 unrevealed card)
    assert!(
        matches!(
            state.current_phase,
            TurnPhase::SelectingCardToLose { player: 1 }
        ),
        "Expected SelectingCardToLose for player 1, got {:?}",
        state.current_phase
    );

    // Player 1 loses their last card
    game.apply_action(&1, &CoupAction::SelectCardToLose { card_index: 1 })
        .expect("select loss ok");

    // After losing last card, Player 1 is eliminated.
    // The block challenge resolution should then resolve the Assassinate.
    // Since Player 1 is already eliminated, enter_card_loss_phase should
    // skip SelectingCardToLose and advance the turn.
    let state = game.state();
    assert!(
        state.players.get(&1).expect("player exists").eliminated,
        "Player 1 should be eliminated"
    );

    // The game should NOT be stuck in SelectingCardToLose for Player 1.
    // It should have advanced (either to AwaitingAction or GameOver).
    assert!(
        !matches!(
            state.current_phase,
            TurnPhase::SelectingCardToLose { player: 1 }
        ),
        "Game should not be stuck in SelectingCardToLose for eliminated player, got {:?}",
        state.current_phase
    );
}

/// Verify that after block challenge succeeds and blocker has no cards,
/// the engine auto-resolves through to the action resolution.
#[test]
fn test_block_challenge_eliminates_blocker_then_resolves_action() {
    let mut game = build_game(3);

    // Player 0 is active with 3 coins (for Assassinate)
    // Player 1 has only 1 unrevealed card (will be eliminated by losing it)
    // Player 2 has 2 unrevealed cards
    {
        let state = game.state_mut();
        state.active_player = 0;
        state.players.get_mut(&0).expect("p0").coins = 3;
    }
    set_player_cards(&mut game, 0, &[Role::Assassin, Role::Duke]);
    // Player 1: 1 revealed, 1 unrevealed, no Contessa
    {
        let p1 = game.state_mut().players.get_mut(&1).expect("p1");
        p1.cards = vec![
            Card {
                role: Role::Ambassador,
                revealed: true,
            },
            Card {
                role: Role::Captain,
                revealed: false,
            },
        ];
    }
    set_player_cards(&mut game, 2, &[Role::Duke, Role::Ambassador]);

    // Assassinate Player 1
    game.apply_action(&0, &CoupAction::Assassinate { target: 1 })
        .expect("ok");
    game.apply_action(&1, &CoupAction::Pass).expect("ok");
    game.apply_action(&2, &CoupAction::Pass).expect("ok");

    let action_id = game.state().pending_action.as_ref().expect("pending").id;

    // Player 1 blocks with Contessa (bluff)
    game.apply_action(
        &1,
        &CoupAction::Block {
            action_id,
            claimed_role: Role::Contessa,
        },
    )
    .expect("ok");

    // Player 0 challenges the block
    game.apply_action(&0, &CoupAction::Challenge { action_id })
        .expect("ok");

    // Player 1 doesn't have Contessa → enters SelectingCardToLose
    assert!(matches!(
        game.state().current_phase,
        TurnPhase::SelectingCardToLose { player: 1 }
    ));

    // Player 1 loses their last unrevealed card → eliminated
    game.apply_action(&1, &CoupAction::SelectCardToLose { card_index: 1 })
        .expect("ok");

    // Player 1 should be eliminated, and the assassinate should auto-resolve
    // (target already eliminated, no card to lose) → turn should advance
    let state = game.state();
    assert!(state.players.get(&1).expect("p1").eliminated);
    assert!(
        matches!(
            state.current_phase,
            TurnPhase::AwaitingAction | TurnPhase::GameOver { .. }
        ),
        "Expected AwaitingAction or GameOver after auto-resolve, got {:?}",
        state.current_phase
    );
}

#[test]
fn test_event_history_stored_and_returned() {
    let mut game = build_game(3);

    // Initial events should only contain GameStarted
    let initial = game.initial_events();
    assert_eq!(initial.len(), 1);
    assert!(matches!(initial[0], SpectatorEvent::GameStarted { .. }));

    // Perform an action
    let outcome = game
        .apply_action(&0, &CoupAction::Income)
        .expect("income ok");
    assert!(!outcome.events.is_empty());

    // Now initial_events should include GameStarted + the income events
    let after_income = game.initial_events();
    assert!(
        after_income.len() > 1,
        "Expected event history to grow after action, got {} events",
        after_income.len()
    );
    assert!(matches!(
        after_income[0],
        SpectatorEvent::GameStarted { .. }
    ));
    assert!(matches!(
        after_income[1],
        SpectatorEvent::ActionDeclared { .. }
    ));
}
