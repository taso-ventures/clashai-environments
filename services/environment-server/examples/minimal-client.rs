//! Reference agent loop — plays a full match from start to terminal state.
//!
//! Run:
//!   1. Start the server:          `cargo run --release --bin environment-server`
//!   2. In another terminal:       `cargo run --example minimal-client -- --game tic_tac_toe`
//!
//! The client picks legal actions uniformly at random. Swap in your own
//! policy / LLM inside `choose_action()` to build a real agent.

use std::env;
use std::time::Duration;

use serde_json::{json, Value};

const DEFAULT_SERVER: &str = "http://localhost:8080";
const POLL_INTERVAL: Duration = Duration::from_millis(200);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let game = args
        .iter()
        .position(|a| a == "--game")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "tic_tac_toe".to_string());
    let server = env::var("SERVER_URL").unwrap_or_else(|_| DEFAULT_SERVER.to_string());

    let client = reqwest::blocking::Client::new();

    // Create match.
    let create: Value = client
        .post(format!("{server}/matches"))
        .json(&json!({
            "environment_type": game,
            "player_count": 2,
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
        for player_id in 0..2 {
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

            let action = choose_action(&legal);
            let resp: Value = client
                .post(format!("{server}/matches/{match_id}/actions"))
                .json(&json!({ "player_id": player_id, "action": action }))
                .send()?
                .json()?;

            println!("p{player_id} -> {action}  => {resp}");
            made_move = true;
            break;
        }

        if !made_move {
            std::thread::sleep(POLL_INTERVAL);
        }
    }
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
