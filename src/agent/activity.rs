//! What agents are doing, for the user: who is connected now and which
//! tools each one used, the agents the user disconnected, and a log of
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
    /// Stable within the log, so the user can act on one entry (Undo).
    /// Entries from before ids existed read 0.
    #[serde(default)]
    pub id: u64,
    /// Unix seconds.
    pub time: i64,
    pub account: AccountKey,
    /// The MCP client's name ("Claude Code").
    pub agent: String,
    /// The tool's name (`send_message`).
    pub tool: String,
    pub chat_id: i64,
    pub chat_title: String,
    /// What the action carried, in the agent's own words or ids
    /// (`"See you at 6"`); the verb comes from `tool`, worded by the UI.
    pub summary: String,
    /// False when Telegram or the user refused it.
    pub ok: bool,
    /// The user declined it in Quill.
    #[serde(default)]
    pub declined: bool,
    /// The messages it made or acted on (a send's final ids, the message
    /// a reaction went on), so the log can open them.
    #[serde(default)]
    pub message_ids: Vec<i64>,
    /// The user undid it from the log.
    #[serde(default)]
    pub undone: bool,
}

/// An agent connected now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connected {
    pub id: u64,
    pub agent: String,
    /// Unix seconds.
    pub since: i64,
    /// The tools it called on this connection, in the order first used.
    pub tools: Vec<String>,
}

static LOG: Mutex<Option<VecDeque<Action>>> = Mutex::new(None);
static CONNECTED: Mutex<Vec<Connected>> = Mutex::new(Vec::new());
/// Connections the user ended with Revoke that have not closed yet.
static REVOKED: Mutex<Vec<u64>> = Mutex::new(Vec::new());
/// Agents the user disconnected: refused at `auth` until the user allows
/// them again, turns Agent access off, or restarts Quill.
static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());
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

/// The newest log snapshot written, so a slow writer never puts back an
/// older one.
static WRITTEN: Mutex<u64> = Mutex::new(0);
static NEXT_SNAPSHOT: AtomicU64 = AtomicU64::new(0);

/// Write a snapshot of the log taken under its lock; runs without the lock,
/// so the Settings page never waits on the disk. Owner-only: the log holds
/// chat titles and short quotes.
fn persist(snapshot: u64, bytes: &[u8]) {
    if !crate::fast_buttons::persistence_enabled() {
        return;
    }
    let mut written = crate::locks::lock_or_recover(&WRITTEN);
    if snapshot <= *written {
        return;
    }
    *written = snapshot;
    if let Some(path) = log_path()
        && let Err(err) = super::access::write_owner_only(&path, bytes)
    {
        eprintln!("quill: could not save {LOG_FILE}: {err}");
    }
}

/// Change the log under its lock, then save it and wake the UI.
fn change_log(f: impl FnOnce(&mut VecDeque<Action>)) {
    let (snapshot, bytes) = with_log(|log| {
        f(log);
        let snapshot = NEXT_SNAPSHOT.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        (snapshot, serde_json::to_vec(log).unwrap_or_default())
    });
    persist(snapshot, &bytes);
    crate::wake::AGENT.poke();
}

/// Add `action` to the log, stamped now.
pub fn record(mut action: Action) {
    action.time = now();
    record_at(action);
}

/// Add `action` to the log with the time it already carries. It gets the
/// next id.
pub fn record_at(mut action: Action) {
    change_log(|log| {
        action.id = log
            .iter()
            .map(|a| a.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        log.push_back(action);
        while log.len() > KEEP {
            log.pop_front();
        }
    });
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

/// Mark entry `id` of `account` as undone by the user.
pub fn mark_undone(account: &AccountKey, id: u64) {
    if id == 0 {
        return;
    }
    change_log(|log| {
        for action in log.iter_mut() {
            if action.id == id && action.account == *account {
                action.undone = true;
            }
        }
    });
}

/// Empty the log for every account.
pub fn clear() {
    change_log(VecDeque::clear);
}

/// Marks an agent as connected until dropped.
pub struct Presence(u64);

impl Presence {
    /// The connection's id, for [`note_tool`] and [`is_revoked`].
    pub fn id(&self) -> u64 {
        self.0
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        crate::locks::lock_or_recover(&CONNECTED).retain(|c| c.id != self.0);
        crate::locks::lock_or_recover(&REVOKED).retain(|id| *id != self.0);
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
        tools: Vec::new(),
    });
    crate::wake::AGENT.poke();
    Presence(id)
}

/// The agents connected now, oldest first.
pub fn connected() -> Vec<Connected> {
    crate::locks::lock_or_recover(&CONNECTED).clone()
}

/// Connection `id` called `tool`.
pub fn note_tool(id: u64, tool: &str) {
    let mut connected = crate::locks::lock_or_recover(&CONNECTED);
    let Some(connection) = connected.iter_mut().find(|c| c.id == id) else {
        return;
    };
    if connection.tools.iter().any(|known| known == tool) {
        return;
    }
    connection.tools.push(tool.to_string());
    drop(connected);
    crate::wake::AGENT.poke();
}

/// Revoke: end connection `id` now and refuse its agent's next ones until
/// the user allows it again. Refused while the UI driver injects input.
pub fn revoke(id: u64) {
    if super::ui_driver::guard::refuse_if_injecting("disconnect an agent") {
        return;
    }
    let agent = connected()
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.agent);
    let Some(agent) = agent else {
        return;
    };
    crate::locks::lock_or_recover(&REVOKED).push(id);
    let mut refused = crate::locks::lock_or_recover(&REFUSED);
    if !refused.contains(&agent) {
        refused.push(agent);
    }
    drop(refused);
    crate::wake::AGENT.poke();
}

/// Whether the user revoked connection `id`.
pub fn is_revoked(id: u64) -> bool {
    crate::locks::lock_or_recover(&REVOKED).contains(&id)
}

/// Whether `agent` was disconnected by the user and may not connect.
pub fn refused(agent: &str) -> bool {
    crate::locks::lock_or_recover(&REFUSED)
        .iter()
        .any(|name| name == agent)
}

/// The agents the user disconnected, in the order they were.
pub fn refused_agents() -> Vec<String> {
    crate::locks::lock_or_recover(&REFUSED).clone()
}

/// "Allow again" for one disconnected agent.
pub fn allow_again(agent: &str) {
    if super::ui_driver::guard::refuse_if_injecting("allow an agent again") {
        return;
    }
    crate::locks::lock_or_recover(&REFUSED).retain(|name| name != agent);
    crate::wake::AGENT.poke();
}

/// Agent access went off: every agent may connect when it comes back.
pub(crate) fn allow_all_again() {
    crate::locks::lock_or_recover(&REFUSED).clear();
}

#[cfg(test)]
mod tests {
    use super::{
        Action, allow_again, connect, connected, is_revoked, mark_undone, note_tool, recent,
        record, refused, revoke,
    };
    use crate::ids::AccountKey;

    fn action(account: &AccountKey, summary: &str) -> Action {
        Action {
            id: 0,
            time: 0,
            account: account.clone(),
            agent: "Test".into(),
            tool: "send_message".into(),
            chat_id: 1,
            chat_title: "Saved Messages".into(),
            summary: summary.into(),
            ok: true,
            declined: false,
            message_ids: vec![7],
            undone: false,
        }
    }

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
            record(action(&account, summary));
        }
        let log = recent(&account);
        assert_eq!(
            log.iter().map(|a| a.summary.as_str()).collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert!(log.iter().all(|a| a.time > 0));
        let ids: Vec<u64> = log.iter().map(|a| a.id).collect();
        assert!(ids.iter().all(|id| *id > 0));
        assert_ne!(ids.first(), ids.last(), "each entry has its own id");
    }

    #[test]
    fn undo_marks_one_entry_of_one_account() {
        crate::fast_buttons::set_persistence(false);
        let account = AccountKey("activity-undo-test".into());
        let other = AccountKey("activity-undo-other".into());
        record(action(&account, "mine"));
        record(action(&other, "theirs"));
        let id = recent(&account).first().map_or(0, |a| a.id);
        mark_undone(&other, id);
        assert!(!recent(&account).iter().any(|a| a.undone), "wrong account");
        mark_undone(&account, id);
        assert!(recent(&account).first().is_some_and(|a| a.undone));
        assert!(!recent(&other).iter().any(|a| a.undone));
    }

    #[test]
    fn an_old_log_entry_reads_without_the_new_fields() {
        let old = r#"{"time":5,"account":"a","agent":"x","tool":"send_message","chat_id":1,"chat_title":"t","summary":"s","ok":true}"#;
        let action: Action = serde_json::from_str(old).unwrap_or_else(|_| panic!("parses"));
        assert_eq!(action.id, 0);
        assert!(action.message_ids.is_empty());
        assert!(!action.undone && !action.declined);
    }

    #[test]
    fn tools_used_are_listed_once_in_first_use_order() {
        let guard = connect("Tools test");
        note_tool(guard.id(), "list_chats");
        note_tool(guard.id(), "send_message");
        note_tool(guard.id(), "list_chats");
        let tools = connected()
            .into_iter()
            .find(|c| c.id == guard.id())
            .map(|c| c.tools)
            .unwrap_or_default();
        assert_eq!(tools, ["list_chats", "send_message"]);
    }

    #[test]
    fn revoke_ends_the_connection_and_refuses_the_agent_until_allowed() {
        let guard = connect("Revoke test");
        let bystander = connect("Bystander test");
        assert!(!refused("Revoke test"));
        revoke(guard.id());
        assert!(is_revoked(guard.id()));
        assert!(!is_revoked(bystander.id()), "only that connection ends");
        assert!(refused("Revoke test"));
        assert!(!refused("Bystander test"));
        let id = guard.id();
        drop(guard);
        assert!(!is_revoked(id), "a closed connection is forgotten");
        assert!(refused("Revoke test"), "but the agent stays refused");
        allow_again("Revoke test");
        assert!(!refused("Revoke test"));
        revoke(9_999_999);
        assert!(!refused(""), "an unknown connection refuses nobody");
    }
}
