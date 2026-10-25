//! Connect gate: JSON sender abstraction and test/live senders.
use super::*;
use crate::calls::engine::{RemoteVideoState, TransportState};
use crate::ids::RequestId;
use crate::locks::LockExt;
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

    /// The guard that screens sends into paid chats; senders that do not
    /// talk to TDLib have none.
    fn paid_guard(&self) -> Option<&crate::paid_messages_guard::PaidGuard> {
        None
    }

    /// TDLib-shaped answers for requests the agent UI driver's screen
    /// refused without sending ([`AgentScreen`]), oldest first. The driver
    /// ingests them at the start of each batch.
    fn take_refused_answers(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The agent UI driver's screen in front of a sender
/// (`guard::screen_request`): while an agent's input is injected, only
/// reads and the changes looking around makes go out. A read refused for
/// money is answered here like TDLib refusing it (`guard::refused_read_answer`),
/// so its caller's error path clears what it set loading; anything else
/// is refused with [`ConnectSendError::AgentDriving`].
#[derive(Default)]
pub struct AgentScreen {
    answers: Mutex<Vec<String>>,
}

impl AgentScreen {
    /// `Ok(true)` when `request` may go out, `Ok(false)` when it was
    /// answered here instead.
    pub fn screen(&self, request: &str) -> Result<bool, ConnectSendError> {
        use crate::agent::ui_driver::guard;
        if guard::screen_request(request).is_ok() {
            return Ok(true);
        }
        let Some(answer) = guard::refused_read_answer(request) else {
            return Err(ConnectSendError::AgentDriving);
        };
        self.answers.lock_or_recover().push(answer);
        crate::wake::UPDATES.poke();
        Ok(false)
    }

    pub fn take_answers(&self) -> Vec<String> {
        std::mem::take(&mut *self.answers.lock_or_recover())
    }
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
        let mut map = self.map.lock_or_recover();
        if map.len() >= RETRY_STASH_LIMIT {
            map.clear();
        }
        map.insert(extra, request.to_string());
    }

    pub fn take(&self, extra: RequestId) -> Option<String> {
        self.map.lock_or_recover().remove(&extra.0)
    }

    pub fn len(&self) -> usize {
        self.map.lock_or_recover().len()
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
    guard: crate::paid_messages_guard::PaidGuard,
    agent: AgentScreen,
}

impl RecordingSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.sent.lock_or_recover().clone()
    }
}

impl JsonSender for RecordingSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        // The same agent screen as the live sender (a no-op unless a test
        // injects).
        if !self.agent.screen(request)? {
            return Ok(());
        }
        // A send into a paid chat waits for the user's confirmation.
        let Some(request) = self.guard.screen(request) else {
            return Ok(());
        };
        self.sent.lock_or_recover().push(request.clone());
        self.stash.note(&request);
        Ok(())
    }

    fn paid_guard(&self) -> Option<&crate::paid_messages_guard::PaidGuard> {
        Some(&self.guard)
    }

    fn take_retained(&self, extra: RequestId) -> Option<String> {
        self.stash.take(extra)
    }

    fn forget_retained(&self, extra: RequestId) {
        self.stash.take(extra);
    }

    fn take_refused_answers(&self) -> Vec<String> {
        self.agent.take_answers()
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

    fn paid_guard(&self) -> Option<&crate::paid_messages_guard::PaidGuard> {
        (**self).paid_guard()
    }

    fn take_refused_answers(&self) -> Vec<String> {
        (**self).take_refused_answers()
    }
}

/// Live `td_send` wrapper.
pub struct LiveSender {
    pub(crate) api: Arc<crate::telegram::ffi::TdJson>,
    client_id: i32,
    stash: RetryStash,
    guard: crate::paid_messages_guard::PaidGuard,
    agent: AgentScreen,
}

impl LiveSender {
    pub fn from_live(live: &LiveTdJson) -> Self {
        Self {
            api: live.api.clone(),
            client_id: live.client_id,
            stash: RetryStash::default(),
            guard: Default::default(),
            agent: AgentScreen::default(),
        }
    }

    /// `parity:data-proxy-web`: sends a carrier page's messages to this
    /// client from the carrier host's reader threads (`td_send` is
    /// thread-safe). They skip the paid-chat screen and the retry stash:
    /// they are never paid sends and never retried.
    pub fn relay_sink(&self) -> crate::proxy_carrier::RelaySink {
        let api = self.api.clone();
        let client_id = self.client_id;
        Arc::new(move |request: &str| api.send(client_id, request).is_ok())
    }
}

impl JsonSender for LiveSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        // While the agent UI driver injects input, only reads and the
        // changes that looking around makes go out (`AgentScreen`).
        if !self.agent.screen(request)? {
            return Ok(());
        }
        // A send into a paid chat waits for the user's confirmation.
        let Some(request) = self.guard.screen(request) else {
            return Ok(());
        };
        let request = request.as_str();
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

    fn paid_guard(&self) -> Option<&crate::paid_messages_guard::PaidGuard> {
        Some(&self.guard)
    }

    fn take_refused_answers(&self) -> Vec<String> {
        self.agent.take_answers()
    }
}

/// Phase C2b: worker-safe queue for engine-emitted signaling.
pub(crate) type SignalingOutbox = Arc<Mutex<VecDeque<(i32, Vec<u8>)>>>;
pub(crate) type TransportOutbox = Arc<Mutex<VecDeque<(i32, TransportState)>>>;
/// Phase C2e: worker-safe queue for engine-emitted peer camera states.
pub(crate) type VideoStateOutbox = Arc<Mutex<VecDeque<(i32, RemoteVideoState)>>>;
