//! One paced queue for every message an agent sends, so an agent cannot
//! flood Telegram on Quill's api_id.
//!
//! Each send takes a turn: at least [`SAME_CHAT_GAP`] after the last send to
//! the same chat and [`ANY_GAP`] after any send. When Telegram answers a
//! send with a flood wait, every agent send is refused until it passes;
//! nothing is retried for the agent. First messages to people who never
//! wrote to the user are capped at [`NEW_PEERS_PER_DAY`].

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SAME_CHAT_GAP: Duration = Duration::from_millis(1500);
const ANY_GAP: Duration = Duration::from_millis(400);
const NEW_PEERS_PER_DAY: usize = 5;
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
/// A turn further away than this is refused rather than waited for.
const MAX_QUEUE_WAIT: Duration = Duration::from_secs(30);

#[derive(Default)]
struct State {
    /// The next free slot per (client, chat), and for any chat.
    next_in_chat: HashMap<(i32, i64), Instant>,
    next_any: Option<Instant>,
    flood_until: Option<Instant>,
    /// When each first message to a new person went out.
    new_peers: VecDeque<Instant>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// Why a send may not go out now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Telegram asked to slow down; this many seconds are left.
    Flood(u64),
    /// Too many first messages to new people today.
    NewPeers,
    /// Too many sends are queued.
    Busy,
}

/// Wait for a turn to send `count` messages to `chat_id`, or refuse.
/// `new_peer` marks a first message to someone who never wrote to the user.
pub fn take_turn(client: i32, chat_id: i64, count: u32, new_peer: bool) -> Result<(), Refusal> {
    let wait = {
        let mut guard = crate::locks::lock_or_recover(&STATE);
        let state = guard.get_or_insert_with(State::default);
        let now = Instant::now();
        if let Some(until) = state.flood_until {
            if until > now {
                return Err(Refusal::Flood(until.duration_since(now).as_secs().max(1)));
            }
            state.flood_until = None;
        }
        while state
            .new_peers
            .front()
            .is_some_and(|sent| now.duration_since(*sent) > DAY)
        {
            state.new_peers.pop_front();
        }
        if new_peer && state.new_peers.len() >= NEW_PEERS_PER_DAY {
            return Err(Refusal::NewPeers);
        }
        let in_chat = state.next_in_chat.get(&(client, chat_id)).copied();
        let start = [Some(now), in_chat, state.next_any]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(now);
        let wait = start.duration_since(now);
        if wait > MAX_QUEUE_WAIT {
            return Err(Refusal::Busy);
        }
        let spread = ANY_GAP.saturating_mul(count.max(1));
        state.next_any = Some(start + spread);
        state
            .next_in_chat
            .insert((client, chat_id), start + SAME_CHAT_GAP.max(spread));
        if new_peer {
            state.new_peers.push_back(start);
        }
        wait
    };
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
    Ok(())
}

/// Telegram answered a send with a flood wait of `seconds`.
pub fn flood(seconds: u64) {
    let until = Instant::now() + Duration::from_secs(seconds.min(24 * 60 * 60));
    let mut guard = crate::locks::lock_or_recover(&STATE);
    let state = guard.get_or_insert_with(State::default);
    state.flood_until = Some(state.flood_until.map_or(until, |old| old.max(until)));
}

/// The seconds in a TDLib flood error ("Too Many Requests: retry after 12"
/// or "FLOOD_WAIT_12").
pub fn flood_seconds(message: &str) -> Option<u64> {
    let digits = |rest: &str| -> Option<u64> {
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        rest.get(..end)?.parse().ok()
    };
    if let Some(at) = message.find("retry after ") {
        return message.get(at + "retry after ".len()..).and_then(digits);
    }
    message
        .find("FLOOD_WAIT_")
        .and_then(|at| message.get(at + "FLOOD_WAIT_".len()..))
        .and_then(digits)
}

#[cfg(test)]
mod tests {
    use super::{Refusal, flood, flood_seconds, take_turn};
    use std::time::Instant;

    #[test]
    fn flood_errors_name_their_seconds() {
        assert_eq!(flood_seconds("Too Many Requests: retry after 12"), Some(12));
        assert_eq!(flood_seconds("FLOOD_WAIT_7"), Some(7));
        assert_eq!(flood_seconds("PEER_ID_INVALID"), None);
    }

    #[test]
    fn sends_to_one_chat_are_spaced() {
        let started = Instant::now();
        assert_eq!(take_turn(9001, 1, 1, false), Ok(()));
        assert_eq!(take_turn(9001, 1, 1, false), Ok(()));
        assert!(started.elapsed() >= super::SAME_CHAT_GAP);
    }

    #[test]
    fn a_flood_wait_refuses_sends_until_it_passes() {
        flood(30);
        assert!(matches!(take_turn(9002, 2, 1, false), Err(Refusal::Flood(s)) if s > 0));
        crate::locks::lock_or_recover(&super::STATE)
            .get_or_insert_with(Default::default)
            .flood_until = None;
    }
}
