//! Force-reply keyboard bar (`parity:bots-force-reply-keyboard`).
//!
//! A bot message carrying `replyMarkupForceReply` renders the
//! reply-keyboard bar above the composer — the same slot the custom
//! keyboard panel uses. The bar shows the bot's prompt
//! (`input_field_placeholder`, which TGX surfaces via
//! `setCustomBotPlaceholder`); tapping it focuses the composer.
//!
//! Lifetime: the bar shows exactly while the forced reply is still the
//! composer's reply-to — sending or cancelling the reply clears the
//! reply-to, which hides the bar (the one-time-keyboard "hides on tap"
//! analog). Dismissal reuses the custom-keyboard `dismissed_keyboards`
//! set.
//!
//! TGX parity (`BotHelper.processForceReply`): a standalone force-reply
//! also dismisses the chat's custom keyboard, locally and via
//! `deleteChatReplyMarkup`. A `replyMarkupShowKeyboard` carrying the
//! `force_reply` flag keeps its keyboard (TGX renders it).

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::*;
use quill::force_reply::active_force_reply;
use quill::ids::{ChatId, MessageId};
use quill::state::{ForceReplyTarget, active_custom_keyboard};
use quill::telegram::envelope::{ReplyKeyboard, ReplyMarkup};
impl QuillApp {
    /// Drain one armed force-reply target: arm the composer reply-to +
    /// focus (B1 behavior), then dismiss the chat's custom keyboard when
    /// the arming markup was a standalone `replyMarkupForceReply` (TGX
    /// `processForceReply` sends `DeleteChatReplyMarkup` on arrival). A
    /// show-keyboard carrying the `force_reply` flag keeps its keyboard.
    /// Called from `flush_notifications`, which runs at the top of
    /// `render`.
    pub(crate) fn drain_force_reply(
        &mut self,
        target: ForceReplyTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_force_reply(target, window, cx);
        let standalone = self
            .session()
            .and_then(|session| session.histories.get(&target.chat_id.0))
            .and_then(|history| history.messages.get(&target.message_id.0))
            .and_then(|message| message.reply_markup.as_ref())
            .is_some_and(|markup| matches!(markup, ReplyMarkup::ForceReply { .. }));
        if !standalone {
            return;
        }
        if let Some((chat_id, message_id, _)) = self.chat_custom_keyboard(target.chat_id) {
            self.dismiss_custom_keyboard(chat_id, message_id, cx);
        }
    }

    /// The active custom keyboard for one chat (live or demo session).
    fn chat_custom_keyboard(&self, chat_id: ChatId) -> Option<(ChatId, MessageId, ReplyKeyboard)> {
        let session = self.session()?;
        let history = session.histories.get(&chat_id.0)?;
        active_custom_keyboard(&history.messages, &self.dismissed_keyboards)
    }

    /// The force-reply bar target for the open chat, if any: the live
    /// standalone `replyMarkupForceReply` message the composer is actually
    /// replying to — shown only while its reply is still the composer's
    /// reply-to (sending or cancelling the reply hides the bar).
    ///
    /// The armed reply wins over the newest message: the drain arms the
    /// oldest unanswered force-reply first and refuses to re-arm while a
    /// draft exists, so after a second force-reply arrives mid-draft the
    /// newest-wins selector alone would point at a message the composer
    /// is not replying to (and the bar would vanish). Falling back to the
    /// selector covers a draft armed before this slice's logic existed.
    fn open_chat_force_reply(&self) -> Option<(ChatId, MessageId, String)> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        let history = session.histories.get(&chat_id.0)?;
        let pending = self.pending_reply.as_ref()?;
        let force_reply_placeholder = |message_id: MessageId| {
            history
                .messages
                .get(&message_id.0)
                .and_then(|message| message.reply_markup.as_ref())
                .and_then(|markup| match markup {
                    ReplyMarkup::ForceReply { placeholder } => Some(placeholder.clone()),
                    _ => None,
                })
                .filter(|_| {
                    !self
                        .dismissed_keyboards
                        .contains(&(chat_id.0, message_id.0))
                })
        };
        if pending.chat_id == chat_id {
            if let Some(placeholder) = force_reply_placeholder(pending.message_id) {
                return Some((chat_id, pending.message_id, placeholder));
            }
        }
        let (kb_chat, kb_message, placeholder) =
            active_force_reply(&history.messages, &self.dismissed_keyboards)?;
        (pending.chat_id == kb_chat && pending.message_id == kb_message).then_some((
            kb_chat,
            kb_message,
            placeholder,
        ))
    }

    /// The force-reply keyboard bar above the composer, or `None` when the
    /// open chat has no live force-reply. Same slot and border language as
    /// the custom keyboard panel; the label is the bot's
    /// `input_field_placeholder`. Tapping the bar focuses the composer.
    pub(crate) fn force_reply_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, message_id, placeholder) = self.open_chat_force_reply()?;
        let prompt = placeholder.trim();
        let label = if prompt.is_empty() {
            "↩ Reply requested".to_string()
        } else {
            format!("↩ {prompt}")
        };
        Some(
            div()
                .id(("force-reply-bar", message_id.0 as u64))
                .flex()
                .flex_col()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(border())
                .child(
                    Button::new("force-reply-focus")
                        .ghost()
                        .label(label)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.composer
                                .update(cx, |input, cx| input.focus(window, cx));
                        })),
                )
                .into_any_element(),
        )
    }
}
