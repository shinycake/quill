//! Methods moved out of `calls.rs` to keep files under 1000 lines.

use super::*;

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
            self.connection.status_note = "You're offline — can't start a call".into();
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
                .child(div().text_sm().font_semibold().px_1().child("Recent calls"))
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
                        .child("Couldn’t load call history."),
                )
                .child(
                    Button::new("calls-retry")
                        .label("Retry")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.fetch_call_history()
                            {
                                this.connection.status_note =
                                    format!("call history request failed: {err:?}");
                            }
                            cx.notify();
                        })),
                );
        } else if loading && entries.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading calls…"),
            );
        } else if entries.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No recent calls."),
            );
        } else {
            for entry in &entries {
                list = list.child(self.recent_call_row(entry, cx));
            }
            if has_more {
                list = list.child(
                    Button::new("calls-load-more")
                        .label("Load more")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.fetch_more_call_history()
                            {
                                this.connection.status_note =
                                    format!("call history request failed: {err:?}");
                            }
                            cx.notify();
                        })),
                );
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
            MessageContent::GroupCallInvitation { .. } => ("Group call".to_owned(), false),
            _ => ("Call".to_owned(), false),
        };
        let peer = self.recent_call_peer(entry.chat_id);
        let name = peer
            .as_ref()
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "Call".to_owned());
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
                    .child("missed")
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if let Some((user_id, _)) = peer {
                Button::new(("call-again", entry.id.0 as u64))
                    .label("Call again")
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
