//! Connect gate: JSON sender abstraction and test/live senders.
use super::*;
use crate::calls::engine::{RemoteVideoState, TransportState};
use crate::telegram::client::LiveTdJson;
use crate::telegram::ffi::TdJsonError;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Outbound JSON (live `td_send` or test recorder). Must not log request bodies.
pub trait JsonSender: Send {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError>;
}

/// Records outbound JSON for unit/replay tests (no network).
#[derive(Default)]
pub struct RecordingSender {
    pub sent: Mutex<Vec<String>>,
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
        Ok(())
    }
}

impl JsonSender for Arc<RecordingSender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        (**self).send_json(request)
    }
}

/// Live `td_send` wrapper.
pub struct LiveSender {
    pub(crate) api: Arc<crate::telegram::ffi::TdJson>,
    client_id: i32,
}

impl LiveSender {
    pub fn from_live(live: &LiveTdJson) -> Self {
        Self {
            api: live.api.clone(),
            client_id: live.client_id,
        }
    }
}

impl JsonSender for LiveSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        self.api.send(self.client_id, request).map_err(|e| match e {
            TdJsonError::InvalidRequest => ConnectSendError::InvalidRequest,
            _ => ConnectSendError::Native,
        })
    }
}

/// Phase C2b: worker-safe queue for engine-emitted signaling.
pub(crate) type SignalingOutbox = Arc<Mutex<VecDeque<(i32, Vec<u8>)>>>;
pub(crate) type TransportOutbox = Arc<Mutex<VecDeque<(i32, TransportState)>>>;
/// Phase C2e: worker-safe queue for engine-emitted peer camera states.
pub(crate) type VideoStateOutbox = Arc<Mutex<VecDeque<(i32, RemoteVideoState)>>>;
