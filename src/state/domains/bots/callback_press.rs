//! Inline button presses waiting for the bot's answer. Telegram Desktop
//! keeps a request id on the pressed button and draws it loading until the
//! answer comes (`ReplyKeyboard::ButtonCallbackRequest`); here the press
//! names its request, and the button is loading while that request is in
//! flight, so an answer, an error or a dropped request all end it.
use crate::state::*;

/// Which inline button of a message was pressed: a callback button by its
/// data (password buttons too), or the message's game button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackButtonKey {
    Data(Vec<u8>),
    Game,
}

/// One press and the `getCallbackQueryAnswer` request it sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackPress {
    pub request: RequestId,
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub button: CallbackButtonKey,
}

impl Session {
    /// Remember which button `request` was sent for. Presses whose request
    /// has ended are dropped here, so the list holds only live ones.
    pub(crate) fn note_callback_press(&mut self, press: CallbackPress) {
        let requests = &self.requests;
        self.bots
            .callback_presses
            .retain(|old| requests.pending.contains_key(&old.request.0));
        self.bots.callback_presses.push(press);
    }

    /// Whether this button's press still waits for the bot.
    pub fn callback_button_waiting(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        button: &CallbackButtonKey,
    ) -> bool {
        self.bots.callback_presses.iter().any(|press| {
            press.chat_id == chat_id
                && press.message_id == message_id
                && press.button == *button
                && self.requests.pending.contains_key(&press.request.0)
        })
    }
}
