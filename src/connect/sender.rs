//! Connect gate: JSON sender abstraction and test/live senders.
use super::*;
use crate::calls::engine::{RemoteVideoState, TransportState};
use crate::ids::RequestId;
use crate::telegram::client::LiveTdJson;
use crate::telegram::ffi::TdJsonError;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Outbound JSON (live `td_send` or test recorder). Must not log request bodies.
pub trait JsonSender: Send {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError>;

    /// Q1: take back the retained JSON of an idempotent request (see
    /// [`RetryStash`]) so it can be re-sent after a flood wait. Senders
    /// that do not retain requests return `None`.
    fn take_retained(&self, _extra: RequestId) -> Option<String> {
        None
    }

    /// Q1: the request was answered; drop its retained JSON.
    fn forget_retained(&self, _extra: RequestId) {}
}

/// Q1: TDLib `@type`s that are safe to re-send verbatim after a rate
/// limit (reads and paging; a repeat never duplicates a side effect).
const RETRYABLE_TYPES: [&str; 22] = [
    "getChatHistory",
    "getMessageThreadHistory",
    "searchChatMessages",
    "searchMessages",
    "searchPublicChats",
    "searchChats",
    "searchChatsOnServer",
    "getChat",
    "getFile",
    "getRemoteFile",
    "getMessage",
    "getMessages",
    "getRepliedMessage",
    "getChatPinnedMessage",
    "getUserFullInfo",
    "getSupergroupFullInfo",
    "getBasicGroupFullInfo",
    "getChatMember",
    "getForumTopics",
    "getChatMessageByDate",
    "getChatScheduledMessages",
    "loadChats",
];

/// Most retained requests at once; a runaway producer clears the stash
/// instead of growing it (a missed retry only means the error surfaces).
const RETRY_STASH_LIMIT: usize = 256;

/// Q1: remembers the JSON of in-flight idempotent requests by `@extra`,
/// so a 429 can be answered by re-sending the same request. Only the
/// [`RETRYABLE_TYPES`] are kept; secrets (auth, passwords) never are.
#[derive(Default)]
pub struct RetryStash {
    map: Mutex<std::collections::HashMap<u64, String>>,
}

impl RetryStash {
    pub fn note(&self, request: &str) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(request) else {
            return;
        };
        let retryable = value
            .get("@type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| RETRYABLE_TYPES.contains(&kind));
        let Some(extra) = value
            .get("@extra")
            .and_then(serde_json::Value::as_str)
            .and_then(|extra| extra.parse::<u64>().ok())
        else {
            return;
        };
        if !retryable {
            return;
        }
        let mut map = self.map.lock().expect("retry stash");
        if map.len() >= RETRY_STASH_LIMIT {
            map.clear();
        }
        map.insert(extra, request.to_string());
    }

    pub fn take(&self, extra: RequestId) -> Option<String> {
        self.map.lock().expect("retry stash").remove(&extra.0)
    }

    pub fn len(&self) -> usize {
        self.map.lock().expect("retry stash").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Records outbound JSON for unit/replay tests (no network).
#[derive(Default)]
pub struct RecordingSender {
    pub sent: Mutex<Vec<String>>,
    stash: RetryStash,
}

impl RecordingSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.sent.lock().expect("recording sender").clone()
    }
}

impl JsonSender for RecordingSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        self.sent
            .lock()
            .expect("recording sender")
            .push(request.to_string());
        self.stash.note(request);
        Ok(())
    }

    fn take_retained(&self, extra: RequestId) -> Option<String> {
        self.stash.take(extra)
    }

    fn forget_retained(&self, extra: RequestId) {
        self.stash.take(extra);
    }
}

impl JsonSender for Arc<RecordingSender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        (**self).send_json(request)
    }

    fn take_retained(&self, extra: RequestId) -> Option<String> {
        (**self).take_retained(extra)
    }

    fn forget_retained(&self, extra: RequestId) {
        (**self).forget_retained(extra)
    }
}

/// Live `td_send` wrapper.
pub struct LiveSender {
    pub(crate) api: Arc<crate::telegram::ffi::TdJson>,
    client_id: i32,
    stash: RetryStash,
}

impl LiveSender {
    pub fn from_live(live: &LiveTdJson) -> Self {
        Self {
            api: live.api.clone(),
            client_id: live.client_id,
            stash: RetryStash::default(),
        }
    }
}

impl JsonSender for LiveSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        self.api
            .send(self.client_id, request)
            .map_err(|e| match e {
                TdJsonError::InvalidRequest => ConnectSendError::InvalidRequest,
                _ => ConnectSendError::Native,
            })?;
        self.stash.note(request);
        Ok(())
    }

    fn take_retained(&self, extra: RequestId) -> Option<String> {
        self.stash.take(extra)
    }

    fn forget_retained(&self, extra: RequestId) {
        self.stash.take(extra);
    }
}

/// Phase C2b: worker-safe queue for engine-emitted signaling.
pub(crate) type SignalingOutbox = Arc<Mutex<VecDeque<(i32, Vec<u8>)>>>;
pub(crate) type TransportOutbox = Arc<Mutex<VecDeque<(i32, TransportState)>>>;
/// Phase C2e: worker-safe queue for engine-emitted peer camera states.
pub(crate) type VideoStateOutbox = Arc<Mutex<VecDeque<(i32, RemoteVideoState)>>>;
