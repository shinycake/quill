//! What agents learn without asking: new messages, the chat folders, and
//! how the messages they sent ended.
//!
//! The hub's receive loop hands every raw TDLib envelope to [`observe`]
//! before parsing. TDLib writes `@type` first, so a prefix test picks out
//! the updates this module wants and costs the rest almost nothing.
//! Folders are kept always (TDLib sends them once, at start); new messages
//! only while an agent is connected, in a short buffer that
//! `wait_for_message` and resource subscriptions read; send outcomes too,
//! for the write tools to report final message ids. Grants are applied by
//! the readers, never here: this runs on the receive thread and must not
//! block.

use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// New messages kept for waiters.
const BUFFER: usize = 256;

/// One new message as TDLib sent it.
#[derive(Clone)]
pub struct NewMessage {
    /// Increases by one per message, across clients.
    pub seq: u64,
    pub client_id: i32,
    pub chat_id: i64,
    pub outgoing: bool,
    pub message: Value,
}

#[derive(Default)]
struct Messages {
    next_seq: u64,
    recent: VecDeque<NewMessage>,
}

static MESSAGES: Mutex<Option<Messages>> = Mutex::new(None);
static ARRIVED: Condvar = Condvar::new();
/// Chat folders per client: (id, name), in the user's order.
/// (id, name) of each folder, in the user's order.
type Folders = Vec<(i64, String)>;
static FOLDERS: Mutex<Option<HashMap<i32, Folders>>> = Mutex::new(None);

const NEW_MESSAGE: &str = "{\"@type\":\"updateNewMessage\"";
const SEND_SUCCEEDED: &str = "{\"@type\":\"updateMessageSendSucceeded\"";
const SEND_FAILED: &str = "{\"@type\":\"updateMessageSendFailed\"";
const CHAT_FOLDERS: &str = "{\"@type\":\"updateChatFolders\"";

/// Look at one raw envelope from TDLib.
pub fn observe(json: &str) {
    if json.starts_with(NEW_MESSAGE) {
        if super::server::connected_agents() > 0 {
            record_message(json);
        }
    } else if json.starts_with(CHAT_FOLDERS) {
        record_folders(json);
    } else if (json.starts_with(SEND_SUCCEEDED) || json.starts_with(SEND_FAILED))
        && super::server::connected_agents() > 0
    {
        record_send(json);
    }
}

/// How a message Quill handed to Telegram ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Sent {
    /// Delivered; the message with its final id.
    Delivered(Value),
    /// Telegram refused it: its code and message.
    Failed(i64, String),
}

/// Outcomes kept for waiters.
const SENT_BUFFER: usize = 256;

/// (client, temporary message id, outcome), oldest first.
type Outcomes = VecDeque<(i32, i64, Sent)>;
static OUTCOMES: Mutex<Option<Outcomes>> = Mutex::new(None);
static SETTLED: Condvar = Condvar::new();

fn record_send(json: &str) {
    let Ok(update) = serde_json::from_str::<Value>(json) else {
        return;
    };
    let Some(old_id) = update.get("old_message_id").and_then(Value::as_i64) else {
        return;
    };
    let message = update.get("message").cloned().unwrap_or(Value::Null);
    let outcome =
        if update.get("@type").and_then(Value::as_str) == Some("updateMessageSendSucceeded") {
            Sent::Delivered(message)
        } else {
            let error = update.get("error");
            Sent::Failed(
                error
                    .and_then(|e| e.get("code"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                error
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_string(),
            )
        };
    let mut guard = crate::locks::lock_or_recover(&OUTCOMES);
    let outcomes = guard.get_or_insert_with(VecDeque::new);
    outcomes.push_back((client_of(&update), old_id, outcome));
    while outcomes.len() > SENT_BUFFER {
        outcomes.pop_front();
    }
    drop(guard);
    SETTLED.notify_all();
}

/// Wait up to `timeout` for every message in `pending` (temporary ids of
/// `client`) to be delivered or refused. Each slot is `None` if it is still
/// pending when the time runs out.
pub fn wait_sent(client_id: i32, pending: &[i64], timeout: Duration) -> Vec<Option<Sent>> {
    let deadline = Instant::now() + timeout;
    let mut guard = crate::locks::lock_or_recover(&OUTCOMES);
    loop {
        let found: Vec<Option<Sent>> = pending
            .iter()
            .map(|id| {
                guard.as_ref().and_then(|outcomes| {
                    outcomes
                        .iter()
                        .rev()
                        .find(|(client, old, _)| *client == client_id && old == id)
                        .map(|(_, _, sent)| sent.clone())
                })
            })
            .collect();
        let now = Instant::now();
        if found.iter().all(Option::is_some) || now >= deadline {
            return found;
        }
        guard = match SETTLED.wait_timeout(guard, deadline.saturating_duration_since(now)) {
            Ok((guard, _)) => guard,
            Err(poisoned) => poisoned.into_inner().0,
        };
    }
}

fn client_of(update: &Value) -> i32 {
    update
        .get("@client_id")
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
        .unwrap_or(0)
}

fn record_message(json: &str) {
    let Ok(update) = serde_json::from_str::<Value>(json) else {
        return;
    };
    let Some(message) = update.get("message").cloned() else {
        return;
    };
    let chat_id = message.get("chat_id").and_then(Value::as_i64).unwrap_or(0);
    let outgoing = message
        .get("is_outgoing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut guard = crate::locks::lock_or_recover(&MESSAGES);
    let state = guard.get_or_insert_with(Messages::default);
    state.next_seq = state.next_seq.wrapping_add(1);
    let seq = state.next_seq;
    state.recent.push_back(NewMessage {
        seq,
        client_id: client_of(&update),
        chat_id,
        outgoing,
        message,
    });
    while state.recent.len() > BUFFER {
        state.recent.pop_front();
    }
    drop(guard);
    ARRIVED.notify_all();
}

fn folder_name(folder: &Value) -> String {
    // `chatFolderName` (TDLib 1.8.53+) wraps formatted text; older builds sent a string.
    let name = folder.get("name");
    name.and_then(|n| n.get("text"))
        .and_then(|t| t.get("text").or(Some(t)))
        .and_then(Value::as_str)
        .or_else(|| name.and_then(Value::as_str))
        .or_else(|| folder.get("title").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn record_folders(json: &str) {
    let Ok(update) = serde_json::from_str::<Value>(json) else {
        return;
    };
    let folders = update
        .get("chat_folders")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|f| Some((f.get("id")?.as_i64()?, folder_name(f))))
                .collect()
        })
        .unwrap_or_default();
    crate::locks::lock_or_recover(&FOLDERS)
        .get_or_insert_with(HashMap::new)
        .insert(client_of(&update), folders);
}

/// The chat folders of `client_id`, in the user's order.
pub fn folders(client_id: i32) -> Vec<(i64, String)> {
    crate::locks::lock_or_recover(&FOLDERS)
        .as_ref()
        .and_then(|all| all.get(&client_id).cloned())
        .unwrap_or_default()
}

/// The newest sequence number so far; messages after it are "new".
pub fn latest_seq() -> u64 {
    crate::locks::lock_or_recover(&MESSAGES)
        .as_ref()
        .map_or(0, |state| state.next_seq)
}

/// Messages of `client_id` after `since` that `keep` accepts, waiting up to
/// `timeout` for the first one. Returns as soon as any arrive, with the
/// sequence number checked up to, which the next wait should start from.
pub fn wait_for(
    client_id: i32,
    since: u64,
    timeout: Duration,
    mut keep: impl FnMut(&NewMessage) -> bool,
) -> (Vec<NewMessage>, u64) {
    let deadline = Instant::now() + timeout;
    let mut checked = since;
    let mut guard = crate::locks::lock_or_recover(&MESSAGES);
    loop {
        let fresh: Vec<NewMessage> = guard
            .as_ref()
            .map(|state| {
                state
                    .recent
                    .iter()
                    .filter(|m| m.seq > checked && m.client_id == client_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if let Some(last) = guard.as_ref().map(|state| state.next_seq) {
            checked = checked.max(last);
        }
        // `keep` may call TDLib; never hold the buffer while it runs.
        drop(guard);
        let kept: Vec<NewMessage> = fresh.into_iter().filter(|m| keep(m)).collect();
        if !kept.is_empty() {
            return (kept, checked);
        }
        let now = Instant::now();
        if now >= deadline {
            return (Vec::new(), checked);
        }
        guard = crate::locks::lock_or_recover(&MESSAGES);
        let latest = guard.as_ref().map_or(0, |state| state.next_seq);
        if latest <= checked {
            guard = match ARRIVED.wait_timeout(guard, deadline.saturating_duration_since(now)) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{folder_name, folders, observe, record_message, wait_for};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn send_outcomes_reach_their_waiter() {
        let waiter = std::thread::spawn(|| super::wait_sent(93, &[10, 11], Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(50));
        super::record_send(
            r#"{"@type":"updateMessageSendSucceeded","message":{"id":2048,"chat_id":5},"old_message_id":10,"@client_id":93}"#,
        );
        super::record_send(
            r#"{"@type":"updateMessageSendFailed","message":{"id":11},"old_message_id":11,"error":{"@type":"error","code":400,"message":"CHAT_WRITE_FORBIDDEN"},"@client_id":93}"#,
        );
        let got = waiter.join().unwrap_or_default();
        assert!(matches!(got.first(), Some(Some(super::Sent::Delivered(m))) if m["id"] == 2048));
        assert_eq!(
            got.get(1),
            Some(&Some(super::Sent::Failed(
                400,
                "CHAT_WRITE_FORBIDDEN".into()
            )))
        );
    }

    #[test]
    fn folders_are_kept_per_client() {
        observe(
            r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":3,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false}}],"main_chat_list_position":0,"are_tags_enabled":false,"@client_id":77}"#,
        );
        assert_eq!(folders(77), vec![(3, "Work".to_string())]);
        assert_eq!(folder_name(&json!({"title": "Old"})), "Old");
    }

    #[test]
    fn waiters_see_messages_that_arrive_after_they_start() {
        let since = super::latest_seq();
        let waiter = std::thread::spawn(move || {
            wait_for(91, since, Duration::from_secs(5), |m| m.chat_id == 5)
        });
        std::thread::sleep(Duration::from_millis(50));
        record_message(
            r#"{"@type":"updateNewMessage","message":{"id":1,"chat_id":4},"@client_id":91}"#,
        );
        record_message(
            r#"{"@type":"updateNewMessage","message":{"id":2,"chat_id":5},"@client_id":91}"#,
        );
        let (got, _) = waiter.join().unwrap_or_default();
        assert_eq!(got.len(), 1);
        assert_eq!(got.first().map(|m| m.chat_id), Some(5));
        let latest = super::latest_seq();
        let (none, checked) = wait_for(91, latest, Duration::from_millis(20), |_| true);
        assert!(none.is_empty());
        assert_eq!(checked, latest);
    }
}
