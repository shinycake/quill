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

/// How one logged write ended. One value, so a refused write cannot also
/// read as done and an Undo only lands on a write that went through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It went through.
    Done,
    /// Telegram refused it.
    Failed,
    /// The user declined it in Quill.
    Declined,
    /// It went through and the user undid it from the log.
    Undone,
}

impl Outcome {
    /// The outcome of a write from what the writer knows: Telegram's answer
    /// and whether the user declined.
    pub fn of(ok: bool, declined: bool) -> Self {
        if declined {
            Self::Declined
        } else if ok {
            Self::Done
        } else {
            Self::Failed
        }
    }
}

/// The log's on-disk shape of [`Action`]: the outcome as the three flags
/// earlier versions wrote.
#[derive(Serialize, Deserialize)]
struct ActionRecord {
    #[serde(default)]
    id: u64,
    time: i64,
    account: AccountKey,
    agent: String,
    tool: String,
    chat_id: i64,
    chat_title: String,
    summary: String,
    ok: bool,
    #[serde(default)]
    declined: bool,
    #[serde(default)]
    message_ids: Vec<i64>,
    #[serde(default)]
    undone: bool,
}

impl From<ActionRecord> for Action {
    fn from(r: ActionRecord) -> Self {
        let outcome = if r.declined {
            Outcome::Declined
        } else if !r.ok {
            Outcome::Failed
        } else if r.undone {
            Outcome::Undone
        } else {
            Outcome::Done
        };
        Action {
            id: r.id,
            time: r.time,
            account: r.account,
            agent: r.agent,
            tool: r.tool,
            chat_id: r.chat_id,
            chat_title: r.chat_title,
            summary: r.summary,
            outcome,
            message_ids: r.message_ids,
        }
    }
}

impl From<Action> for ActionRecord {
    fn from(a: Action) -> Self {
        ActionRecord {
            id: a.id,
            time: a.time,
            account: a.account,
            agent: a.agent,
            tool: a.tool,
            chat_id: a.chat_id,
            chat_title: a.chat_title,
            summary: a.summary,
            ok: matches!(a.outcome, Outcome::Done | Outcome::Undone),
            declined: a.outcome == Outcome::Declined,
            message_ids: a.message_ids,
            undone: a.outcome == Outcome::Undone,
        }
    }
}

/// One write an agent made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ActionRecord", into = "ActionRecord")]
pub struct Action {
    /// Stable within the log, so the user can act on one entry (Undo).
    /// Entries from before ids existed read 0.
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
    /// How it ended.
    pub outcome: Outcome,
    /// The messages it made or acted on (a send's final ids, the message
    /// a reaction went on), so the log can open them.
    pub message_ids: Vec<i64>,
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

/// The log as saved: the entries and the last id handed out, kept apart so
/// an id is never handed out twice, not even after Clear.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    last_id: u64,
    #[serde(default)]
    actions: VecDeque<Action>,
}

/// The file as earlier versions wrote it, too: a bare list of entries.
#[derive(Deserialize)]
#[serde(untagged)]
enum OnDisk {
    Now(Stored),
    Before(VecDeque<Action>),
}

impl Stored {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let mut stored = match serde_json::from_slice(bytes).ok()? {
            OnDisk::Now(stored) => stored,
            OnDisk::Before(actions) => Stored {
                last_id: 0,
                actions,
            },
        };
        let newest = stored.actions.iter().map(|a| a.id).max().unwrap_or(0);
        stored.last_id = stored.last_id.max(newest);
        Some(stored)
    }

    /// Adds `action` with the next id, dropping the oldest past [`KEEP`].
    fn push(&mut self, mut action: Action) {
        self.last_id = self.last_id.saturating_add(1);
        action.id = self.last_id;
        self.actions.push_back(action);
        while self.actions.len() > KEEP {
            self.actions.pop_front();
        }
    }
}

static LOG: Mutex<Option<Stored>> = Mutex::new(None);

/// Where one connection is: open, or ended by the user's Revoke and not
/// closed yet.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Live,
    Revoked,
}

struct Connection {
    shown: Connected,
    stage: Stage,
    /// The client's process, as the socket reports it. Unlike the name the
    /// client declares, it cannot be changed to get back in after Revoke.
    process: Option<i64>,
}

/// An agent the user disconnected.
struct Refused {
    agent: String,
    /// The processes that were connected under the name when it was revoked.
    processes: Vec<i64>,
}

/// Who is connected and who is refused, under one lock so a connection and
/// its agent's refusal change together.
struct Roster {
    connections: Vec<Connection>,
    /// Agents the user disconnected: refused at `auth` until the user
    /// allows them again, turns Agent access off, or restarts Quill.
    refused: Vec<Refused>,
}

static ROSTER: Mutex<Roster> = Mutex::new(Roster {
    connections: Vec::new(),
    refused: Vec::new(),
});

fn roster() -> std::sync::MutexGuard<'static, Roster> {
    crate::locks::lock_or_recover(&ROSTER)
}
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn log_path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join(LOG_FILE))
}

fn with_store<R>(f: impl FnOnce(&mut Stored) -> R) -> R {
    let mut guard = crate::locks::lock_or_recover(&LOG);
    let store = guard.get_or_insert_with(|| {
        log_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| Stored::parse(&bytes))
            .unwrap_or_default()
    });
    f(store)
}

fn with_log<R>(f: impl FnOnce(&mut VecDeque<Action>) -> R) -> R {
    with_store(|store| f(&mut store.actions))
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
fn change_log(f: impl FnOnce(&mut Stored)) {
    let (snapshot, bytes) = with_store(|store| {
        f(store);
        let snapshot = NEXT_SNAPSHOT.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        (snapshot, serde_json::to_vec(store).unwrap_or_default())
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
/// next id, one no entry has had before.
pub fn record_at(action: Action) {
    change_log(|store| store.push(action));
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
    change_log(|store| {
        for action in store.actions.iter_mut() {
            if action.id == id && action.account == *account && action.outcome == Outcome::Done {
                action.outcome = Outcome::Undone;
            }
        }
    });
}

/// `account` logged out or was removed: its entries go (chat titles, the
/// text sent, Undo targets), so the next sign-in under the key sees none.
pub fn forget_account(account: &AccountKey) {
    change_log(|store| store.actions.retain(|action| action.account != *account));
}

/// Empty the log for every account. Ids go on from where they were, so an
/// Undo the page still holds can't land on a new entry.
pub fn clear() {
    change_log(|store| store.actions.clear());
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
        roster().connections.retain(|c| c.shown.id != self.0);
        super::prompts::withdraw_connection(self.0);
        crate::wake::AGENT.poke();
    }
}

/// Mark `agent` as connected; it stays listed while the guard lives.
pub fn connect(agent: &str) -> Presence {
    connect_from(agent, None)
}

/// [`connect`] for a client whose socket named its `process`.
pub fn connect_from(agent: &str, process: Option<i64>) -> Presence {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    roster().connections.push(Connection {
        shown: Connected {
            id,
            agent: agent.to_string(),
            since: now(),
            tools: Vec::new(),
        },
        stage: Stage::Live,
        process,
    });
    crate::wake::AGENT.poke();
    Presence(id)
}

/// The agents connected now, oldest first.
pub fn connected() -> Vec<Connected> {
    roster()
        .connections
        .iter()
        .map(|c| c.shown.clone())
        .collect()
}

/// Whether connection `id` is still connected.
pub fn is_connected(id: u64) -> bool {
    roster().connections.iter().any(|c| c.shown.id == id)
}

/// Connection `id` called `tool`.
pub fn note_tool(id: u64, tool: &str) {
    let mut roster = roster();
    let Some(connection) = roster.connections.iter_mut().find(|c| c.shown.id == id) else {
        return;
    };
    if connection.shown.tools.iter().any(|known| known == tool) {
        return;
    }
    connection.shown.tools.push(tool.to_string());
    drop(roster);
    crate::wake::AGENT.poke();
}

/// Revoke: end connection `id` now and refuse its agent's next ones until
/// the user allows it again. Refused while the UI driver injects input.
pub fn revoke(id: u64) {
    if super::ui_driver::guard::refuse_if_injecting("disconnect an agent") {
        return;
    }
    let mut roster = roster();
    let Some(connection) = roster.connections.iter_mut().find(|c| c.shown.id == id) else {
        return;
    };
    connection.stage = Stage::Revoked;
    let agent = connection.shown.agent.clone();
    let process = connection.process;
    let at = match roster.refused.iter().position(|r| r.agent == agent) {
        Some(at) => at,
        None => {
            roster.refused.push(Refused {
                agent,
                processes: Vec::new(),
            });
            roster.refused.len() - 1
        }
    };
    if let Some(process) = process
        && let Some(entry) = roster.refused.get_mut(at)
        && !entry.processes.contains(&process)
    {
        entry.processes.push(process);
    }
    drop(roster);
    crate::wake::AGENT.poke();
}

/// Whether the user revoked connection `id`.
pub fn is_revoked(id: u64) -> bool {
    roster()
        .connections
        .iter()
        .any(|c| c.shown.id == id && c.stage == Stage::Revoked)
}

/// Whether `agent` was disconnected by the user and may not connect.
pub fn refused(agent: &str) -> bool {
    refused_from(agent, None)
}

/// [`refused`] for a client that also names its `process`: a revoked
/// agent's process stays refused under any other name it declares.
pub fn refused_from(agent: &str, process: Option<i64>) -> bool {
    roster()
        .refused
        .iter()
        .any(|r| r.agent == agent || process.is_some_and(|process| r.processes.contains(&process)))
}

/// The agents the user disconnected, in the order they were.
pub fn refused_agents() -> Vec<String> {
    roster().refused.iter().map(|r| r.agent.clone()).collect()
}

/// "Allow again" for one disconnected agent.
pub fn allow_again(agent: &str) {
    if super::ui_driver::guard::refuse_if_injecting("allow an agent again") {
        return;
    }
    roster().refused.retain(|r| r.agent != agent);
    crate::wake::AGENT.poke();
}

/// Agent access went off: every agent may connect when it comes back.
pub(crate) fn allow_all_again() {
    roster().refused.clear();
}

#[cfg(test)]
mod tests {
    use super::{
        Action, Outcome, Stored, allow_again, connect, connected, is_revoked, mark_undone,
        note_tool, recent, record, refused, revoke,
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
            outcome: Outcome::Done,
            message_ids: vec![7],
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
        assert!(
            !recent(&account)
                .iter()
                .any(|a| a.outcome == Outcome::Undone),
            "wrong account"
        );
        mark_undone(&account, id);
        assert!(
            recent(&account)
                .first()
                .is_some_and(|a| a.outcome == Outcome::Undone)
        );
        assert!(!recent(&other).iter().any(|a| a.outcome == Outcome::Undone));
    }

    #[test]
    fn an_old_log_entry_reads_without_the_new_fields() {
        let old = r#"{"time":5,"account":"a","agent":"x","tool":"send_message","chat_id":1,"chat_title":"t","summary":"s","ok":true}"#;
        let action: Action = serde_json::from_str(old).unwrap_or_else(|_| panic!("parses"));
        assert_eq!(action.id, 0);
        assert!(action.message_ids.is_empty());
        assert_eq!(action.outcome, Outcome::Done);
    }

    #[test]
    fn ids_never_repeat_not_even_after_clear_or_a_restart() {
        let account = AccountKey("activity-ids-test".into());
        let mut store = Stored::default();
        store.push(action(&account, "one"));
        store.push(action(&account, "two"));
        // Clear empties the entries but keeps the count.
        store.actions.clear();
        store.push(action(&account, "after clear"));
        let ids: Vec<u64> = store.actions.iter().map(|a| a.id).collect();
        assert_eq!(ids, [3]);
        // Saved and read back, the next id goes on from there.
        let bytes = serde_json::to_vec(&store).unwrap_or_default();
        let mut read = Stored::parse(&bytes).unwrap_or_default();
        read.actions.clear();
        read.push(action(&account, "after restart"));
        assert_eq!(read.actions.front().map(|a| a.id), Some(4));
    }

    #[test]
    fn a_log_saved_as_a_bare_list_reads_and_counts_on() {
        let account = AccountKey("activity-old-file-test".into());
        let mut old = action(&account, "old");
        old.id = 9;
        let bytes = serde_json::to_vec(&vec![old]).unwrap_or_default();
        let mut store = Stored::parse(&bytes).unwrap_or_default();
        assert_eq!(store.actions.len(), 1);
        store.push(action(&account, "new"));
        assert_eq!(store.actions.back().map(|a| a.id), Some(10));
    }

    #[test]
    fn only_a_write_that_went_through_can_be_undone() {
        crate::fast_buttons::set_persistence(false);
        let account = AccountKey("activity-undo-outcomes".into());
        for outcome in [Outcome::Failed, Outcome::Declined] {
            let mut refused = action(&account, "refused");
            refused.outcome = outcome;
            record(refused);
            let id = recent(&account).first().map_or(0, |a| a.id);
            mark_undone(&account, id);
            assert_eq!(recent(&account).first().map(|a| a.outcome), Some(outcome));
        }
    }

    #[test]
    fn the_outcome_reads_and_writes_the_old_flags() {
        let account = AccountKey("activity-flags".into());
        for (outcome, flags) in [
            (Outcome::Done, (true, false, false)),
            (Outcome::Failed, (false, false, false)),
            (Outcome::Declined, (false, true, false)),
            (Outcome::Undone, (true, false, true)),
        ] {
            let mut a = action(&account, "x");
            a.outcome = outcome;
            let json = serde_json::to_value(&a).unwrap_or_default();
            let flag = |key: &str| json.get(key).and_then(|v| v.as_bool());
            assert_eq!(
                (flag("ok"), flag("declined"), flag("undone")),
                (Some(flags.0), Some(flags.1), Some(flags.2))
            );
            let back: Action = serde_json::from_value(json).unwrap_or_else(|_| panic!("reads"));
            assert_eq!(back.outcome, outcome);
        }
    }

    #[test]
    fn a_revoked_process_stays_refused_under_another_name() {
        let _lock = crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK);
        let guard = super::connect_from("Rename test", Some(4_242_001));
        revoke(guard.id());
        assert!(super::refused_from("Rename test 2", Some(4_242_001)));
        assert!(!super::refused_from("Rename test 2", Some(4_242_002)));
        assert!(!super::refused_from("Rename test 2", None));
        allow_again("Rename test");
        assert!(!super::refused_from("Rename test 2", Some(4_242_001)));
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
        // Turning agent access off in another test lets every agent in again.
        let _lock = crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK);
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
