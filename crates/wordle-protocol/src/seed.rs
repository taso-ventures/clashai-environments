use chrono::NaiveDate;
use sha2::{Digest, Sha256};

/// Versioned suffix for deterministic Wordle daily seed key derivation.
const WORDLE_DAILY_SEED_KEY_VERSION: &str = "wordle:v1";

/// Build the deterministic daily seed key for Wordle from UTC date + slot index.
pub fn build_wordle_daily_seed_key(date_utc: NaiveDate, slot_index: u32) -> String {
    format!(
        "{}:slot:{}:{}",
        date_utc.format("%Y-%m-%d"),
        slot_index,
        WORDLE_DAILY_SEED_KEY_VERSION
    )
}

/// Derive the deterministic Wordle daily seed.
///
/// Algorithm:
/// 1. Build key: `"{date_utc}:slot:{slot_index}:wordle:v1"`
/// 2. SHA-256 over UTF-8 bytes
/// 3. First 8 bytes interpreted as big-endian u64
pub fn derive_wordle_daily_seed(date_utc: NaiveDate, slot_index: u32) -> u64 {
    let key = build_wordle_daily_seed_key(date_utc, slot_index);
    let digest = Sha256::digest(key.as_bytes());
    let mut seed_bytes = [0_u8; 8];
    seed_bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(seed_bytes)
}
