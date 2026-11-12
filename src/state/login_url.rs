//! The login-URL button press's context (the stage is `LoginUrlStage` in the bots domain).
use crate::ids::{ChatId, MessageId};

/// B1: context of an in-flight login-URL button press. Kept while
/// `getLoginUrlInfo` (and then `getLoginUrl`, after consent) is in flight,
/// so an error degrades the button to a plain URL-button press (schema
/// 1.8.67 doc on `getLoginUrl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginUrlRequest {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub button_id: i64,
    /// The button's raw URL — the plain-URL fallback.
    pub raw_url: String,
    /// Pressed by a step the agent UI driver injected: what the answer
    /// opens runs as part of that step (`guard::with_injecting`).
    pub from_agent: bool,
}
