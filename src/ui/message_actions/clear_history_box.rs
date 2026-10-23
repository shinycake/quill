//! The "Clear history" confirmation. tdesktop `DeleteMessagesBox` with
//! `_wipeHistoryJustClear` asks once, with a "delete for the other side"
//! checkbox where the server allows it; `quill::clear_history` decides what
//! the box shows, this opens it and sends `deleteChatHistory`.

use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::checkbox::Checkbox;
use quill::clear_history::{ClearHistoryPlan, ClearTarget, Revoke, plan};
use quill::lang::{keys, tr_with};
use quill::state::ChatSummary;
use std::cell::Cell;
use std::rc::Rc;

/// The question above the checkbox.
fn question(plan: &ClearHistoryPlan, title: &str) -> String {
    let text = match plan.target {
        ClearTarget::SavedMessages => tr(keys::SURE_DELETE_SAVED_MESSAGES).to_string(),
        ClearTarget::Contact => tr_with(keys::SURE_DELETE_HISTORY, &[("contact", title)]),
        ClearTarget::Group => tr_with(keys::SURE_DELETE_GROUP_HISTORY, &[("group", title)]),
        ClearTarget::Channel => tr_with(keys::SURE_DELETE_CHANNEL_HISTORY, &[("channel", title)]),
    };
    if plan.for_me_note {
        format!("{text}\n\n{}", tr(keys::DELETE_CLEAR_FOR_ME))
    } else {
        text
    }
}

impl QuillApp {
    /// Whether the chat row menu offers "Clear history" for `chat_id`.
    pub(in crate::ui) fn clear_history_plan(&self, chat: &ChatSummary) -> Option<ClearHistoryPlan> {
        let session = self.session()?;
        plan(chat, session.is_saved_messages(chat.id))
    }

    /// Ask, then clear. One box for every chat kind, with the revoke
    /// checkbox where `clear_history_plan` offers it.
    pub(in crate::ui) fn open_clear_history_box(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(chat) = session.chats.get(&chat_id.0) else {
            return;
        };
        let Some(box_plan) = self.clear_history_plan(chat) else {
            return;
        };
        let text = question(&box_plan, &chat.title);
        let checkbox_label = (box_plan.revoke == Revoke::Choose).then(|| match &chat.kind {
            ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => session
                .user(user_id.0)
                .map(|user| {
                    tr_with(
                        keys::DELETE_FOR_OTHER_CHECK,
                        &[("user", user.first_name.as_str())],
                    )
                })
                .unwrap_or_else(|| tr(keys::DELETE_FOR_EVERYONE_CHECK).to_string()),
            _ => tr(keys::DELETE_FOR_EVERYONE_CHECK).to_string(),
        });
        let fixed = match box_plan.revoke {
            Revoke::Fixed(revoke) => revoke,
            Revoke::Choose => false,
        };
        let revoke = Rc::new(Cell::new(fixed));
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, revoke) = (app.clone(), revoke.clone());
            let checkbox_state = revoke.clone();
            alert
                .description(text.clone())
                .when_some(checkbox_label.clone(), |alert, label| {
                    alert.content(move |content, _, _| {
                        let state = checkbox_state.clone();
                        content.child(
                            Checkbox::new("clear-history-revoke")
                                .label(label.clone())
                                .checked(state.get())
                                .on_click(move |checked, window, _| {
                                    state.set(*checked);
                                    window.refresh();
                                }),
                        )
                    })
                })
                .ok_text(tr(keys::BOX_DELETE))
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let revoke = revoke.get();
                    let _ = app.update(cx, |this, cx| this.clear_history_now(chat_id, revoke, cx));
                    true
                })
        });
    }

    /// `deleteChatHistory` keeping the chat in the list (`revoke` clears it
    /// for everyone). TDLib answers asynchronously; a refusal surfaces
    /// through `Session::chat_action_error`.
    fn clear_history_now(&mut self, chat_id: ChatId, revoke: bool, cx: &mut Context<Self>) {
        let note = match self.live.as_mut() {
            Some(live) => match live.driver.clear_chat_history(chat_id, revoke) {
                Ok(Some(_)) => tr(keys::Q_PEOPLE_CLEARING_HISTORY).to_string(),
                Ok(None) => tr(keys::Q_PEOPLE_REQUEST_ALREADY_IN_FLIGHT).to_string(),
                Err(_) => tr(keys::Q_PEOPLE_ACTION_FAILED).to_string(),
            },
            None => tr(keys::Q_PEOPLE_CHAT_ACTIONS_NEED_A_LIVE).to_string(),
        };
        self.connection.set_note(StatusNote::info(note));
        cx.notify();
    }
}
