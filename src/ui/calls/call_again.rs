//! Methods moved out of `calls.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::scroll_load::{load_due, loading_row, scrolled_near_end};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Phase C2i: "Call again" from a history row — honours the
    /// confirm-before-calling preference before dialling.
    pub(in crate::ui) fn call_again(
        &mut self,
        user_id: i64,
        is_video: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.is_offline())
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_OPS_YOURE_OFFLINE_CANT_START_A,
            )));
            cx.notify();
            return;
        }
        let confirm = self
            .session()
            .is_some_and(|session| session.calls.prefs.confirm_before_calling);
        if confirm {
            self.calls.confirm = Some((user_id, is_video));
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
            scrolled_near_end(&self.calls.history_scroll),
            !calls.recent_calls_offset.is_empty(),
            calls.recent_calls_loading,
        ) || calls.recent_calls_error && !scrolled
            || !self.calls.history_asked.allow(loaded, scrolled)
        {
            return;
        }
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_more_call_history()
        {
            self.connection.set_note(StatusNote::error(user_error_for(
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
                    session.calls.recent_calls_loading,
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
                                this.connection.set_note(StatusNote::error(user_error_for(
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
            for entry in &entries {
                list = list.child(self.recent_call_row(entry, cx));
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
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (label, is_video) = match &entry.content {
            MessageContent::Call {
                is_video,
                discard_reason,
                duration,
            } => (
                call_entry_label(*is_video, discard_reason, *duration, entry.is_outgoing),
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
        let missed = !entry.is_outgoing
            && matches!(
                entry.content,
                MessageContent::Call {
                    discard_reason: CallDiscardReason::Missed,
                    ..
                }
            );
        div()
            .id(("call-row", entry.id.0 as u64))
            .px_2()
            .py_2()
            .rounded_md()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .flex_1()
                    .child(div().font_medium().text_sm().child(name))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(label),
                    ),
            )
            .child(if missed {
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(danger_soft())
                    .child(tr(keys::Q_LEFTOVERS_MISSED))
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if let Some((user_id, _)) = peer {
                Button::new(("call-again", entry.id.0 as u64))
                    .label(tr(keys::Q_OPS_CALL_AGAIN))
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.call_again(user_id, is_video, cx);
                    }))
                    .into_any_element()
            } else {
                div().into_any_element()
            })
    }
}
