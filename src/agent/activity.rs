//! What agents are doing, for the user: who is connected now, and a log of
//! every write an agent made (time, agent, tool, chat).
//!
//! The log keeps the newest [`KEEP`] entries in `agent_actions.json` in the
//! app data folder, across accounts. Reads are not logged.

use crate::ids::AccountKey;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const LOG_FILE: &str = "agent_actions.json";
/// Log entries kept.
const KEEP: usize = 500;

/// One write an agent made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    /// Unix seconds.
    pub time: i64,
    pub account: AccountKey,
    /// The MCP client's name ("Claude Code").
    pub agent: String,
    /// The tool's name (`send_message`).
    pub tool: String,
    pub chat_id: i64,
    pub chat_title: String,
    /// One line on what happened ("Sent \"See you at 6\"").
    pub summary: String,
    /// False when Telegram or the user refused it.
    pub ok: bool,
}

/// An agent connected now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connected {
    pub id: u64,
    pub agent: String,
    /// Unix seconds.
    pub since: i64,
}

static LOG: Mutex<Option<VecDeque<Action>>> = Mutex::new(None);
static CONNECTED: Mutex<Vec<Connected>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn log_path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join(LOG_FILE))
}

fn with_log<R>(f: impl FnOnce(&mut VecDeque<Action>) -> R) -> R {
    let mut guard = crate::locks::lock_or_recover(&LOG);
    let log = guard.get_or_insert_with(|| {
        log_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    });
    f(log)
}

fn persist(log: &VecDeque<Action>) {
    if !crate::fast_buttons::persistence_enabled() {
        return;
    }
    if let Some(path) = log_path()
        && let Err(err) = crate::settings::write_json_atomic(&path, log)
    {
        eprintln!("quill: could not save {LOG_FILE}: {err}");
    }
}

/// Add `action` to the log, stamped now.
pub fn record(mut action: Action) {
    action.time = now();
    with_log(|log| {
        log.push_back(action);
        while log.len() > KEEP {
            log.pop_front();
        }
        persist(log);
    });
    crate::wake::AGENT.poke();
}

/// The log for `account`, newest first.
pub fn recent(account: &AccountKey) -> Vec<Action> {
    with_log(|log| {
        log.iter()
            .rev()
            .filter(|action| action.account == *account)
            .cloned()
            .collect()
    })
}

/// Empty the log for every account.
pub fn clear() {
    with_log(|log| {
        log.clear();
        persist(log);
    });
    crate::wake::AGENT.poke();
}

/// Marks an agent as connected until dropped.
pub struct Presence(u64);

impl Drop for Presence {
    fn drop(&mut self) {
        crate::locks::lock_or_recover(&CONNECTED).retain(|c| c.id != self.0);
        crate::wake::AGENT.poke();
    }
}

/// Mark `agent` as connected; it stays listed while the guard lives.
pub fn connect(agent: &str) -> Presence {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    crate::locks::lock_or_recover(&CONNECTED).push(Connected {
        id,
        agent: agent.to_string(),
        since: now(),
    });
    crate::wake::AGENT.poke();
    Presence(id)
}

/// The agents connected now, oldest first.
pub fn connected() -> Vec<Connected> {
    crate::locks::lock_or_recover(&CONNECTED).clone()
}

#[cfg(test)]
mod tests {
    use super::{Action, connect, connected, recent, record};
    use crate::ids::AccountKey;

    #[test]
    fn presence_lasts_as_long_as_the_guard() {
        let guard = connect("Presence test");
        assert!(connected().iter().any(|c| c.agent == "Presence test"));
        drop(guard);
        assert!(!connected().iter().any(|c| c.agent == "Presence test"));
    }

    #[test]
    fn the_log_is_per_account_and_newest_first() {
        crate::fast_buttons::set_persistence(false);
        let account = AccountKey("activity-test".into());
        for summary in ["first", "second"] {
            record(Action {
                time: 0,
                account: account.clone(),
                agent: "Test".into(),
                tool: "send_message".into(),
                chat_id: 1,
                chat_title: "Saved Messages".into(),
                summary: summary.into(),
                ok: true,
            });
        }
        let log = recent(&account);
        assert_eq!(
            log.iter().map(|a| a.summary.as_str()).collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert!(log.iter().all(|a| a.time > 0));
    }
}
