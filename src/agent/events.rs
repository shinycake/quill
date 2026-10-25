//! What agents learn without asking: new messages and the chat folders.
//!
//! The hub's receive loop hands every raw TDLib envelope to [`observe`]
//! before parsing. TDLib writes `@type` first, so a prefix test picks out
//! the two updates this module wants and costs the rest almost nothing.
//! Folders are kept always (TDLib sends them once, at start); new messages
//! only while an agent is connected, in a short buffer that
//! `wait_for_message` and resource subscriptions read. Grants are applied by
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
const CHAT_FOLDERS: &str = "{\"@type\":\"updateChatFolders\"";

/// Look at one raw envelope from TDLib.
pub fn observe(json: &str) {
    if json.starts_with(NEW_MESSAGE) {
        if super::server::connected_agents() > 0 {
            record_message(json);
        }
    } else if json.starts_with(CHAT_FOLDERS) {
        record_folders(json);
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
/// `timeout` for the first one. Returns as soon as any arrive.
pub fn wait_for(
    client_id: i32,
    since: u64,
    timeout: Duration,
    mut keep: impl FnMut(&NewMessage) -> bool,
) -> Vec<NewMessage> {
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
            return kept;
        }
        let now = Instant::now();
        if now >= deadline {
            return Vec::new();
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
        let got = waiter.join().unwrap_or_default();
        assert_eq!(got.len(), 1);
        assert_eq!(got.first().map(|m| m.chat_id), Some(5));
        assert!(wait_for(91, super::latest_seq(), Duration::from_millis(20), |_| true).is_empty());
    }
}
