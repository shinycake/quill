//! The boost box (`parity:deeplink-boost`, see `quill::boost_box`).
//!
//! Opened by a boost link and by the info panel's Boost button. It shows
//! the level, the boosts missing for the next level and what that level
//! unlocks. The one button boosts, or opens the box tdesktop shows when no
//! slot is free. Only a click on that button reads the slots, and only a
//! click on Reassign or Replace moves a boost from another chat.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::link_info_boxes::finish;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_base::{Disableable, StyledExt};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::boost_box::{
    BoostBoxState, BoostCounters, BoostPhase, reassign_text, replace_question, row_status,
};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::{RequestPurpose, Session};

fn chat_title(session: &Session, chat_id: i64) -> String {
    session
        .chats
        .get(&chat_id)
        .map(|chat| chat.title.clone())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| tr(keys::Q_MONEY_ANOTHER_CHAT).into())
}

impl QuillApp {
    fn boost_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    /// Open the box for a channel or group. Reads the level; boosts nothing.
    pub(super) fn open_boost_box_for(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => match live.driver.open_boost_box(chat_id) {
                Ok(true) => {}
                Ok(false) => self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MONEY_THIS_CHAT_CANT_BE_BOOSTED,
                ))),
                Err(_) => self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MONEY_COULD_NOT_LOAD_BOOST_STATUS,
                ))),
            },
            None => self.connection.set_note(StatusNote::info(tr(
                keys::Q_MONEY_BOOSTS_NEED_A_LIVE_CONNECTION,
            ))),
        }
        cx.notify();
    }

    fn close_boost_box_state(&mut self, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => live.driver.close_boost_box(),
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.groups.boost_box = None;
                }
            }
        }
        cx.notify();
    }

    /// The Boost button.
    fn boost_box_press(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.request_chat_boost(chat_id).is_err()
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MONEY_COULD_NOT_BOOST)));
        }
        cx.notify();
    }

    fn boost_box_retry(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.open_boost_box_for(chat_id, cx);
    }

    fn boost_box_toggle(&mut self, slot_id: i32, cx: &mut Context<Self>) {
        let now = quill::local_time::now_unix();
        if let Some(session) = self.boost_session_mut() {
            let receive = session.groups.boosts_per_gift.unwrap_or(0);
            if let Some(state) = session.groups.boost_box.as_mut() {
                state.toggle_slot(slot_id, now, receive);
            }
        }
        cx.notify();
    }

    fn boost_box_cancel_reassign(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_boost_reassign();
        } else if let Some(state) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.groups.boost_box.as_mut())
        {
            state.phase = BoostPhase::Main;
        }
        cx.notify();
    }

    /// Reassign or Replace.
    fn boost_box_reassign(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.reassign_boosts().is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_REASSIGN_THE_BOOST,
            )));
        }
        cx.notify();
    }

    /// The Premium needed box's "Yes": the Premium page, telling the bot
    /// which channel it is for (tdesktop `"channel_boost__" + id`).
    fn boost_box_open_premium(&mut self, cx: &mut Context<Self>) {
        let start_ref = self
            .session()
            .and_then(|s| {
                let chat_id = s.groups.boost_box.as_ref()?.chat_id;
                match s.chats.get(&chat_id)?.kind {
                    quill::telegram::envelope::ChatKind::Supergroup { supergroup_id, .. } => {
                        Some(format!("channel_boost__{supergroup_id}"))
                    }
                    _ => None,
                }
            })
            .unwrap_or_default();
        self.close_boost_box_state(cx);
        self.open_premium_from(&start_ref, cx);
    }

    pub(super) fn boost_progress(counters: &BoostCounters, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let next = if counters.next_level_boosts > 0 {
            tr_with(
                keys::Q_MONEY_LEVEL_4,
                &[("level_1", &(counters.level + 1).to_string())],
            )
        } else {
            tr(keys::Q_MONEY_MAXIMUM_LEVEL).to_string()
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr_with(
                        keys::Q_MONEY_LEVEL_2,
                        &[("level", &counters.level.to_string())],
                    ))
                    .child(next),
            )
            .child(
                div()
                    .h(px(8.))
                    .w_full()
                    .rounded_full()
                    .bg(theme.muted)
                    .child(
                        div()
                            .h_full()
                            .w(relative(counters.fraction()))
                            .rounded_full()
                            .bg(theme.primary),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_sm()
                    .child(
                        div()
                            .text_color(text_muted())
                            .child(tr(keys::Q_MONEY_EXISTING_BOOSTS)),
                    )
                    .child(counters.boosts.to_string()),
            )
            .into_any_element()
    }

    /// What the next level unlocks, from the level table.
    fn boost_features(session: &Session, state: &BoostBoxState, target: i32) -> Option<AnyElement> {
        Self::boost_features_for(session, state.chat_id, state.group, target)
    }

    /// What each level from `target` up unlocks for `chat_id`.
    pub(super) fn boost_features_for(
        session: &Session,
        chat_id: i64,
        group: bool,
        target: i32,
    ) -> Option<AnyElement> {
        let features = session.groups.chat_boost_features.get(&chat_id)?;
        let rows: Vec<_> = features
            .table(group)
            .into_iter()
            .filter(|(level, _)| *level >= target)
            .collect();
        if rows.is_empty() {
            return None;
        }
        let mut list = div().flex().flex_col().gap_1().child(
            div()
                .pt_2()
                .text_xs()
                .font_semibold()
                .text_color(text_muted())
                .child(tr(keys::Q_MONEY_WHAT_EACH_LEVEL_UNLOCKS)),
        );
        for (level, lines) in rows {
            list = list.child(
                div()
                    .flex()
                    .gap_2()
                    .text_sm()
                    .child(div().w(px(56.)).text_color(text_muted()).child(tr_with(
                        keys::Q_MONEY_LEVEL_2,
                        &[("level", &level.to_string())],
                    )))
                    .child(div().flex_1().child(lines.join(", "))),
            );
        }
        Some(list.into_any_element())
    }

    fn boost_main_body(
        &self,
        session: &Session,
        state: &BoostBoxState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat_id = ChatId(state.chat_id);
        let name = chat_title(session, state.chat_id);
        let mut body = div().flex().flex_col().gap_3();
        match session.groups.chat_boost_details.get(&state.chat_id) {
            Some(status) => {
                let counters = BoostCounters::from(status);
                let mut title = counters.title(&name, state.group);
                if let Some(times) = counters.repeated() {
                    title = format!("{title} {times}");
                }
                body = body
                    .child(
                        div()
                            .id("boost-box-title")
                            .text_lg()
                            .font_semibold()
                            .child(title),
                    )
                    .child(
                        div()
                            .id("boost-box-text")
                            .text_sm()
                            .text_color(text_muted())
                            .child(counters.text(&name, state.group)),
                    )
                    .child(Self::boost_progress(&counters, cx));
                if let Some(features) =
                    Self::boost_features(session, state, counters.target_level())
                {
                    body = body.child(features);
                }
            }
            None if session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetChatBoostStatus, chat_id) =>
            {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_MONEY_LOADING_BOOST_STATUS)),
                );
            }
            None => {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(text_muted())
                                .child(tr(keys::Q_MONEY_BOOST_STATUS_UNAVAILABLE)),
                        )
                        .child(
                            Button::new("boost-box-retry")
                                .label(tr(keys::Q_MONEY_RETRY))
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.boost_box_retry(chat_id, cx)
                                })),
                        ),
                );
            }
        }
        if let Some(note) = state.note.clone() {
            body = body.child(div().id("boost-box-note").text_sm().child(note));
        }
        body.into_any_element()
    }

    fn boost_reassign_body(
        &self,
        session: &Session,
        state: &BoostBoxState,
        slots: &[quill::telegram::envelope::ParsedBoostSlot],
        selected: &[i32],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = quill::local_time::now_unix();
        let name = chat_title(session, state.chat_id);
        if let [only] = slots
            && !quill::boost_box::is_waiting(only, now)
        {
            return div()
                .id("boost-box-replace")
                .text_sm()
                .child(replace_question(&chat_title(session, only.chat_id), &name))
                .into_any_element();
        }
        let mut list = div().flex().flex_col().gap_2().child(
            div()
                .id("boost-box-reassign-text")
                .text_sm()
                .text_color(text_muted())
                .child(reassign_text(
                    &name,
                    session.groups.boosts_per_gift.unwrap_or(0),
                )),
        );
        for slot in slots {
            let slot_id = slot.slot_id;
            let waiting = quill::boost_box::is_waiting(slot, now);
            let ticked = selected.contains(&slot_id);
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .when(waiting, |row| row.opacity(0.5))
                    .child(
                        Checkbox::new(("boost-slot", slot_id as u64))
                            .checked(ticked)
                            .label(chat_title(session, slot.chat_id))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.boost_box_toggle(slot_id, cx);
                            })),
                    )
                    .child(
                        div()
                            .pl_6()
                            .text_xs()
                            .text_color(text_muted())
                            .child(row_status(slot, now)),
                    ),
            );
        }
        if let Some(note) = state.note.clone() {
            list = list.child(div().id("boost-box-note").text_sm().child(note));
        }
        list.into_any_element()
    }

    pub(super) fn build_boost_box_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::BoostBox, |this, _, cx| {
                this.close_boost_box_state(cx);
            });
        app.update(cx, |this, cx| {
            let Some(session) = this.session() else {
                return dialog.overlay(true).on_close(on_close.clone());
            };
            let Some(state) = session.groups.boost_box.clone() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(tr(keys::VIEW_BUTTON_BOOST)))
                    .on_close(on_close.clone());
            };
            let chat_id = ChatId(state.chat_id);
            let close_button = |label: &'static str| {
                Button::new("boost-box-close")
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_boost_box_state(cx);
                        this.close_kit_dialog_if_done(DialogKind::BoostBox, window, cx);
                    }))
            };
            let mut footer = div().flex().justify_end().gap_2();
            let (title, body) = match &state.phase {
                BoostPhase::Main | BoostPhase::Working => {
                    let counters = session
                        .groups
                        .chat_boost_details
                        .get(&state.chat_id)
                        .map(BoostCounters::from);
                    let working = matches!(state.phase, BoostPhase::Working);
                    footer = footer.child(close_button(tr(keys::CLOSE)));
                    if let Some(counters) = counters {
                        let allow_multi = session.groups.boosts_per_gift.unwrap_or(0) > 0;
                        let button = counters.button(state.group, allow_multi);
                        footer = footer.child(
                            Button::new("boost-box-boost")
                                .label(button.label)
                                .primary()
                                .loading(working)
                                .disabled(working)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if button.closes {
                                        this.close_boost_box_state(cx);
                                        this.close_kit_dialog_if_done(
                                            DialogKind::BoostBox,
                                            window,
                                            cx,
                                        );
                                    } else {
                                        this.boost_box_press(chat_id, cx);
                                    }
                                })),
                        );
                    }
                    let title = if state.group {
                        tr(keys::BOOST_GROUP_BUTTON)
                    } else {
                        tr(keys::BOOST_CHANNEL_BUTTON)
                    };
                    (title, this.boost_main_body(session, &state, cx))
                }
                BoostPhase::Reassign { slots, selected } => {
                    let now = quill::local_time::now_unix();
                    let single = matches!(slots.as_slice(), [only] if !quill::boost_box::is_waiting(only, now));
                    let can_move = !state.reassign_selection(now).is_empty();
                    footer = footer
                        .child(
                            Button::new("boost-box-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.boost_box_cancel_reassign(cx);
                                })),
                        )
                        .child(
                            Button::new("boost-box-reassign")
                                .label(if single { tr(keys::BOOST_NOW_REPLACE) } else { tr(keys::BOOST_REASSIGN_BUTTON) })
                                .primary()
                                .disabled(!can_move)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.boost_box_reassign(cx);
                                })),
                        );
                    (
                        if single {
                            tr(keys::Q_MONEY_REPLACE_BOOST)
                        } else {
                            tr(keys::BOOST_REASSIGN_TITLE)
                        },
                        this.boost_reassign_body(session, &state, slots, selected, cx),
                    )
                }
                BoostPhase::Notice(notice) => {
                    let notice = *notice;
                    if notice.offers_premium() {
                        footer = footer.child(close_button(tr(keys::CANCEL))).child(
                            Button::new("boost-box-premium")
                                .label(tr(keys::BOX_YES))
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.boost_box_open_premium(cx);
                                    this.close_kit_dialog_if_done(DialogKind::BoostBox, window, cx);
                                })),
                        );
                    } else {
                        footer = footer.child(close_button("OK"));
                    }
                    (
                        notice.title(),
                        div()
                            .id("boost-box-notice")
                            .text_sm()
                            .child(notice.text(&chat_title(session, state.chat_id), state.group))
                            .into_any_element(),
                    )
                }
            };
            finish(dialog, title, body, footer.into_any_element(), on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// Boost link and the info panel's Boost button: level, features, the
    /// one button, and the reassign list.
    BoostBox => DialogSpec::new(
        2950,
        |app| app.session().is_some_and(|s| s.groups.boost_box.is_some()),
        QuillApp::build_boost_box_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "boost_box_ui_tests.rs"]
mod tests;
