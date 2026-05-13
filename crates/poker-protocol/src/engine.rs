use thiserror::Error;

use crate::card::{evaluate_hand, Card, Deck};
use crate::{
    BettingRound, HandResult, HandState, LegalActions, MatchPhase, MatchState, PlayerAction,
    PlayerHandView, PlayerId, PlayerMatchView, PokerAction, BIG_BLIND, INITIAL_STACK, MAX_HANDS,
    SMALL_BLIND,
};

#[derive(Debug, Error)]
pub enum PokerError {
    #[error("invalid action: {0}")]
    InvalidAction(String),
    #[error("not player's turn: expected {expected}, got {got}")]
    NotPlayerTurn { expected: PlayerId, got: PlayerId },
    #[error("match already finished")]
    MatchFinished,
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, PokerError>;

/// Heads-Up No-Limit Texas Hold'em match engine.
///
/// Runs multiple hands between two players, tracking cumulative profit.
/// Stacks reset each hand to `INITIAL_STACK`.
#[derive(Debug, Clone)]
pub struct PokerMatch {
    seed: u64,
    hand_number: u32,
    /// Cumulative profit for each player across all hands.
    profits: [i32; 2],
    /// Match-level stacks. Each player starts with [`INITIAL_STACK`]; each
    /// completed hand's profits flow into these. Match ends when a player
    /// is busted (stack <= 0) OR when [`MAX_HANDS`] is reached.
    match_stacks: [i32; 2],
    /// Current hand state, `None` between hands or when match is over.
    current_hand: Option<HandEngine>,
    /// Completed hand results.
    hand_history: Vec<HandResult>,
    /// Match phase.
    phase: MatchPhase,
    /// Button position (alternates each hand). 0 or 1.
    button: PlayerId,
}

impl PokerMatch {
    /// Create a new match with the given seed.
    pub fn new(seed: u64) -> Result<Self> {
        let mut m = Self {
            seed,
            hand_number: 0,
            profits: [0, 0],
            match_stacks: [INITIAL_STACK, INITIAL_STACK],
            current_hand: None,
            hand_history: Vec::new(),
            phase: MatchPhase::PreMatch,
            button: 0,
        };
        m.start_next_hand()?;
        Ok(m)
    }

    /// Start the next hand.
    fn start_next_hand(&mut self) -> Result<()> {
        self.hand_number += 1;
        self.button = if self.hand_number % 2 == 1 { 0 } else { 1 };
        // Per-hand seed: base_seed XOR hand_number for reproducibility
        let hand_seed = self.seed ^ (self.hand_number as u64);
        // Each hand starts with the match-level stacks (tournament-style
        // carryover), not a fresh INITIAL_STACK. Busted players cannot
        // start another hand — the caller is expected to check elimination.
        self.current_hand = Some(HandEngine::new(hand_seed, self.button, self.match_stacks)?);
        self.phase = MatchPhase::Playing;
        Ok(())
    }

    /// Get the full match state (for admin/spectator).
    pub fn state(&self) -> MatchState {
        MatchState {
            hand_number: self.hand_number,
            max_hands: MAX_HANDS,
            profits: self.profits,
            phase: self.phase,
            button: self.button,
            current_hand: self.current_hand.as_ref().map(|h| h.state()),
            hand_history: self.hand_history.clone(),
        }
    }

    /// Get a player-filtered view of the match state.
    pub fn state_for_player(&self, player_id: PlayerId) -> PlayerMatchView {
        PlayerMatchView {
            player_id,
            hand_number: self.hand_number,
            max_hands: MAX_HANDS,
            your_profit: self.profits[player_id as usize],
            opponent_profit: self.profits[1 - player_id as usize],
            your_stack: self.match_stacks[player_id as usize],
            opponent_stack: self.match_stacks[1 - player_id as usize],
            phase: self.phase,
            button: self.button,
            current_hand: self.current_hand.as_ref().map(|h| h.player_view(player_id)),
            last_hand_result: self.hand_history.last().cloned(),
        }
    }

    /// Get legal actions for the given player.
    pub fn legal_actions(&self, player_id: PlayerId) -> LegalActions {
        match &self.current_hand {
            Some(hand) if hand.active_player() == player_id && !hand.is_finished() => {
                hand.legal_actions()
            }
            _ => LegalActions::none(),
        }
    }

    /// Apply an action from a player.
    pub fn apply_action(&mut self, player_id: PlayerId, action: &PokerAction) -> Result<()> {
        if self.is_terminal() {
            return Err(PokerError::MatchFinished);
        }

        let hand = self
            .current_hand
            .as_mut()
            .ok_or_else(|| PokerError::Internal("no active hand".into()))?;

        if hand.active_player() != player_id {
            return Err(PokerError::NotPlayerTurn {
                expected: hand.active_player(),
                got: player_id,
            });
        }

        hand.apply_action(player_id, action)?;

        // Check if hand is finished
        if hand.is_finished() {
            let mut result = hand
                .result()?
                .ok_or_else(|| PokerError::Internal("hand finished but no result".into()))?;
            result.hand_number = self.hand_number;

            // Update cumulative profits and match-level stacks.
            self.profits[0] += result.profits[0];
            self.profits[1] += result.profits[1];
            self.match_stacks[0] += result.profits[0];
            self.match_stacks[1] += result.profits[1];
            self.hand_history.push(result);

            // End match if either player is busted OR we've reached the cap.
            let eliminated = self.match_stacks[0] <= 0 || self.match_stacks[1] <= 0;
            if eliminated || self.hand_number >= MAX_HANDS {
                self.phase = MatchPhase::Completed;
                self.current_hand = None;
            } else {
                self.start_next_hand()?;
            }
        }

        Ok(())
    }

    /// Whether the match has ended.
    pub fn is_terminal(&self) -> bool {
        matches!(self.phase, MatchPhase::Completed)
    }

    /// Return the winner (player with higher profit), or `None` if draw or not finished.
    pub fn winner(&self) -> Option<PlayerId> {
        if !self.is_terminal() {
            return None;
        }
        if self.profits[0] > self.profits[1] {
            Some(0)
        } else if self.profits[1] > self.profits[0] {
            Some(1)
        } else {
            None // draw
        }
    }
}

// =====================
// Single hand engine
// =====================

/// Engine for a single poker hand.
#[derive(Debug, Clone)]
pub(crate) struct HandEngine {
    /// Player hole cards: [player0_cards, player1_cards].
    hole_cards: [[Card; 2]; 2],
    /// Community cards dealt so far.
    community: Vec<Card>,
    /// Remaining deck cards after dealing hole cards.
    deck: Vec<Card>,
    deck_position: usize,
    /// Current betting round.
    round: BettingRound,
    /// Player stacks (chips remaining, not counting current bets).
    stacks: [i32; 2],
    /// Current street bets for each player.
    street_bets: [i32; 2],
    /// Total pot contributions across all streets.
    pot_contributions: [i32; 2],
    /// Who has the button (SB in HU).
    button: PlayerId,
    /// Whose turn to act.
    action_on: PlayerId,
    /// Whether a player has folded.
    folded: [bool; 2],
    /// Whether the hand is finished.
    finished: bool,
    /// Number of actions taken on current street.
    street_actions: u32,
    /// Last raise increment on current street (for min-raise calculation).
    last_raise_size: i32,
    /// Action history for this hand.
    action_history: Vec<(PlayerId, PlayerAction)>,
}

impl HandEngine {
    /// Create a new hand with the given seed, button position, and the
    /// carried-over stacks from the previous hand (tournament mode).
    pub(crate) fn new(seed: u64, button: PlayerId, starting_stacks: [i32; 2]) -> Result<Self> {
        let mut deck = Deck::new_shuffled(seed);

        // Deal hole cards: button (SB) first, then BB
        let sb = button;
        let bb = 1 - button;

        let sb_card1 = deck
            .deal()
            .ok_or_else(|| PokerError::Internal("deck empty dealing SB card1".into()))?;
        let bb_card1 = deck
            .deal()
            .ok_or_else(|| PokerError::Internal("deck empty dealing BB card1".into()))?;
        let sb_card2 = deck
            .deal()
            .ok_or_else(|| PokerError::Internal("deck empty dealing SB card2".into()))?;
        let bb_card2 = deck
            .deal()
            .ok_or_else(|| PokerError::Internal("deck empty dealing BB card2".into()))?;

        let mut hole_cards = [[sb_card1, sb_card2]; 2];
        hole_cards[sb as usize] = [sb_card1, sb_card2];
        hole_cards[bb as usize] = [bb_card1, bb_card2];

        // Post blinds from the carried-over stacks. If a player has less
        // than the full blind, they post what they have (all-in blind).
        let mut stacks = starting_stacks;
        let sb_post = stacks[sb as usize].min(SMALL_BLIND);
        stacks[sb as usize] -= sb_post;
        let bb_post = stacks[bb as usize].min(BIG_BLIND);
        stacks[bb as usize] -= bb_post;

        // Remaining deck cards
        let remaining = deck
            .deal_n(52 - 4)
            .ok_or_else(|| PokerError::Internal("not enough cards in deck".into()))?;

        // In HU, SB (button) acts first preflop
        let action_on = sb;

        let mut street_bets = [0i32; 2];
        street_bets[sb as usize] = sb_post;
        street_bets[bb as usize] = bb_post;
        let pot_contributions = street_bets;

        Ok(Self {
            hole_cards,
            community: Vec::new(),
            deck: remaining,
            deck_position: 0,
            round: BettingRound::Preflop,
            stacks,
            street_bets,
            pot_contributions,
            button,
            action_on,
            folded: [false; 2],
            finished: false,
            street_actions: 0,
            last_raise_size: BIG_BLIND,
            action_history: Vec::new(),
        })
    }

    pub(crate) fn active_player(&self) -> PlayerId {
        self.action_on
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished
    }

    /// Get the full hand state.
    pub(crate) fn state(&self) -> HandState {
        HandState {
            hole_cards: self.hole_cards,
            community: self.community.clone(),
            round: self.round,
            stacks: self.stacks,
            pot: self.pot_contributions[0] + self.pot_contributions[1],
            street_bets: self.street_bets,
            pot_contributions: self.pot_contributions,
            button: self.button,
            action_on: self.action_on,
            folded: self.folded,
            finished: self.finished,
            action_history: self.action_history.clone(),
        }
    }

    /// Get a player-filtered view (hides opponent's hole cards).
    pub(crate) fn player_view(&self, player_id: PlayerId) -> PlayerHandView {
        PlayerHandView {
            your_cards: self.hole_cards[player_id as usize],
            community: self.community.clone(),
            round: self.round,
            your_stack: self.stacks[player_id as usize],
            opponent_stack: self.stacks[1 - player_id as usize],
            pot: self.pot_contributions[0] + self.pot_contributions[1],
            your_street_bet: self.street_bets[player_id as usize],
            opponent_street_bet: self.street_bets[1 - player_id as usize],
            button: self.button,
            action_on: self.action_on,
            folded: self.folded,
            finished: self.finished,
            action_history: self.action_history.clone(),
        }
    }

    /// Get legal actions for the active player.
    pub(crate) fn legal_actions(&self) -> LegalActions {
        if self.finished {
            return LegalActions::none();
        }

        let player = self.action_on;
        let my_street_bet = self.street_bets[player as usize];
        let opp_street_bet = self.street_bets[1 - player as usize];
        let my_stack = self.stacks[player as usize];
        let call_amount = opp_street_bet - my_street_bet;

        let can_fold = call_amount > 0;
        let can_check = call_amount == 0;
        // Short all-in call: a player with fewer chips than the outstanding
        // bet can still call for whatever they have left. Uncalled chips
        // from the raiser are refunded at hand end.
        let can_call = call_amount > 0 && my_stack > 0;

        // Min raise: must raise by at least the last raise increment (or BB)
        let min_raise_increment = self.last_raise_size.max(BIG_BLIND);
        let min_raise_total = opp_street_bet + min_raise_increment;
        // Max raise = all-in
        let max_raise_total = my_street_bet + my_stack;

        // Can raise if we have chips beyond the call amount and can meet min raise
        // (or if all-in is less than min raise, we can still go all-in)
        let can_raise = my_stack > call_amount && max_raise_total > opp_street_bet;

        let effective_min_raise = if can_raise {
            min_raise_total.min(max_raise_total)
        } else {
            0
        };

        LegalActions {
            can_fold,
            can_check,
            can_call,
            call_amount: if can_call { call_amount } else { 0 },
            can_raise,
            min_raise: effective_min_raise,
            max_raise: if can_raise { max_raise_total } else { 0 },
        }
    }

    /// Apply a player action.
    pub(crate) fn apply_action(&mut self, player_id: PlayerId, action: &PokerAction) -> Result<()> {
        if self.finished {
            return Err(PokerError::InvalidAction("hand is finished".into()));
        }
        if player_id != self.action_on {
            return Err(PokerError::NotPlayerTurn {
                expected: self.action_on,
                got: player_id,
            });
        }

        let legal = self.legal_actions();

        match action {
            PokerAction::Fold => {
                if !legal.can_fold {
                    return Err(PokerError::InvalidAction(
                        "cannot fold when no bet to call".into(),
                    ));
                }
                self.folded[player_id as usize] = true;
                self.action_history.push((player_id, PlayerAction::Fold));
                self.finish_hand();
            }
            PokerAction::Check => {
                if !legal.can_check {
                    return Err(PokerError::InvalidAction(
                        "cannot check when facing a bet".into(),
                    ));
                }
                self.action_history.push((player_id, PlayerAction::Check));
                self.street_actions += 1;
                self.try_advance_street();
            }
            PokerAction::Call => {
                if !legal.can_call {
                    return Err(PokerError::InvalidAction("cannot call".into()));
                }
                let call_amount = legal.call_amount.min(self.stacks[player_id as usize]);
                self.stacks[player_id as usize] -= call_amount;
                self.street_bets[player_id as usize] += call_amount;
                self.pot_contributions[player_id as usize] += call_amount;
                self.action_history
                    .push((player_id, PlayerAction::Call(call_amount)));
                self.street_actions += 1;
                self.try_advance_street();
            }
            PokerAction::Raise { amount } => {
                if !legal.can_raise {
                    return Err(PokerError::InvalidAction("cannot raise".into()));
                }
                let total_bet = *amount;
                if total_bet < legal.min_raise || total_bet > legal.max_raise {
                    return Err(PokerError::InvalidAction(format!(
                        "raise {total_bet} not in range [{}, {}]",
                        legal.min_raise, legal.max_raise
                    )));
                }
                let additional = total_bet - self.street_bets[player_id as usize];
                let raise_increment = total_bet - self.street_bets[1 - player_id as usize];
                self.last_raise_size = raise_increment;
                self.stacks[player_id as usize] -= additional;
                self.street_bets[player_id as usize] = total_bet;
                self.pot_contributions[player_id as usize] += additional;
                self.action_history
                    .push((player_id, PlayerAction::Raise(total_bet)));
                self.street_actions += 1;
                // After a raise, action goes to opponent
                self.action_on = 1 - player_id;
            }
        }

        Ok(())
    }

    /// Check if the current betting round should advance to the next street.
    fn try_advance_street(&mut self) {
        let bets_equal = self.street_bets[0] == self.street_bets[1];
        let someone_all_in = self.stacks[0] == 0 || self.stacks[1] == 0;

        if bets_equal && self.street_actions >= 2 {
            if someone_all_in {
                // Deal remaining community cards and go to showdown
                self.deal_remaining_community();
                self.finish_hand();
            } else {
                self.advance_to_next_street();
            }
        } else if someone_all_in && bets_equal {
            self.deal_remaining_community();
            self.finish_hand();
        } else {
            // Action continues to the other player
            self.action_on = 1 - self.action_on;
        }
    }

    fn advance_to_next_street(&mut self) {
        match self.round {
            BettingRound::Preflop => {
                self.deal_community(3);
                self.round = BettingRound::Flop;
            }
            BettingRound::Flop => {
                self.deal_community(1);
                self.round = BettingRound::Turn;
            }
            BettingRound::Turn => {
                self.deal_community(1);
                self.round = BettingRound::River;
            }
            BettingRound::River => {
                self.finish_hand();
                return;
            }
        }

        // Reset street state
        self.street_bets = [0; 2];
        self.street_actions = 0;
        self.last_raise_size = BIG_BLIND;

        // Postflop: BB (non-button) acts first in HU
        self.action_on = 1 - self.button;
    }

    fn deal_community(&mut self, count: usize) {
        for _ in 0..count {
            if self.deck_position < self.deck.len() {
                self.community.push(self.deck[self.deck_position]);
                self.deck_position += 1;
            }
        }
    }

    fn deal_remaining_community(&mut self) {
        let needed = 5 - self.community.len();
        self.deal_community(needed);
    }

    fn finish_hand(&mut self) {
        // Refund uncalled chips. In HU, if one player has contributed more
        // to the pot than the other (typically because the opponent went
        // all-in for less), the excess is not in play — return it to the
        // contributor before computing the result.
        let effective = self.pot_contributions[0].min(self.pot_contributions[1]);
        for p in 0..2 {
            let excess = self.pot_contributions[p] - effective;
            if excess > 0 {
                self.pot_contributions[p] = effective;
                self.stacks[p] += excess;
                // Also flush the street bet display for cleanliness.
                if self.street_bets[p] >= excess {
                    self.street_bets[p] -= excess;
                }
            }
        }
        self.finished = true;
    }

    /// Get the hand result (only valid when finished).
    pub(crate) fn result(&self) -> Result<Option<HandResult>> {
        if !self.finished {
            return Ok(None);
        }

        let pot = self.pot_contributions[0] + self.pot_contributions[1];
        let mut profits = [0i32; 2];

        if self.folded[0] {
            profits[1] = self.pot_contributions[0];
            profits[0] = -self.pot_contributions[0];
            Ok(Some(HandResult {
                hand_number: 0,
                winner: Some(1),
                profits,
                pot,
                hole_cards: self.hole_cards,
                community: self.community.clone(),
                showdown: false,
                winning_hand: None,
            }))
        } else if self.folded[1] {
            profits[0] = self.pot_contributions[1];
            profits[1] = -self.pot_contributions[1];
            Ok(Some(HandResult {
                hand_number: 0,
                winner: Some(0),
                profits,
                pot,
                hole_cards: self.hole_cards,
                community: self.community.clone(),
                showdown: false,
                winning_hand: None,
            }))
        } else {
            // Showdown
            let hand0: Vec<Card> = self.hole_cards[0]
                .iter()
                .chain(self.community.iter())
                .copied()
                .collect();
            let hand1: Vec<Card> = self.hole_cards[1]
                .iter()
                .chain(self.community.iter())
                .copied()
                .collect();
            let score0 = evaluate_hand(&hand0)
                .map_err(|e| PokerError::InvalidAction(format!("showdown eval P0: {e}")))?;
            let score1 = evaluate_hand(&hand1)
                .map_err(|e| PokerError::InvalidAction(format!("showdown eval P1: {e}")))?;

            let (winner, winning_hand) = if score0 > score1 {
                profits[0] = self.pot_contributions[1];
                profits[1] = -self.pot_contributions[1];
                (Some(0), Some(score0))
            } else if score1 > score0 {
                profits[1] = self.pot_contributions[0];
                profits[0] = -self.pot_contributions[0];
                (Some(1), Some(score1))
            } else {
                // Split pot
                (None, Some(score0))
            };

            Ok(Some(HandResult {
                hand_number: 0,
                winner,
                profits,
                pot,
                hole_cards: self.hole_cards,
                community: self.community.clone(),
                showdown: true,
                winning_hand,
            }))
        }
    }
}
