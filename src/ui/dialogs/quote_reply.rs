use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
/// Slice G1: which close action a modal dialog's backdrop / close
/// button runs.
#[derive(Debug, Clone, PartialEq, Eq)]

/// Slice G1: partial-quote dialog (message menu → "Quote reply").
/// The input is prefilled with the message's full text; the user
/// trims it down to the quoted part. Submit validates that the
/// remainder is a verbatim substring and computes its UTF-16 offset
/// (`inputTextQuote`, schema 1.8.67, line 3056).
pub struct QuoteReplyDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    pub(crate) full_text: String,
    pub(crate) input: Entity<TextareaState>,
}

impl QuoteReplyDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        chat_id: ChatId,
        message_id: MessageId,
        full_text: String,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Trim to the part to quote")
                .auto_grow(2, 6)
                .submit_on_enter(false)
        });
        input.update(cx, |input, cx| {
            input.set_value(&full_text, window, cx);
        });
        Self {
            chat_id,
            message_id,
            full_text,
            input,
        }
    }
}
