//! Telegram Desktop's pin and unpin boxes (`pin_messages_box.cpp`,
//! `ToggleMessagePinned`, `UnpinMessages`): a confirmation with "Notify
//! all members" in groups or "Also pin for {user}" in private chats.

use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::selection_pin::{PinChoice, pin_choice, pin_question, unpin_question};
use quill::telegram::envelope::ChatKind;
use std::cell::Cell;
use std::rc::Rc;

impl QuillApp {
    /// The message menu's Pin / Unpin: pinning opens the pin box, unpinning
    /// asks "Would you like to unpin this message?".
    pub(super) fn request_toggle_pin(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pinned = self
            .session()
            .and_then(|s| s.histories.get(&chat_id.0)?.messages.get(&message_id.0))
            .is_some_and(|message| message.is_pinned);
        if pinned {
            self.confirm_unpin(chat_id, vec![message_id], window, cx);
        } else {
            self.open_pin_box(chat_id, message_id, window, cx);
        }
    }

    /// The pin box for one message.
    pub(super) fn open_pin_box(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(kind) = session.chats.get(&chat_id.0).map(|chat| chat.kind.clone()) else {
            return;
        };
        // A message older than the newest pinned one (`IsOldForPin`).
        let pinning_old = session
            .pinned_list(chat_id)
            .first()
            .is_some_and(|top| message_id.0 < top.id.0);
        let choice = pin_choice(&kind, session.is_saved_messages(chat_id), pinning_old);
        let label = match (&kind, choice) {
            (_, PinChoice::NotifyAll) => Some("Notify all members".to_string()),
            (ChatKind::Private { user_id }, PinChoice::AlsoForPeer) => Some(
                session
                    .user(user_id.0)
                    .map(|user| format!("Also pin for {}", user.first_name))
                    .unwrap_or_else(|| "Also pin for the other side".into()),
            ),
            _ => None,
        };
        let question = pin_question(&kind, pinning_old);
        let checked = Rc::new(Cell::new(choice.default_checked()));
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, checked) = (app.clone(), checked.clone());
            let checkbox_state = checked.clone();
            let label = label.clone();
            alert
                .description(question)
                .when_some(label, |alert, label| {
                    alert.content(move |content, _, _| {
                        let state = checkbox_state.clone();
                        content.child(
                            gpui_kit::component::checkbox::Checkbox::new("pin-box-choice")
                                .label(label.clone())
                                .checked(state.get())
                                .on_click(move |on, window, _| {
                                    state.set(*on);
                                    window.refresh();
                                }),
                        )
                    })
                })
                .ok_text("Pin")
                .cancel_text("Cancel")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let (silent, only_for_self) = choice.request_flags(checked.get());
                    let _ = app.update(cx, |this, cx| {
                        this.pin_with_flags(chat_id, message_id, silent, only_for_self, cx);
                    });
                    true
                })
        });
    }

    fn pin_with_flags(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        silent: bool,
        only_for_self: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.status_note =
                match live
                    .driver
                    .pin_chat_message_with(chat_id, message_id, silent, only_for_self)
                {
                    Ok(_) => "pinning…".into(),
                    Err(_) => "could not pin".into(),
                };
        } else if self.demo_session.is_some() {
            self.apply_demo_pin_toggle(chat_id, message_id);
            self.status_note = "pinned".into();
        }
        cx.notify();
    }

    /// Unpin box for one or several messages (`lng_pinned_unpin_sure`,
    /// `lng_pinned_unpin_many_sure`).
    pub(super) fn confirm_unpin(
        &mut self,
        chat_id: ChatId,
        ids: Vec<MessageId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ids.is_empty() {
            return;
        }
        let question = unpin_question(ids.len());
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, ids) = (app.clone(), ids.clone());
            alert
                .description(question.clone())
                .ok_text("Unpin")
                .cancel_text("Cancel")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let ids = ids.clone();
                    let _ = app.update(cx, |this, cx| {
                        for id in ids {
                            this.unpin_from_banner(chat_id, id, cx);
                        }
                        this.pending_forward = None;
                        this.forward_picker_open = false;
                        cx.notify();
                    });
                    true
                })
        });
    }

    /// Whether every selected message is pinned and the user may unpin
    /// (`MessagesToUnpin`).
    pub(super) fn selection_unpinnable(&self, chat_id: ChatId) -> bool {
        let Some(draft) = self.pending_forward.as_ref() else {
            return false;
        };
        let Some(session) = self.session() else {
            return false;
        };
        if draft.from_chat_id != chat_id
            || draft.message_ids.is_empty()
            || !session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| chat.can_pin_messages())
        {
            return false;
        }
        let Some(history) = session.histories.get(&chat_id.0) else {
            return false;
        };
        draft.message_ids.iter().all(|id| {
            history
                .messages
                .get(&id.0)
                .is_some_and(|m| m.is_pinned && m.can_pin())
        })
    }
}
