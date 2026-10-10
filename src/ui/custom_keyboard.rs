//! custom reply keyboards.

use super::app::QuillApp;
use super::request_share::RequestShareKind;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::ComposerReplyTo;
use quill::ids::{ChatId, MessageId};
use quill::state::{ForceReplyTarget, effective_preview};
use quill::telegram::envelope::{KeyboardButton, KeyboardButtonType, ReplyKeyboard};
impl QuillApp {
    /// B1: the active custom keyboard for the open chat, if any (live or
    /// demo session).
    pub(super) fn open_chat_custom_keyboard(&self) -> Option<(ChatId, MessageId, ReplyKeyboard)> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        session.custom_keyboard_for_chat(chat_id, &self.dismissed_keyboards)
    }

    /// The composer's keyboard button: shows or hides the bot keyboard
    /// (tdesktop `_botKeyboardShow` / `_botKeyboardHide`). `None` when the
    /// open chat has no keyboard.
    pub(super) fn keyboard_toggle_button(&self, cx: &mut Context<Self>) -> Option<Button> {
        let (chat_id, message_id, _) = self.open_chat_custom_keyboard()?;
        let hidden = self
            .collapsed_keyboards
            .contains(&(chat_id.0, message_id.0));
        let label = if hidden {
            "Show bot keyboard"
        } else {
            "Hide bot keyboard"
        };
        Some(
            Button::new("composer-keyboard-toggle")
                .icon(if hidden {
                    gpui_kit::assets::IconName::Keyboard
                } else {
                    gpui_kit::assets::IconName::KeyboardOff
                })
                .ghost()
                .tooltip(label)
                .accessibility_label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let key = (chat_id.0, message_id.0);
                    if !this.collapsed_keyboards.remove(&key) {
                        this.collapsed_keyboards.insert(key);
                    }
                    cx.notify();
                })),
        )
    }

    /// B1: the custom keyboard panel rendered above the composer, or
    /// `None` when the open chat has no active `replyMarkupShowKeyboard`.
    /// `resize_keyboard` renders compact; `is_persistent` has no visual
    /// difference (it only tells TDLib to keep showing the keyboard).
    pub(super) fn custom_keyboard_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (chat_id, message_id, keyboard) = self.open_chat_custom_keyboard()?;
        if self
            .collapsed_keyboards
            .contains(&(chat_id.0, message_id.0))
        {
            return None;
        }
        let mut grid = div()
            .id(("custom-keyboard", message_id.0 as u64))
            .flex()
            .flex_col()
            .gap_1()
            .px_3();
        if keyboard.resize_keyboard {
            grid = grid.py_1();
        } else {
            grid = grid.py_2();
        }
        grid = grid.border_t_1().border_color(border());
        for (row_index, row) in keyboard.rows.iter().enumerate() {
            if row.is_empty() {
                continue;
            }
            let mut line = div()
                .id(format!("custom-keyboard-row-{}-{row_index}", message_id.0))
                .flex()
                .gap_1();
            for (button_index, button) in row.iter().enumerate() {
                line = line.child(
                    Self::custom_keyboard_button(
                        chat_id,
                        message_id,
                        row_index,
                        button_index,
                        button,
                        keyboard.one_time,
                        cx,
                    )
                    .flex_1(),
                );
            }
            grid = grid.child(line);
        }
        Some(grid.into_any_element())
    }

    /// B1: tap a `keyboardButtonTypeText` button — sends the button's text
    /// through the normal composer path (same as typing it and pressing
    /// Enter). A one-time keyboard hides first, locally and via
    /// `deleteChatReplyMarkup` (schema 1.8.67, line 13183).
    pub(super) fn press_custom_keyboard_text(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        text: &str,
        one_time: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if one_time {
            self.dismiss_custom_keyboard(chat_id, message_id, cx);
        }
        self.submit_composer(text.to_string(), window, cx);
    }

    /// B1: hide a one-time custom keyboard locally and tell TDLib
    /// (`deleteChatReplyMarkup`). Best-effort: the local hide stands even
    /// when offline.
    pub(super) fn dismiss_custom_keyboard(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        self.dismissed_keyboards.insert((chat_id.0, message_id.0));
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.delete_chat_reply_markup(chat_id, message_id);
        }
        cx.notify();
    }

    /// B1: apply a drained force-reply target — set the composer's
    /// reply-to and focus the composer (TGX behavior for
    /// `replyMarkupForceReply`). Only when the target chat is open, the
    /// message still exists, and the user has no reply draft already.
    pub(super) fn apply_force_reply(
        &mut self,
        target: ForceReplyTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ready = self
            .session()
            .filter(|session| session.open_chat == Some(target.chat_id))
            .and_then(|session| {
                session
                    .histories
                    .get(&target.chat_id.0)
                    .and_then(|history| history.messages.get(&target.message_id.0))
                    .map(effective_preview)
            });
        let Some(preview) = ready else {
            return;
        };
        if self.pending_reply.is_some() {
            return;
        }
        self.begin_reply_to(
            ComposerReplyTo::new(target.chat_id, target.message_id, preview),
            window,
            cx,
        );
    }

    /// B1: set the status-bar note and refresh.
    pub(super) fn set_status_note(&mut self, note: &str, cx: &mut Context<Self>) {
        self.status_note = note.into();
        cx.notify();
    }

    /// B1: one custom keyboard button. `Text` sends the text; `WebApp` opens
    /// the mini app window (`ui/web_app_ui.rs`); request
    /// contact/location/poll/user/chat/bot variants stay disabled with
    /// honest tooltips — Quill has no permission/selection flows for them.
    pub(super) fn custom_keyboard_button(
        chat_id: ChatId,
        message_id: MessageId,
        row_index: usize,
        button_index: usize,
        button: &KeyboardButton,
        one_time: bool,
        cx: &mut Context<QuillApp>,
    ) -> Button {
        let element = Button::new(format!(
            "kbd-btn-{}-{row_index}-{button_index}",
            message_id.0
        ))
        .label(button.text.clone())
        .tooltip(Self::custom_keyboard_button_tooltip(&button.kind));
        match &button.kind {
            KeyboardButtonType::Text => {
                let text = button.text.clone();
                element.on_click(cx.listener(move |this, _, window, cx| {
                    this.press_custom_keyboard_text(
                        chat_id, message_id, &text, one_time, window, cx,
                    );
                }))
            }
            KeyboardButtonType::WebApp { url } => {
                let url = url.clone();
                let text = button.text.clone();
                element.on_click(cx.listener(move |this, _, _, cx| {
                    this.open_simple_web_app(chat_id, &text, &url, cx);
                }))
            }
            KeyboardButtonType::RequestPhoneNumber => {
                element.on_click(cx.listener(move |this, _, _, cx| {
                    let bot = this.session().and_then(|s| s.bot_user_id_for_chat(chat_id));
                    match bot {
                        Some(bot_user_id) => this.open_request_share(
                            chat_id,
                            message_id,
                            RequestShareKind::Phone { bot_user_id },
                            cx,
                        ),
                        None => this.set_status_note("Only a bot can ask for your number", cx),
                    }
                }))
            }
            KeyboardButtonType::RequestUsers(spec) => {
                let spec = spec.clone();
                element.on_click(cx.listener(move |this, _, _, cx| {
                    this.open_request_share(
                        chat_id,
                        message_id,
                        RequestShareKind::Users(spec.clone()),
                        cx,
                    );
                }))
            }
            KeyboardButtonType::RequestChat(spec) => {
                let spec = spec.clone();
                element.on_click(cx.listener(move |this, _, _, cx| {
                    this.open_request_share(
                        chat_id,
                        message_id,
                        RequestShareKind::Chat(spec.clone()),
                        cx,
                    );
                }))
            }
            _ => element.disabled(true),
        }
    }

    /// B1: short hint for unsupported custom keyboard buttons.
    pub(super) fn custom_keyboard_button_tooltip(kind: &KeyboardButtonType) -> &'static str {
        match kind {
            KeyboardButtonType::Text | KeyboardButtonType::WebApp { .. } => "",
            KeyboardButtonType::RequestPhoneNumber => "Share your phone number with the bot",
            KeyboardButtonType::RequestLocation => "Sharing your location is not supported yet",
            KeyboardButtonType::RequestPoll => "Creating a poll is not supported yet",
            KeyboardButtonType::RequestUsers(_) => "Choose users to share with the bot",
            KeyboardButtonType::RequestChat(_) => "Choose a chat to share with the bot",
            KeyboardButtonType::RequestManagedBot => "Bot setup is not supported yet",
            KeyboardButtonType::Unknown { .. } => "Unsupported button",
        }
    }
}
