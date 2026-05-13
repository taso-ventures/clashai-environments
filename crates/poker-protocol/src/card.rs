use std::fmt;

use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

/// Card suits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    pub fn symbol(self) -> char {
        match self {
            Suit::Clubs => 'c',
            Suit::Diamonds => 'd',
            Suit::Hearts => 'h',
            Suit::Spades => 's',
        }
    }
}

impl fmt::Display for Suit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

/// Card ranks (2..=Ace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rank {
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Jack = 11,
    Queen = 12,
    King = 13,
    Ace = 14,
}

impl Rank {
    pub const ALL: [Rank; 13] = [
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ];

    pub fn symbol(self) -> char {
        match self {
            Rank::Two => '2',
            Rank::Three => '3',
            Rank::Four => '4',
            Rank::Five => '5',
            Rank::Six => '6',
            Rank::Seven => '7',
            Rank::Eight => '8',
            Rank::Nine => '9',
            Rank::Ten => 'T',
            Rank::Jack => 'J',
            Rank::Queen => 'Q',
            Rank::King => 'K',
            Rank::Ace => 'A',
        }
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

/// A playing card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Card {
    pub rank: Rank,
    pub suit: Suit,
}

impl Card {
    pub fn new(rank: Rank, suit: Suit) -> Self {
        Self { rank, suit }
    }

    /// Short notation like "Ah", "Td", "2c".
    pub fn short_name(&self) -> String {
        format!("{}{}", self.rank.symbol(), self.suit.symbol())
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank.symbol(), self.suit.symbol())
    }
}

/// A standard 52-card deck with deterministic shuffling.
pub struct Deck {
    cards: Vec<Card>,
    position: usize,
}

impl Deck {
    /// Create a new deck shuffled with the given seed.
    pub fn new_shuffled(seed: u64) -> Self {
        let mut cards = Vec::with_capacity(52);
        for &suit in &Suit::ALL {
            for &rank in &Rank::ALL {
                cards.push(Card::new(rank, suit));
            }
        }
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        cards.shuffle(&mut rng);
        Self { cards, position: 0 }
    }

    /// Deal the next card from the deck.
    ///
    /// Returns `None` if the deck is exhausted.
    pub fn deal(&mut self) -> Option<Card> {
        if self.position < self.cards.len() {
            let card = self.cards[self.position];
            self.position += 1;
            Some(card)
        } else {
            None
        }
    }

    /// Deal `n` cards.
    ///
    /// Returns `None` if not enough cards remain.
    pub fn deal_n(&mut self, n: usize) -> Option<Vec<Card>> {
        if self.position + n <= self.cards.len() {
            let cards = self.cards[self.position..self.position + n].to_vec();
            self.position += n;
            Some(cards)
        } else {
            None
        }
    }
}

// =====================
// Hand evaluation
// =====================

/// Hand ranking categories, ordered from worst to best.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandCategory {
    HighCard = 0,
    OnePair = 1,
    TwoPair = 2,
    ThreeOfAKind = 3,
    Straight = 4,
    Flush = 5,
    FullHouse = 6,
    FourOfAKind = 7,
    StraightFlush = 8,
}

impl fmt::Display for HandCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandCategory::HighCard => write!(f, "High Card"),
            HandCategory::OnePair => write!(f, "One Pair"),
            HandCategory::TwoPair => write!(f, "Two Pair"),
            HandCategory::ThreeOfAKind => write!(f, "Three of a Kind"),
            HandCategory::Straight => write!(f, "Straight"),
            HandCategory::Flush => write!(f, "Flush"),
            HandCategory::FullHouse => write!(f, "Full House"),
            HandCategory::FourOfAKind => write!(f, "Four of a Kind"),
            HandCategory::StraightFlush => write!(f, "Straight Flush"),
        }
    }
}

/// A comparable hand score. Higher is better.
///
/// The primary comparison is by `category`, then by `kickers` lexicographically.
/// `kickers` encodes the relevant rank values in descending priority order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HandScore {
    pub category: HandCategory,
    /// Rank values used for tiebreaking, highest priority first.
    pub kickers: Vec<u8>,
}

impl fmt::Display for HandScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.category)
    }
}

/// Evaluate the best 5-card hand from exactly 5 cards.
fn evaluate_five(cards: &[Card; 5]) -> HandScore {
    let mut ranks: Vec<u8> = cards.iter().map(|c| c.rank as u8).collect();
    ranks.sort_unstable_by(|a, b| b.cmp(a)); // descending

    let is_flush = cards.iter().all(|c| c.suit == cards[0].suit);

    // Check for straight (including A-2-3-4-5 wheel)
    let is_straight = is_consecutive(&ranks);
    let is_wheel = ranks == [14, 5, 4, 3, 2];

    if is_straight || is_wheel {
        let high = if is_wheel { 5u8 } else { ranks[0] };
        if is_flush {
            return HandScore {
                category: HandCategory::StraightFlush,
                kickers: vec![high],
            };
        }
        return HandScore {
            category: HandCategory::Straight,
            kickers: vec![high],
        };
    }

    if is_flush {
        return HandScore {
            category: HandCategory::Flush,
            kickers: ranks,
        };
    }

    // Count rank frequencies
    let mut counts: Vec<(u8, u8)> = rank_counts(&ranks);
    // Sort by count desc, then rank desc
    counts.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

    match counts[0].0 {
        4 => HandScore {
            category: HandCategory::FourOfAKind,
            kickers: vec![counts[0].1, counts[1].1],
        },
        3 => {
            if counts[1].0 == 2 {
                HandScore {
                    category: HandCategory::FullHouse,
                    kickers: vec![counts[0].1, counts[1].1],
                }
            } else {
                HandScore {
                    category: HandCategory::ThreeOfAKind,
                    kickers: vec![counts[0].1, counts[1].1, counts[2].1],
                }
            }
        }
        2 => {
            if counts[1].0 == 2 {
                // Two pair: higher pair first, then lower pair, then kicker
                let high_pair = counts[0].1.max(counts[1].1);
                let low_pair = counts[0].1.min(counts[1].1);
                HandScore {
                    category: HandCategory::TwoPair,
                    kickers: vec![high_pair, low_pair, counts[2].1],
                }
            } else {
                HandScore {
                    category: HandCategory::OnePair,
                    kickers: vec![counts[0].1, counts[1].1, counts[2].1, counts[3].1],
                }
            }
        }
        _ => HandScore {
            category: HandCategory::HighCard,
            kickers: ranks,
        },
    }
}

fn is_consecutive(ranks: &[u8]) -> bool {
    if ranks.len() < 2 {
        return true;
    }
    for i in 0..ranks.len() - 1 {
        if ranks[i] != ranks[i + 1] + 1 {
            return false;
        }
    }
    true
}

fn rank_counts(ranks: &[u8]) -> Vec<(u8, u8)> {
    let mut map = std::collections::HashMap::new();
    for &r in ranks {
        *map.entry(r).or_insert(0u8) += 1;
    }
    map.into_iter().map(|(rank, count)| (count, rank)).collect()
}

/// Evaluate the best 5-card hand from 7 cards (2 hole + 5 community).
///
/// Tries all C(7,5) = 21 combinations and returns the best.
///
/// Returns an error if fewer than 5 cards are provided.
pub fn evaluate_hand(cards: &[Card]) -> Result<HandScore, String> {
    if cards.len() < 5 {
        return Err(format!(
            "need at least 5 cards to evaluate, got {}",
            cards.len()
        ));
    }

    let n = cards.len();
    let mut best: Option<HandScore> = None;

    // Generate all C(n,5) combinations
    for i in 0..n {
        for j in (i + 1)..n {
            for k in (j + 1)..n {
                for l in (k + 1)..n {
                    for m in (l + 1)..n {
                        let five = [cards[i], cards[j], cards[k], cards[l], cards[m]];
                        let score = evaluate_five(&five);
                        best = Some(match best {
                            Some(ref b) if score > *b => score,
                            Some(b) => b,
                            None => score,
                        });
                    }
                }
            }
        }
    }

    best.ok_or_else(|| "no 5-card combinations found".to_string())
}
