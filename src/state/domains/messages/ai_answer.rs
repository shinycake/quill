//! An answer to an AI composer action (Fix, Rewrite, Create), waiting for
//! the window that asked.
use crate::state::*;

/// What TDLib answered.
#[derive(Debug, Clone, PartialEq)]
pub enum AiResult {
    /// `fixTextWithAi` / `composeTextWithAi`: the new draft markup.
    Text(String),
    /// A rich message, and the note naming the method that answered.
    Blocks(RichMessageContent, &'static str),
    /// The request failed (the reason is in `MessagesState::ai_error`).
    Failed,
}

/// One answer, for request `request`. The window that asked takes it
/// (`None`: the main window) and only while it still holds that request;
/// `chat` guards against a chat the window has since left.
#[derive(Debug, Clone, PartialEq)]
pub struct AiAnswer {
    pub request: RequestId,
    pub window: Option<ChatId>,
    pub chat: ChatId,
    pub result: AiResult,
}
