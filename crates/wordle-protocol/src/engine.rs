use std::collections::{HashMap, HashSet};

use chrono::Utc;
use thiserror::Error;

use crate::{
    word_list, ChatMessage, ChatPhase, GuessResult, LetterFeedback, OpponentSummary, PlayerId,
    PlayerProgress, TerminalReason, WordleAction, WordleConfig, WordleFullState, WordlePhase,
    WordlePlayerView,
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("invalid setup: {0}")]
    InvalidSetup(String),
    #[error("unknown player id: {0}")]
    UnknownPlayer(PlayerId),
    #[error("invalid phase for action")]
    InvalidPhase,
    #[error("message too long: {len} chars (max {max})")]
    MessageTooLong { len: usize, max: usize },
    #[error("lobby message already sent")]
    LobbyMessageAlreadySent,
    #[error("banter message already sent")]
    BanterMessageAlreadySent,
    #[error("win message not allowed for this player")]
    WinMessageNotAllowed,
    #[error("already guessed this turn")]
    AlreadyGuessedThisTurn,
    #[error("player cannot guess")]
    GuessNotAllowed,
    #[error("invalid guess word: {0}")]
    InvalidGuessWord(String),
}

pub struct WordleGame {
    config: WordleConfig,
    target_word: String,
    players: Vec<PlayerProgress>,
    chat_messages: Vec<ChatMessage>,
    turn: u32,
    phase: WordlePhase,
    solve_order: Vec<PlayerId>,
    terminal_reason: Option<TerminalReason>,
    guessed_this_turn: HashSet<PlayerId>,
    sent_lobby_message: HashSet<PlayerId>,
    sent_win_message: HashSet<PlayerId>,
    sent_banter_message: HashSet<PlayerId>,
    valid_words: &'static HashSet<String>,
}

impl WordleGame {
    pub fn new(
        player_ids: Vec<PlayerId>,
        player_names: HashMap<PlayerId, String>,
        config: WordleConfig,
        seed: u64,
    ) -> Result<Self, EngineError> {
        Self::new_with_target(
            player_ids,
            player_names,
            config,
            word_list::select_word(seed),
        )
    }

    pub fn new_with_target(
        player_ids: Vec<PlayerId>,
        player_names: HashMap<PlayerId, String>,
        config: WordleConfig,
        target_word: String,
    ) -> Result<Self, EngineError> {
        if !(3..=6).contains(&player_ids.len()) {
            return Err(EngineError::InvalidSetup(
                "wordle requires 3 to 6 players".to_string(),
            ));
        }
        if config.max_guesses == 0 {
            return Err(EngineError::InvalidSetup(
                "max_guesses must be greater than 0".to_string(),
            ));
        }
        let unique_ids: HashSet<PlayerId> = player_ids.iter().copied().collect();
        if unique_ids.len() != player_ids.len() {
            return Err(EngineError::InvalidSetup(
                "player_ids must be unique".to_string(),
            ));
        }

        let normalized_target = target_word.trim().to_lowercase();
        if normalized_target.chars().count() != 5
            || !normalized_target.chars().all(|c| c.is_ascii_alphabetic())
        {
            return Err(EngineError::InvalidSetup(
                "target word must be 5 ASCII letters".to_string(),
            ));
        }

        let players = player_ids
            .into_iter()
            .map(|player_id| PlayerProgress {
                player_id,
                display_name: player_names
                    .get(&player_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Player {player_id}")),
                guesses: Vec::new(),
                solved: false,
                eliminated: false,
                solved_turn: None,
            })
            .collect();

        Ok(Self {
            config,
            target_word: normalized_target,
            players,
            chat_messages: Vec::new(),
            turn: 0,
            phase: WordlePhase::Lobby,
            solve_order: Vec::new(),
            terminal_reason: None,
            guessed_this_turn: HashSet::new(),
            sent_lobby_message: HashSet::new(),
            sent_win_message: HashSet::new(),
            sent_banter_message: HashSet::new(),
            valid_words: word_list::valid_word_set(),
        })
    }

    pub fn full_state(&self) -> WordleFullState {
        let revealed = matches!(self.phase, WordlePhase::Banter | WordlePhase::GameOver);
        WordleFullState {
            target_word: if revealed {
                Some(self.target_word.clone())
            } else {
                None
            },
            turn: self.turn,
            phase: self.phase,
            players: self.players.clone(),
            chat_messages: self.chat_messages.clone(),
            is_terminal: self.is_terminal(),
            terminal_reason: self.terminal_reason,
            solve_order: self.solve_order.clone(),
        }
    }

    pub fn state_for_player(&self, player_id: PlayerId) -> Result<WordlePlayerView, EngineError> {
        let my_progress = self
            .players
            .iter()
            .find(|p| p.player_id == player_id)
            .cloned()
            .ok_or(EngineError::UnknownPlayer(player_id))?;

        let opponents = self
            .players
            .iter()
            .filter(|p| p.player_id != player_id)
            .map(|p| OpponentSummary {
                player_id: p.player_id,
                display_name: p.display_name.clone(),
                guess_count: p.guesses.len() as u32,
                solved: p.solved,
                eliminated: p.eliminated,
            })
            .collect();

        Ok(WordlePlayerView {
            turn: self.turn,
            phase: self.phase.as_str().to_string(),
            my_progress,
            opponents,
            chat_messages: self.chat_messages.clone(),
            revealed_target_word: if matches!(
                self.phase,
                WordlePhase::Banter | WordlePhase::GameOver
            ) {
                Some(self.target_word.clone())
            } else {
                None
            },
            needs_guess_this_turn: self.needs_guess_this_turn(player_id)?,
            is_terminal: self.is_terminal(),
            max_guesses: self.config.max_guesses,
        })
    }

    pub fn legal_actions(&self, player_id: PlayerId) -> Vec<WordleAction> {
        if self.players.iter().all(|p| p.player_id != player_id) {
            return vec![];
        }

        match self.phase {
            WordlePhase::Lobby => {
                if self.sent_lobby_message.contains(&player_id) {
                    vec![]
                } else {
                    vec![WordleAction::SendMessage {
                        message: String::new(),
                    }]
                }
            }
            WordlePhase::Guessing => {
                let Some(player) = self.players.iter().find(|p| p.player_id == player_id) else {
                    return vec![];
                };
                if player.solved && !self.sent_win_message.contains(&player_id) {
                    vec![WordleAction::SendMessage {
                        message: String::new(),
                    }]
                } else if !player.solved
                    && !player.eliminated
                    && !self.guessed_this_turn.contains(&player_id)
                {
                    vec![WordleAction::Guess {
                        word: String::new(),
                    }]
                } else {
                    vec![]
                }
            }
            WordlePhase::Banter => {
                if self.sent_banter_message.contains(&player_id) {
                    vec![]
                } else {
                    vec![WordleAction::SendMessage {
                        message: String::new(),
                    }]
                }
            }
            WordlePhase::GameOver => vec![],
        }
    }

    pub fn apply_action(
        &mut self,
        player_id: PlayerId,
        action: &WordleAction,
    ) -> Result<(), EngineError> {
        let player_idx = self
            .players
            .iter()
            .position(|p| p.player_id == player_id)
            .ok_or(EngineError::UnknownPlayer(player_id))?;

        match action {
            WordleAction::SendMessage { message } => self.apply_send_message(player_idx, message),
            WordleAction::Guess { word } => self.apply_guess(player_idx, word),
        }
    }

    pub fn is_terminal(&self) -> bool {
        self.phase == WordlePhase::GameOver
    }

    fn apply_send_message(&mut self, player_idx: usize, message: &str) -> Result<(), EngineError> {
        if message.chars().count() > self.config.max_message_chars as usize {
            return Err(EngineError::MessageTooLong {
                len: message.chars().count(),
                max: self.config.max_message_chars as usize,
            });
        }

        let player_id = self.players[player_idx].player_id;
        let phase = match self.phase {
            WordlePhase::Lobby => {
                if self.sent_lobby_message.contains(&player_id) {
                    return Err(EngineError::LobbyMessageAlreadySent);
                }
                self.sent_lobby_message.insert(player_id);
                ChatPhase::Lobby
            }
            WordlePhase::Guessing => {
                if !self.players[player_idx].solved || self.sent_win_message.contains(&player_id) {
                    return Err(EngineError::WinMessageNotAllowed);
                }
                self.sent_win_message.insert(player_id);
                ChatPhase::Win
            }
            WordlePhase::Banter => {
                if self.sent_banter_message.contains(&player_id) {
                    return Err(EngineError::BanterMessageAlreadySent);
                }
                self.sent_banter_message.insert(player_id);
                ChatPhase::Banter
            }
            WordlePhase::GameOver => return Err(EngineError::InvalidPhase),
        };

        // Redact the target word from Win/Banter messages to prevent
        // leaking the answer to spectators and other agents still guessing.
        let text = match phase {
            ChatPhase::Win | ChatPhase::Banter => redact_word(message, &self.target_word),
            ChatPhase::Lobby => message.to_string(),
        };

        self.chat_messages.push(ChatMessage {
            player_id,
            player_name: self.players[player_idx].display_name.clone(),
            text,
            turn: self.turn,
            timestamp_ms: Utc::now().timestamp_millis(),
            phase,
        });

        if self.phase == WordlePhase::Lobby && self.sent_lobby_message.len() == self.players.len() {
            self.phase = WordlePhase::Guessing;
            self.turn = 1;
        }
        if self.phase == WordlePhase::Banter && self.sent_banter_message.len() == self.players.len()
        {
            self.phase = WordlePhase::GameOver;
        }

        Ok(())
    }

    fn apply_guess(&mut self, player_idx: usize, word: &str) -> Result<(), EngineError> {
        if self.phase != WordlePhase::Guessing {
            return Err(EngineError::InvalidPhase);
        }

        let player_id = self.players[player_idx].player_id;
        let player = &self.players[player_idx];
        if player.solved || player.eliminated {
            return Err(EngineError::GuessNotAllowed);
        }
        if self.guessed_this_turn.contains(&player_id) {
            return Err(EngineError::AlreadyGuessedThisTurn);
        }

        let normalized = word.trim().to_lowercase();
        if normalized.len() != 5 || !normalized.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(EngineError::InvalidGuessWord(normalized));
        }
        if !self.valid_words.contains(&normalized) {
            return Err(EngineError::InvalidGuessWord(normalized));
        }

        let feedback = feedback_for_guess(&self.target_word, &normalized);
        let is_correct = feedback.iter().all(|f| *f == LetterFeedback::Correct);
        self.players[player_idx].guesses.push(GuessResult {
            word: normalized,
            feedback,
            is_correct,
            turn: self.turn,
        });
        self.guessed_this_turn.insert(player_id);

        if is_correct && !self.players[player_idx].solved {
            self.players[player_idx].solved = true;
            self.players[player_idx].solved_turn = Some(self.turn);
            self.solve_order.push(player_id);
        }
        if !self.players[player_idx].solved
            && self.players[player_idx].guesses.len() as u32 >= self.config.max_guesses
        {
            self.players[player_idx].eliminated = true;
        }

        self.advance_turn_if_complete();

        Ok(())
    }

    fn advance_turn_if_complete(&mut self) {
        let active_guessers: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|p| !p.solved && !p.eliminated)
            .map(|p| p.player_id)
            .collect();
        let all_active_guessed = active_guessers
            .iter()
            .all(|player_id| self.guessed_this_turn.contains(player_id));
        if !all_active_guessed {
            return;
        }

        self.turn += 1;
        self.guessed_this_turn.clear();

        let all_solved_or_eliminated = self.players.iter().all(|p| p.solved || p.eliminated);
        let exhausted = self.turn > self.config.max_guesses;
        if all_solved_or_eliminated || exhausted {
            self.phase = WordlePhase::Banter;
            self.terminal_reason = Some(if exhausted {
                TerminalReason::MaxGuessesExhausted
            } else {
                TerminalReason::AllSolvedOrEliminated
            });
        }
    }

    fn needs_guess_this_turn(&self, player_id: PlayerId) -> Result<bool, EngineError> {
        let player = self
            .players
            .iter()
            .find(|p| p.player_id == player_id)
            .ok_or(EngineError::UnknownPlayer(player_id))?;
        Ok(self.phase == WordlePhase::Guessing
            && !player.solved
            && !player.eliminated
            && !self.guessed_this_turn.contains(&player_id))
    }
}

fn feedback_for_guess(target_word: &str, guessed_word: &str) -> Vec<LetterFeedback> {
    let target_chars: Vec<char> = target_word.chars().collect();
    let guess_chars: Vec<char> = guessed_word.chars().collect();
    let mut feedback = vec![LetterFeedback::Absent; 5];
    let mut remaining: HashMap<char, usize> = HashMap::new();

    for i in 0..5 {
        if guess_chars[i] == target_chars[i] {
            feedback[i] = LetterFeedback::Correct;
        } else {
            *remaining.entry(target_chars[i]).or_insert(0) += 1;
        }
    }

    for i in 0..5 {
        if feedback[i] == LetterFeedback::Correct {
            continue;
        }
        if let Some(count) = remaining.get_mut(&guess_chars[i]) {
            if *count > 0 {
                feedback[i] = LetterFeedback::Present;
                *count -= 1;
            }
        }
    }

    feedback
}

/// Case-insensitive replacement of `word` with asterisks in `message`.
fn redact_word(message: &str, word: &str) -> String {
    // Case-insensitive replacement that preserves multi-byte UTF-8 (emojis, etc.)
    // SAFETY ASSUMPTION: Wordle target words are strictly ASCII a-z, so
    // to_lowercase() is byte-length-preserving and byte indices stay aligned
    // between `message` and `msg_lower`.
    let word_lower = word.to_lowercase();
    let msg_lower = message.to_lowercase();
    let replacement = "*".repeat(word.chars().count());
    let mut result = String::with_capacity(message.len());
    let mut i = 0;

    while i < message.len() {
        if msg_lower[i..].starts_with(&word_lower) {
            result.push_str(&replacement);
            i += word_lower.len();
        } else {
            // Advance by one full UTF-8 character, not one byte
            let ch = &message[i..];
            let c = ch
                .chars()
                .next()
                .expect("loop invariant: i < message.len() so at least one char remains");
            result.push(c);
            i += c.len_utf8();
        }
    }
    result
}
