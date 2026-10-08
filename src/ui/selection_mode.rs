//! Message selection mode, as in Telegram Desktop: once a message is
//! selected ("Select" in its menu), a click anywhere on a row toggles it,
//! each row shows a check circle, and ⌘C copies the selected messages.

use super::app::QuillApp;
use super::motion;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::telegram::envelope::effective_content;

/// A history row's selection-mode decoration (see
/// [`QuillApp::selection_row`]).
#[derive(Default)]
pub(super) struct SelectionRow {
    pub(super) overlay: Option<AnyElement>,
    /// Opacity of the selected-row tint, `0..=1`.
    pub(super) tint: f32,
    /// How far an outgoing bubble slides left, px.
    pub(super) slide: f32,
}

impl QuillApp {
    /// Whether selection mode is on in `chat_id`.
    pub(super) fn selecting_in(&self, chat_id: ChatId) -> bool {
        self.pending_forward
            .as_ref()
            .is_some_and(|draft| draft.from_chat_id == chat_id && !draft.message_ids.is_empty())
    }

    /// Follow selection mode for the motion state; returns the check fade
    /// (`0..=1`) and the top bar slide (`0..=1`). Asks the frame clock for
    /// frames while either moves. An inactive window snaps.
    pub(super) fn selection_motion(&self, chat_id: ChatId, cx: &mut Context<Self>) -> (f32, f32) {
        let now = std::time::Instant::now();
        let on = self.selecting_in(chat_id);
        let mut fx = self.motion.selection.borrow_mut();
        if on && let Some(draft) = self.pending_forward.as_ref() {
            fx.last_count = draft.count();
        }
        fx.sync_mode(on, self.window_active.get(), now);
        if fx.moving(now) {
            self.request_animation_tick(60, cx);
        }
        (fx.mode(now), fx.bar(now))
    }

    /// What a history row shows for selection mode: the fading check
    /// circle (an overlay that also takes the row's clicks while the mode
    /// is on), the selection tint, and how far an outgoing bubble slides
    /// aside (`Message::draw`, `history_view_message.cpp:2313`).
    pub(super) fn selection_row(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        pending: bool,
        cx: &mut Context<Self>,
    ) -> SelectionRow {
        let (mode, _) = self.selection_motion(chat_id, cx);
        let selected = self
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.contains(message_id));
        if mode <= 0. && !selected {
            return SelectionRow::default();
        }
        let now = std::time::Instant::now();
        let checked = self.motion.selection.borrow_mut().check(
            message_id.0,
            selected,
            self.window_active.get(),
            now,
        );
        let interactive = self.selecting_in(chat_id);
        let overlay = (mode > 0.).then(|| {
            let ring = cx.theme().background;
            let (fill, tick) = motion::check_frame(checked);
            let size = motion::CHECK_SIZE;
            // The circle slides in from the right edge while it fades.
            let edge = 11.5 - 15. * (1. - mode);
            div()
                .id(("selection-overlay", message_id.0 as u64))
                .absolute()
                .inset_0()
                .when(interactive, |this| {
                    this.occlude()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_forward_select(chat_id, message_id, pending, cx);
                        }))
                })
                .child(super::anim_layer::occluder(
                    div()
                        .absolute()
                        .right(px(edge))
                        .bottom(px(motion::CHECK_BOTTOM_SKIP + 1.))
                        .size(px(size))
                        .opacity(mode)
                        .rounded_full()
                        .border_2()
                        .border_color(ring)
                        .bg(gpui_kit::black().opacity(0.3))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(fill > 0., |this| {
                            this.child(
                                div()
                                    .size(px((size - 4.) * fill))
                                    .rounded_full()
                                    .bg(cx.theme().success),
                            )
                        })
                        .when(tick > 0., |this| {
                            // The tick wipes in from the left.
                            this.child(
                                div()
                                    .absolute()
                                    .left(px(2.))
                                    .top(px(2.))
                                    .w(px(12. * tick))
                                    .h(px(12.))
                                    .overflow_hidden()
                                    .child(
                                        div().flex_none().size(px(12.)).child(
                                            Icon::new(gpui_kit::assets::IconName::Check)
                                                .size(px(12.))
                                                .text_color(gpui_kit::white()),
                                        ),
                                    ),
                            )
                        }),
                ))
                .into_any_element()
        });
        SelectionRow {
            overlay,
            tint: checked,
            slide: motion::bubble_slide(mode),
        }
    }

    /// The conversation header while selecting: Telegram Desktop swaps the
    /// top bar for "Forward N", "Delete N" and "Cancel", sliding the
    /// buttons down over 150 ms (`toggleSelectedControls`).
    pub(super) fn with_selection_bar(
        &self,
        chat_id: ChatId,
        header: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (_, slide) = self.selection_motion(chat_id, cx);
        if slide <= 0. {
            return header.into_any_element();
        }
        let count = self.motion.selection.borrow().last_count;
        let bar = div()
            .id("selection-bar")
            .absolute()
            .left_0()
            .right_0()
            .h_full()
            .top(relative(-(1. - slide)))
            .occlude()
            .px_4()
            .flex()
            .items_center()
            .gap_2()
            .bg(cx.theme().background)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("selection-forward")
                    .label(format!("Forward {count}"))
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_forward_picker(window, cx);
                    })),
            )
            .child(
                Button::new("selection-delete")
                    .label(format!("Delete {count}"))
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.confirm_delete_selection(window, cx);
                    })),
            )
            .when(self.selection_reportable(chat_id), |bar| {
                bar.child(
                    Button::new("selection-report")
                        .label(format!("Report {count}"))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.report_selection(window, cx);
                        })),
                )
            })
            .child(div().flex_1())
            .child(
                Button::new("selection-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.clear_forward(window, cx);
                    })),
            );
        div()
            .relative()
            .overflow_hidden()
            .child(header)
            .child(bar)
            .into_any_element()
    }

    /// Whether the selection can be reported: other people's sent
    /// messages in a chat that is not Saved Messages (Telegram Desktop's
    /// `suggestReport`).
    pub(super) fn selection_reportable(&self, chat_id: ChatId) -> bool {
        let Some(draft) = self.pending_forward.as_ref() else {
            return false;
        };
        let Some(session) = self.session() else {
            return false;
        };
        if draft.from_chat_id != chat_id || session.is_saved_messages(chat_id) {
            return false;
        }
        let Some(history) = session.histories.get(&chat_id.0) else {
            return false;
        };
        !draft.message_ids.is_empty()
            && draft.message_ids.iter().all(|id| {
                history
                    .messages
                    .get(&id.0)
                    .is_some_and(|m| !m.is_outgoing && !m.pending && m.id.0 > 0)
            })
    }

    /// "Report N": the Report flow for every selected message.
    pub(super) fn report_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.pending_forward.clone() else {
            return;
        };
        let mut ids = draft.message_ids.clone();
        ids.sort_by_key(|id| id.0);
        self.clear_forward(window, cx);
        self.open_message_report(draft.from_chat_id, ids, cx);
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
        self.begin_vanish(chat_id, ids);
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

    /// Telegram Desktop's delete box for one message: "Do you want to delete
    /// this message?" with "Also delete for {name}" (private chats) or
    /// "Delete for everyone", checked by default, when deleting for everyone
    /// is possible.
    pub(super) fn open_delete_dialog(
        &mut self,
        confirm: quill::composer::DeleteConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_delete_dialog_with(confirm, None, window, cx);
    }

    /// The delete box, with the admin checkboxes of a group moderator
    /// (Report Spam, Delete all from the user, Ban the user) when
    /// `moderation` offers them.
    pub(super) fn open_delete_dialog_with(
        &mut self,
        confirm: quill::composer::DeleteConfirm,
        moderation: Option<super::message_menu_ui::ModerationOffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = confirm.can_revoke.then(|| {
            let session = self.session();
            match session
                .and_then(|s| s.chats.get(&confirm.chat_id.0))
                .map(|chat| &chat.kind)
            {
                Some(quill::telegram::envelope::ChatKind::Private { user_id }) => session
                    .and_then(|s| s.user(user_id.0))
                    .map(|user| format!("Also delete for {}", user.first_name))
                    .unwrap_or_else(|| "Delete for everyone".into()),
                _ => "Delete for everyone".into(),
            }
        });
        let revoke = std::rc::Rc::new(std::cell::Cell::new(confirm.revoke));
        // Report Spam / Delete all / Ban: unchecked until the admin ticks them.
        let choice = std::rc::Rc::new(std::cell::Cell::new(quill::connect::ModerationChoice::default()));
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, revoke, confirm) = (app.clone(), revoke.clone(), confirm.clone());
            let checkbox_state = revoke.clone();
            let (moderation_state, moderation_offer) = (choice.clone(), moderation.clone());
            let on_ok_choice = choice.clone();
            let on_ok_offer = moderation.clone();
            alert
                .description("Do you want to delete this message?")
                .when(label.is_some() || moderation_offer.is_some(), |alert| {
                    let label = label.clone();
                    alert.content(move |content, _, _| {
                        let state = checkbox_state.clone();
                        let mut content = content;
                        if let Some(label) = label.clone() {
                            content = content.child(
                                gpui_kit::component::checkbox::Checkbox::new("delete-message-revoke")
                                    .label(label)
                                    .checked(state.get())
                                    .on_click(move |checked, window, _| {
                                        state.set(*checked);
                                        window.refresh();
                                    }),
                            );
                        }
                        if let Some(offer) = moderation_offer.clone() {
                            type Get = fn(quill::connect::ModerationChoice) -> bool;
                            type Set = fn(&mut quill::connect::ModerationChoice, bool);
                            let rows: [(&'static str, String, bool, Get, Set); 3] = [
                                (
                                    "delete-moderate-spam",
                                    "Report Spam".to_string(),
                                    offer.report_spam,
                                    |c| c.report_spam,
                                    |c, v| c.report_spam = v,
                                ),
                                (
                                    "delete-moderate-all",
                                    format!("Delete all from {}", offer.user_name),
                                    offer.delete_all,
                                    |c| c.delete_all,
                                    |c, v| c.delete_all = v,
                                ),
                                (
                                    "delete-moderate-ban",
                                    format!("Ban {}", offer.user_name),
                                    offer.ban,
                                    |c| c.ban,
                                    |c, v| c.ban = v,
                                ),
                            ];
                            for (id, text, available, get, set) in rows {
                                if !available {
                                    continue;
                                }
                                let state = moderation_state.clone();
                                let checked = get(state.get());
                                content = content.child(
                                    gpui_kit::component::checkbox::Checkbox::new(id)
                                        .label(text)
                                        .checked(checked)
                                        .on_click(move |checked, window, _| {
                                            let mut now = state.get();
                                            set(&mut now, *checked);
                                            state.set(now);
                                            window.refresh();
                                        }),
                                );
                            }
                        }
                        content
                    })
                })
                .ok_text("Delete")
                .cancel_text("Cancel")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let mut confirm = confirm.clone();
                    confirm.revoke = confirm.can_revoke && revoke.get();
                    let (picked, offer) = (on_ok_choice.get(), on_ok_offer.clone());
                    let _ = app.update(cx, |this, cx| {
                        // Reports and "delete all" name the user's
                        // messages, so they go out before the delete.
                        if let (true, Some(offer), Some(live)) =
                            (picked.any(), offer, this.live.as_mut())
                        {
                            let _ = live.driver.moderate_message(
                                offer.chat_id,
                                &[confirm.message_id],
                                offer.user_id,
                                picked,
                            );
                        }
                        this.pending_delete = Some(confirm);
                        this.confirm_delete(cx);
                    });
                    true
                })
        });
    }
}
