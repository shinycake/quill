//! Methods moved out of `calls.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::bidi_line::name_line;
use crate::ui::chat_row::chat_avatar;
use crate::ui::pressable::PressableDiv;
use crate::ui::scroll_load::{load_due, loading_row, scrolled_near_end};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::assets::IconName;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Redial from an ended call's own screen (the call window's Redial,
    /// the end card's Call again): the end screen goes and the call is
    /// placed at once. tdesktop's panel redials with no box
    /// (`Panel::_answerHangupRedial` calls `Call::redial`): the press on
    /// the call's own screen is the confirmation. The confirm box would
    /// open in the main window, hidden behind the call window that just
    /// closed, or not drawn under the lock.
    pub(in crate::ui) fn redial(&mut self, user_id: i64, is_video: bool, cx: &mut Context<Self>) {
        self.dismiss_call_summary(cx);
        if self.refuse_offline_call(cx) {
            return;
        }
        self.dial_user(user_id, is_video, cx);
    }

    /// Offline: a call cannot start, and the status line says so.
    fn refuse_offline_call(&mut self, cx: &mut Context<Self>) -> bool {
        let offline = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.is_offline());
        if offline {
            self.set_note(StatusNote::error(tr(
                keys::Q_OPS_YOURE_OFFLINE_CANT_START_A,
            )));
            cx.notify();
        }
        offline
    }

    /// Phase C2i: "Call again" from a history row — honours the
    /// confirm-before-calling preference before dialling.
    pub(in crate::ui) fn call_again(
        &mut self,
        user_id: i64,
        is_video: bool,
        cx: &mut Context<Self>,
    ) {
        if self.refuse_offline_call(cx) {
            return;
        }
        let confirm = self
            .session()
            .is_some_and(|session| session.calls.prefs.confirm_before_calling);
        if confirm {
            self.call_boxes.confirm = Some((user_id, is_video));
            self.open_box(crate::ui::shell::DialogKind::CallConfirm);
            cx.notify();
            return;
        }
        self.dial_user(user_id, is_video, cx);
    }

    /// The next page of call history, once the Calls tab is scrolled near
    /// its end or the loaded calls do not fill it.
    pub(in crate::ui) fn calls_history_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        let calls = &session.calls;
        let loaded = calls.recent_calls.len();
        if !load_due(
            scrolled_near_end(&self.call_boxes.history_scroll),
            !calls.recent_calls_offset.is_empty(),
            calls.recent_calls_loading(),
        ) || calls.recent_calls_error && !scrolled
            || !self.call_boxes.history_asked.allow(loaded, scrolled)
        {
            return;
        }
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_more_call_history()
        {
            self.set_note(StatusNote::error(user_error_for(
                keys::Q_NOTES_FAILED_CALL_HISTORY,
                &err,
            )));
        }
        cx.notify();
    }

    /// Recent calls; preferences live in Settings.
    pub(in crate::ui) fn calls_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().id("calls-list").flex().flex_col().gap_1().px_1();
        let (entries, loading, failed, has_more, clearing) = self
            .session()
            .map(|session| {
                (
                    session.calls.recent_calls.clone(),
                    session.calls.recent_calls_loading(),
                    session.calls.recent_calls_error,
                    !session.calls.recent_calls_offset.is_empty(),
                    session.calls.recent_calls_clearing,
                )
            })
            .unwrap_or_default();
        list = list.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .px_1()
                        .child(tr(keys::Q_OPS_RECENT_CALLS)),
                )
                .when(
                    quill::chatlist_calls::can_clear(entries.len(), clearing),
                    |this| {
                        this.child(
                            Button::new("calls-clear-all")
                                .label(quill::chatlist_calls::CLEAR_ALL_LABEL)
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.open_clear_calls(cx))),
                        )
                    },
                ),
        );
        if failed {
            list = list
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_OPS_COULDN_T_LOAD_CALL_HISTORY)),
                )
                .child(
                    Button::new("calls-retry")
                        .label(tr(keys::Q_OPS_RETRY))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.fetch_call_history()
                            {
                                this.set_note(StatusNote::error(user_error_for(
                                    keys::Q_NOTES_FAILED_CALL_HISTORY,
                                    &err,
                                )));
                            }
                            cx.notify();
                        })),
                );
        } else if loading && entries.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_OPS_LOADING_CALLS)),
            );
        } else if entries.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_OPS_NO_RECENT_CALLS)),
            );
        } else {
            let now = quill::local_time::civil_local(quill::local_time::now_unix());
            let keys: Vec<_> = entries
                .iter()
                .map(|entry| {
                    let day = quill::local_time::civil_local(i64::from(entry.date)).day_number();
                    (entry.chat_id, call_kind(entry), day)
                })
                .collect();
            for run in quill::chatlist_calls::call_runs(&keys) {
                if let Some(entry) = entries.get(run.start) {
                    list = list.child(self.recent_call_row(entry, run.len(), &now, cx));
                }
            }
            if has_more {
                list = list.child(loading_row("calls-loading-more", cx));
            }
        }
        list
    }

    /// Phase C2i: one row of the server-side recent-calls list, with a
    /// reason-aware label (Telegram X `TD.getCallName` style) and
    /// "Call again" for 1:1 calls.
    pub(in crate::ui) fn recent_call_row(
        &self,
        entry: &ParsedMessage,
        count: usize,
        now: &quill::local_time::CivilTime,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (label, is_video) = match &entry.content {
            MessageContent::Call {
                is_video,
                discard_reason,
                duration,
            } => (
                call_entry_label(
                    *is_video,
                    discard_reason,
                    if count > 1 { 0 } else { *duration },
                    entry.is_outgoing,
                ),
                *is_video,
            ),
            MessageContent::GroupCallInvitation { .. } => {
                (tr(keys::Q_OPS_GROUP_CALL).to_owned(), false)
            }
            _ => (tr(keys::PROFILE_ACTION_SHORT_CALL).to_owned(), false),
        };
        let peer = self.recent_call_peer(entry.chat_id);
        let name = peer
            .as_ref()
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| tr(keys::PROFILE_ACTION_SHORT_CALL).to_owned());
        let missed = call_kind(entry) == CallKind::Missed;
        // tdesktop's call row: userpic, name, an arrow for the direction
        // (red when missed) before the date, and a phone or camera button
        // that calls back (`Calls::BoxController::Row`).
        let when = quill::local_time::civil_local(i64::from(entry.date));
        // The full date for screen readers; the row shows the chat list's
        // short stamp beside the name.
        let spoken = quill::chatlist_calls::call_status(&when, now);
        let stamp = quill::local_time::chat_list_stamp(&when, now);
        let status = quill::chatlist_calls::grouped_status(count, label.clone());
        let (arrow, arrow_color) = if missed {
            (IconName::ArrowDownLeft, Hsla::from(danger_soft()))
        } else if entry.is_outgoing {
            (IconName::ArrowUpRight, cx.theme().success)
        } else {
            (IconName::ArrowDownLeft, cx.theme().success)
        };
        let photo = self
            .session()
            .and_then(|s| s.chat_photo_path(entry.chat_id))
            .and_then(|path| {
                quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
            });
        let chat_id = entry.chat_id;
        div()
            .id(("call-row", entry.id.0 as u64))
            .px_2()
            .py_2()
            .rounded_md()
            .flex()
            .items_center()
            .gap_3()
            .cursor_pointer()
            .pressable(cx.theme())
            .role(gpui_kit::Role::Button)
            .aria_label(format!("{name}, {status}, {spoken}"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_listed_chat(chat_id, window, cx);
            }))
            .child(chat_avatar(
                &name,
                photo.as_deref(),
                crate::ui::chat_row::CHAT_ROW_AVATAR,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .min_w_0()
                    .flex_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_semibold()
                                    .overflow_hidden()
                                    .when(missed, |this| this.text_color(Hsla::from(danger_soft())))
                                    .child(name_line(&name)),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(stamp),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(arrow).size(px(14.)).text_color(arrow_color))
                            .child(div().truncate().child(status)),
                    ),
            )
            .when_some(peer, |this, (user_id, _)| {
                let icon = if is_video {
                    IconName::Video
                } else {
                    IconName::Phone
                };
                this.child(
                    Button::new(("call-again", entry.id.0 as u64))
                        .icon(icon)
                        .ghost()
                        .tooltip(tr(keys::Q_OPS_CALL_AGAIN))
                        .accessibility_label(tr(keys::Q_OPS_CALL_AGAIN))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.call_again(user_id, is_video, cx);
                        })),
                )
            })
    }
}

/// tdesktop's three call row kinds (`Calls::BoxController::Row::Type`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum CallKind {
    Out,
    In,
    Missed,
}

fn call_kind(entry: &ParsedMessage) -> CallKind {
    match entry.content {
        _ if entry.is_outgoing => CallKind::Out,
        MessageContent::Call {
            discard_reason: CallDiscardReason::Missed,
            ..
        } => CallKind::Missed,
        _ => CallKind::In,
    }
}
