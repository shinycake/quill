//! One paced queue for every message an agent sends, so an agent cannot
//! flood Telegram on Quill's api_id.
//!
//! Each send takes a turn: at least [`SAME_CHAT_GAP`] after the last send to
//! the same chat and [`ANY_GAP`] after any send. When Telegram answers a
//! send with a flood wait, every agent send on that account is refused
//! until it passes (a flood wait belongs to the account Telegram gave it
//! to, and goes with it on log-out); nothing is retried for the agent. First messages to people who never
//! wrote to the user are capped at [`NEW_PEERS_PER_DAY`].

use crate::ids::AccountKey;
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
    /// Until when Telegram asked each account to slow down.
    flood_until: HashMap<AccountKey, Instant>,
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

/// A booked turn. The new-person slot and the chat's next free slot are
/// kept only once the send went out ([`Turn::commit`]); a turn dropped
/// without it (access went off, TDLib refused the send) gives them back.
#[must_use = "commit the turn once the send went out"]
pub struct Turn {
    booking: Option<Booking>,
}

struct Booking {
    key: (i32, i64),
    /// The chat's slot before this turn, and the one this turn set.
    previous: Option<Instant>,
    set: Instant,
    /// When this turn's first message to a new person was booked.
    new_peer: Option<Instant>,
}

impl Turn {
    /// The send went out: keep the booking.
    pub fn commit(mut self) {
        self.booking = None;
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        if let Some(booking) = self.booking.take() {
            let mut guard = crate::locks::lock_or_recover(&STATE);
            if let Some(state) = guard.as_mut() {
                state.release(booking);
            }
        }
    }
}

impl State {
    /// Book a turn at `now`: how long to wait for it, or why it is refused.
    fn reserve(
        &mut self,
        now: Instant,
        account: &AccountKey,
        key: (i32, i64),
        count: u32,
        new_peer: bool,
    ) -> Result<(Duration, Booking), Refusal> {
        if let Some(until) = self.flood_until.get(account).copied() {
            if until > now {
                return Err(Refusal::Flood(until.duration_since(now).as_secs().max(1)));
            }
            self.flood_until.remove(account);
        }
        while self
            .new_peers
            .front()
            .is_some_and(|sent| now.duration_since(*sent) > DAY)
        {
            self.new_peers.pop_front();
        }
        if new_peer && self.new_peers.len() >= NEW_PEERS_PER_DAY {
            return Err(Refusal::NewPeers);
        }
        let previous = self.next_in_chat.get(&key).copied();
        let start = [Some(now), previous, self.next_any]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(now);
        let wait = start.duration_since(now);
        if wait > MAX_QUEUE_WAIT {
            return Err(Refusal::Busy);
        }
        let spread = ANY_GAP.saturating_mul(count.max(1));
        self.next_any = Some(start + spread);
        let set = start + SAME_CHAT_GAP.max(spread);
        self.next_in_chat.insert(key, set);
        if new_peer {
            self.new_peers.push_back(start);
        }
        Ok((
            wait,
            Booking {
                key,
                previous,
                set,
                new_peer: new_peer.then_some(start),
            },
        ))
    }

    fn release(&mut self, booking: Booking) {
        if let Some(start) = booking.new_peer {
            if let Some(at) = self.new_peers.iter().position(|sent| *sent == start) {
                self.new_peers.remove(at);
            }
        }
        if self.next_in_chat.get(&booking.key) == Some(&booking.set) {
            match booking.previous {
                Some(previous) => {
                    self.next_in_chat.insert(booking.key, previous);
                }
                None => {
                    self.next_in_chat.remove(&booking.key);
                }
            }
        }
    }
}

/// Wait for a turn to send `count` messages to `chat_id` from `account`
/// (on TDLib client `client`), or refuse. `new_peer` marks a first message
/// to someone who never wrote to the user.
pub fn take_turn(
    account: &AccountKey,
    client: i32,
    chat_id: i64,
    count: u32,
    new_peer: bool,
) -> Result<Turn, Refusal> {
    let (wait, booking) = {
        let mut guard = crate::locks::lock_or_recover(&STATE);
        let state = guard.get_or_insert_with(State::default);
        state.reserve(Instant::now(), account, (client, chat_id), count, new_peer)?
    };
    let turn = Turn {
        booking: Some(booking),
    };
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
    Ok(turn)
}

/// Telegram answered a send from `account` with a flood wait of `seconds`.
pub fn flood(account: &AccountKey, seconds: u64) {
    let until = Instant::now() + Duration::from_secs(seconds.min(24 * 60 * 60));
    let mut guard = crate::locks::lock_or_recover(&STATE);
    let state = guard.get_or_insert_with(State::default);
    state
        .flood_until
        .entry(account.clone())
        .and_modify(|old| *old = (*old).max(until))
        .or_insert(until);
}

/// `account` logged out or was removed: its flood wait goes with it, so
/// whoever signs in next under the same key is not held to it.
pub fn forget_account(account: &AccountKey) {
    if let Some(state) = crate::locks::lock_or_recover(&STATE).as_mut() {
        state.flood_until.remove(account);
    }
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
    use super::{
        ANY_GAP, NEW_PEERS_PER_DAY, Refusal, SAME_CHAT_GAP, State, flood, flood_seconds,
        forget_account, take_turn,
    };
    use crate::ids::AccountKey;
    use std::time::{Duration, Instant};

    /// The queue's state is process-wide: the tests that use it must not
    /// run at once.
    static FLOOD_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn flood_errors_name_their_seconds() {
        assert_eq!(flood_seconds("Too Many Requests: retry after 12"), Some(12));
        assert_eq!(flood_seconds("FLOOD_WAIT_7"), Some(7));
        assert_eq!(flood_seconds("PEER_ID_INVALID"), None);
    }

    #[test]
    fn sends_to_one_chat_are_spaced() {
        let mut state = State::default();
        let account = AccountKey("pace".into());
        let now = Instant::now();
        let (first, _) = state.reserve(now, &account, (1, 1), 1, false).unwrap();
        assert_eq!(first, Duration::ZERO);
        let (second, _) = state.reserve(now, &account, (1, 1), 1, false).unwrap();
        assert_eq!(second, SAME_CHAT_GAP);
        let (other, _) = state.reserve(now, &account, (1, 2), 1, false).unwrap();
        assert!(other >= ANY_GAP, "any chat is spaced from the last send");
    }

    #[test]
    fn a_turn_that_never_sent_gives_back_its_slots() {
        let mut state = State::default();
        let account = AccountKey("refused".into());
        let now = Instant::now();
        for _ in 0..NEW_PEERS_PER_DAY * 3 {
            let (_, booking) = state.reserve(now, &account, (1, 7), 1, true).unwrap();
            state.release(booking);
        }
        assert!(state.new_peers.is_empty(), "no first message went out");
        assert!(!state.next_in_chat.contains_key(&(1, 7)));
        let (_, kept) = state.reserve(now, &account, (1, 7), 1, true).unwrap();
        assert_eq!(state.new_peers.len(), 1, "a send that went out is kept");
        assert!(state.next_in_chat.contains_key(&(1, 7)));
        drop(kept);
    }

    #[test]
    fn a_dropped_turn_is_released_and_a_committed_one_is_kept() {
        let _guard = crate::locks::lock_or_recover(&FLOOD_TEST);
        let account = AccountKey("turn-drop".into());
        let count = || {
            crate::locks::lock_or_recover(&super::STATE)
                .as_ref()
                .map_or(0, |s| s.new_peers.len())
        };
        let before = count();
        match take_turn(&account, 9010, 10, 1, true) {
            Ok(turn) => drop(turn),
            Err(_) => return,
        }
        assert_eq!(count(), before, "dropped without commit");
        if let Ok(turn) = take_turn(&account, 9011, 11, 1, true) {
            turn.commit();
            assert_eq!(count(), before + 1, "committed");
        }
    }

    #[test]
    fn a_flood_wait_refuses_that_accounts_sends_until_it_passes() {
        let _flood = crate::locks::lock_or_recover(&FLOOD_TEST);
        let a = AccountKey("flood-a".into());
        let b = AccountKey("flood-b".into());
        flood(&a, 30);
        assert!(matches!(take_turn(&a, 9002, 2, 1, false), Err(Refusal::Flood(s)) if s > 0));
        assert!(
            take_turn(&b, 9003, 3, 1, false).is_ok(),
            "another account's sends are not held to it"
        );
        forget_account(&a);
        assert!(
            take_turn(&a, 9004, 4, 1, false).is_ok(),
            "a log-out clears the account's flood wait"
        );
    }
}
