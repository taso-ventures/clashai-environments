use poker_protocol::card::{evaluate_hand, Card, Deck, HandCategory, Rank, Suit};

#[test]
fn test_royal_flush() {
    let cards = vec![
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::King, Suit::Spades),
        Card::new(Rank::Queen, Suit::Spades),
        Card::new(Rank::Jack, Suit::Spades),
        Card::new(Rank::Ten, Suit::Spades),
    ];
    let score = evaluate_hand(&cards).unwrap();
    assert_eq!(score.category, HandCategory::StraightFlush);
    assert_eq!(score.kickers, vec![14]);
}

#[test]
fn test_wheel_straight() {
    let cards = vec![
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::Two, Suit::Hearts),
        Card::new(Rank::Three, Suit::Diamonds),
        Card::new(Rank::Four, Suit::Clubs),
        Card::new(Rank::Five, Suit::Spades),
    ];
    let score = evaluate_hand(&cards).unwrap();
    assert_eq!(score.category, HandCategory::Straight);
    assert_eq!(score.kickers, vec![5]); // 5-high straight
}

#[test]
fn test_full_house() {
    let cards = vec![
        Card::new(Rank::King, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
        Card::new(Rank::King, Suit::Diamonds),
        Card::new(Rank::Two, Suit::Clubs),
        Card::new(Rank::Two, Suit::Spades),
    ];
    let score = evaluate_hand(&cards).unwrap();
    assert_eq!(score.category, HandCategory::FullHouse);
    assert_eq!(score.kickers, vec![13, 2]);
}

#[test]
fn test_seven_card_evaluation() {
    // 7 cards: pair of aces + random cards, best hand should include the pair
    let cards = vec![
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::Ace, Suit::Hearts),
        Card::new(Rank::King, Suit::Diamonds),
        Card::new(Rank::Queen, Suit::Clubs),
        Card::new(Rank::Five, Suit::Spades),
        Card::new(Rank::Three, Suit::Hearts),
        Card::new(Rank::Two, Suit::Diamonds),
    ];
    let score = evaluate_hand(&cards).unwrap();
    assert_eq!(score.category, HandCategory::OnePair);
    assert_eq!(score.kickers[0], 14); // pair of aces
}

#[test]
fn test_deck_deterministic() {
    // Two decks seeded identically must deal identical sequences.
    let mut d1 = Deck::new_shuffled(42);
    let mut d2 = Deck::new_shuffled(42);
    for _ in 0..52 {
        assert_eq!(d1.deal(), d2.deal());
    }
}

#[test]
fn test_deck_deal() {
    let mut deck = Deck::new_shuffled(0);
    let mut dealt = Vec::new();
    for _ in 0..52 {
        dealt.push(deck.deal().unwrap());
    }
    assert!(deck.deal().is_none());
    // All 52 unique cards
    let unique: std::collections::HashSet<_> = dealt.iter().collect();
    assert_eq!(unique.len(), 52);
}
