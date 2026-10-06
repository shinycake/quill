//! Message selection mode, as in Telegram Desktop: once a message is
//! selected ("Select" in its menu), a click anywhere on a row toggles it,
//! each row shows a check circle, and ⌘C copies the selected messages.

use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::telegram::envelope::effective_content;

impl QuillApp {
    /// Whether selection mode is on in `chat_id`.
    pub(super) fn selecting_in(&self, chat_id: ChatId) -> bool {
        self.pending_forward
            .as_ref()
            .is_some_and(|draft| draft.from_chat_id == chat_id && !draft.message_ids.is_empty())
    }

    /// The row overlay while selecting: it takes the row's clicks (links
    /// and media don't fire) and shows the check circle.
    pub(super) fn selection_overlay(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        pending: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.selecting_in(chat_id) {
            return None;
        }
        let selected = self
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.contains(message_id));
        let accent = cx.theme().primary;
        Some(
            div()
                .id(("selection-overlay", message_id.0 as u64))
                .absolute()
                .inset_0()
                .occlude()
                .cursor_pointer()
                .flex()
                .items_end()
                .pl_1()
                .pb_1()
                .child(
                    div()
                        .size(px(20.))
                        .rounded_full()
                        .border_2()
                        .border_color(if selected {
                            accent
                        } else {
                            gpui_kit::white().opacity(0.6)
                        })
                        .when(selected, |this| this.bg(accent))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(selected, |this| {
                            this.child(
                                Icon::new(gpui_kit::assets::IconName::Check)
                                    .size(px(12.))
                                    .text_color(gpui_kit::white()),
                            )
                        }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_forward_select(chat_id, message_id, pending, cx);
                }))
                .into_any_element(),
        )
    }

    /// The selected messages as text, as Telegram Desktop copies them: a
    /// single message as its text; several as "[date time] Name: text"
    /// lines, oldest first.
    pub(super) fn selected_messages_text(&self) -> Option<(ChatId, String)> {
        let draft = self.pending_forward.as_ref()?;
        let session = self.session()?;
        let history = session.histories.get(&draft.from_chat_id.0)?;
        let mut ids = draft.message_ids.clone();
        ids.sort_by_key(|id| id.0);
        let messages: Vec<_> = ids
            .iter()
            .filter_map(|id| history.messages.get(&id.0))
            .collect();
        let body = |message: &quill::state::HistoryMessage| {
            Self::message_copyable_text(effective_content(
                &message.content,
                message.ephemeral.as_ref(),
            ))
            .filter(|text| !text.is_empty())
            // Media without text reads as its kind ("Sticker", "Video"…).
            .unwrap_or_else(|| quill::state::effective_preview(message))
        };
        let text = match messages.as_slice() {
            [] => return None,
            [only] => body(only),
            many => many
                .iter()
                .map(|message| {
                    let date = quill::local_time::civil_local(i64::from(message.date));
                    format!(
                        "[{:02}.{:02}.{:02} {:02}:{:02}] {}: {}",
                        date.day,
                        date.month,
                        date.year.rem_euclid(100),
                        date.hour,
                        date.minute,
                        session.message_author_name(message),
                        body(message)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        };
        (!text.trim().is_empty()).then_some((draft.from_chat_id, text))
    }

    /// "Delete N" for the selection: Telegram Desktop's confirmation, with
    /// "delete for everyone" (checked by default) when every selected
    /// message is yours and the chat isn't Saved Messages.
    pub(super) fn confirm_delete_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.pending_forward.as_ref() else {
            return;
        };
        let (chat_id, ids) = (draft.from_chat_id, draft.message_ids.clone());
        let Some(session) = self.session() else {
            return;
        };
        let all_outgoing = session.histories.get(&chat_id.0).is_some_and(|history| {
            ids.iter().all(|id| {
                history
                    .messages
                    .get(&id.0)
                    .is_some_and(|message| message.is_outgoing)
            })
        });
        let revoke_label = (all_outgoing && !session.is_saved_messages(chat_id)).then(|| {
            match session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
                Some(quill::telegram::envelope::ChatKind::Private { user_id }) => session
                    .user(user_id.0)
                    .map(|user| format!("Also delete for {}", user.first_name))
                    .unwrap_or_else(|| "Delete for everyone".into()),
                _ => "Delete for everyone".into(),
            }
        });
        let question = if ids.len() == 1 {
            "Do you want to delete this message?".to_string()
        } else {
            format!("Do you want to delete {} messages?", ids.len())
        };
        let revoke = std::rc::Rc::new(std::cell::Cell::new(true));
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, ids, revoke) = (app.clone(), ids.clone(), revoke.clone());
            let checkbox_state = revoke.clone();
            let label = revoke_label.clone();
            alert
                .description(question.clone())
                .when_some(label, |alert, label| {
                    alert.content(move |content, _, _| {
                        let state = checkbox_state.clone();
                        content.child(
                            gpui_kit::component::checkbox::Checkbox::new("delete-selection-revoke")
                                .label(label.clone())
                                .checked(state.get())
                                .on_click(move |checked, window, _| {
                                    state.set(*checked);
                                    window.refresh();
                                }),
                        )
                    })
                })
                .ok_text("Delete")
                .cancel_text("Cancel")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let revoke = revoke.get();
                    let ids = ids.clone();
                    let _ = app.update(cx, |this, cx| {
                        this.delete_selection(chat_id, &ids, revoke, cx);
                    });
                    true
                })
        });
    }

    fn delete_selection(
        &mut self,
        chat_id: ChatId,
        ids: &[MessageId],
        revoke: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.delete_selected(chat_id, ids, revoke).is_err() {
                self.status_note = "could not delete messages".into();
            }
        } else if self.demo_session.is_some() {
            for id in ids {
                self.apply_demo_delete(chat_id, *id);
            }
        }
        self.pending_forward = None;
        self.forward_picker_open = false;
        cx.notify();
    }
}
