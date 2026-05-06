//! Reference agent loop — plays a full match from start to terminal state.
//!
//! Run:
//!   1. Start the server:          `cargo run --release --bin environment-server`
//!   2. In another terminal:       `cargo run --example minimal-client -- --game tic_tac_toe`
//!
//! The client picks legal actions uniformly at random. Swap in your own
//! policy / LLM inside `choose_action()` to build a real agent.
//!
//! Flags / env vars:
//!   `--game <env_type>`     environment to play (default: `tic_tac_toe`)
//!   `--players <N>`         number of players (default: 2; use 4 for vibe_check, 3+ for wordle)
//!   `--delay-ms <N>`        sleep between actions (default: 0). Use ~500 to watch a viewer in the browser.
//!   `SERVER_URL=<url>`      override the default server (default: `http://localhost:8080`)

use std::env;
use std::time::Duration;

use serde_json::{json, Value};

const DEFAULT_SERVER: &str = "http://localhost:8080";
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Stub value for empty string fields in placeholder actions (e.g. red_button
/// `Speak.message`, wordle `Guess.word`). Picked because it's a non-empty
/// short string AND a valid 5-letter wordle dictionary entry.
const PLACEHOLDER_STRING: &str = "hello";
/// Bail out after this many consecutive rejected actions to surface
/// pathological loops (e.g. legal_actions returning an action the engine then
/// rejects) instead of busy-looping forever.
const MAX_CONSECUTIVE_REJECTIONS: u32 = 5;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let game = args
        .iter()
        .position(|a| a == "--game")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "tic_tac_toe".to_string());
    let players: usize = args
        .iter()
        .position(|a| a == "--players")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let delay = args
        .iter()
        .position(|a| a == "--delay-ms")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::ZERO);
    let server = env::var("SERVER_URL").unwrap_or_else(|_| DEFAULT_SERVER.to_string());

    let client = reqwest::blocking::Client::new();

    // Create match.
    let create: Value = client
        .post(format!("{server}/matches"))
        .json(&json!({
            "environment_type": game,
            "player_count": players,
            "seed": 1
        }))
        .send()?
        .json()?;

    let match_id = create["match_id"].as_str().expect("match_id").to_string();
    println!("Created {game} match {match_id}");
    if let Some(url) = create["spectator_url"].as_str() {
        println!("Spectate: {url}");
    }

    // Drive the match to terminal.
    let mut consecutive_rejections: u32 = 0;
    loop {
        let status: Value = client
            .get(format!("{server}/matches/{match_id}/status"))
            .send()?
            .json()?;
        if status["is_terminal"].as_bool().unwrap_or(false) {
            println!("Match terminal. Final status: {status}");
            return Ok(());
        }

        // Try each player in turn. Whoever has legal actions right now plays.
        let mut made_move = false;
        for player_id in 0..players {
            let actions: Value = client
                .get(format!(
                    "{server}/matches/{match_id}/legal_actions?player_id={player_id}"
                ))
                .send()?
                .json()?;

            // Server returns a bare JSON array of action objects.
            let legal = actions.as_array().cloned().unwrap_or_default();
            if legal.is_empty() {
                continue;
            }

            let action = fill_placeholders(choose_action(&legal));
            let resp: Value = client
                .post(format!("{server}/matches/{match_id}/actions"))
                .json(&json!({ "player_id": player_id, "action": action }))
                .send()?
                .json()?;

            println!("p{player_id} -> {action}  => {resp}");
            made_move = true;

            if !delay.is_zero() {
                std::thread::sleep(delay);
            }

            if resp["accepted"].as_bool() == Some(false) {
                consecutive_rejections += 1;
                if consecutive_rejections >= MAX_CONSECUTIVE_REJECTIONS {
                    return Err(format!(
                        "{MAX_CONSECUTIVE_REJECTIONS} consecutive actions rejected; \
                         giving up to avoid busy-looping. Last response: {resp}"
                    )
                    .into());
                }
            } else {
                consecutive_rejections = 0;
            }
            break;
        }

        if !made_move {
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

/// Replace any empty-string field in the action's top-level JSON object with
/// a stub value. Several environments return placeholder actions from
/// `legal_actions` whose free-form fields (`message`, `word`, etc.) are
/// expected to be filled in by the agent — without this, a random-policy
/// client submits empty strings and the engine rejects every action.
fn fill_placeholders(mut action: Value) -> Value {
    if let Some(obj) = action.as_object_mut() {
        for (_, v) in obj.iter_mut() {
            if v.as_str() == Some("") {
                *v = Value::String(PLACEHOLDER_STRING.to_string());
            }
        }
    }
    action
}

/// Replace this with your own policy. Random over the legal set is enough
/// to exercise the wire protocol end-to-end.
fn choose_action(legal: &[Value]) -> Value {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    legal[nanos % legal.len()].clone()
}
