use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
/// B1: password prompt for an `inlineKeyboardButtonTypeCallbackWithPassword`
/// button press (TDLib 1.8.67, `schema/td_api.tl:3789`). Submits the entered
/// Slice A2: which form the two-step verification overlay shows.
/// The status screen is the hub; each form submits one TDLib request
/// and the status screen renders the authoritative answer.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TwofaView {
    #[default]
    Status,
    Enable,
    Change,
    Disable,
    Email,
}

/// 2-step password with the button's callback `data` via
/// `callbackQueryPayloadDataWithPassword` (schema:7740); the `data` is
/// cleared from memory after the send.
pub struct CallbackPasswordDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    pub(crate) data: Vec<u8>,
    pub(crate) password_input: Entity<TextareaState>,
}

impl CallbackPasswordDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        chat_id: ChatId,
        message_id: MessageId,
        data: Vec<u8>,
    ) -> Self {
        let password_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("2-step verification password")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            chat_id,
            message_id,
            data,
            password_input,
        }
    }
}
