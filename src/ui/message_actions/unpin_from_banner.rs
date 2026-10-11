//! Methods moved out of `message_actions.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn unpin_from_banner(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .unpin_chat_message(chat_id, message_id);
            self.connection.status_note = match result {
                Ok(_) => "unpinning…".into(),
                Err(_) => "could not unpin".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            let json = format!(
                r#"{{"@type":"updateMessageIsPinned","chat_id":{},"message_id":{},"is_pinned":false}}"#,
                chat_id.0, message_id.0
            );
            if let Some(session) = self.demo_session.as_mut() {
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
                if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            self.connection.status_note = "unpinned".into();
            cx.notify();
        }
    }

    /// Telegram Desktop's pinned bar: the pinned message at the bar's
    /// position, titled "Pinned message", "Previous message" or
    /// "Pinned message #N"; a segment per pinned message on the left. A
    /// click jumps to it and steps to the next older one. The right
    /// button unpins (or hides) a lone pin, or lists several.
    pub(in crate::ui) fn pinned_message_banner(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let list = session.pinned_list(chat_id);
        let newest = list.first()?.id;
        if self.history.hidden_pinned.get(&chat_id.0) == Some(&newest) {
            return None;
        }
        let count = list.len();
        let index = self
            .history
            .pinned_cursor
            .get(&chat_id.0)
            .copied()
            .unwrap_or(0)
            .min(count - 1);
        let message_id = list[index].id;
        let preview = effective_preview(list[index]);
        let title = if index == 0 {
            "Pinned message".to_string()
        } else if count == 2 {
            "Previous message".to_string()
        } else {
            format!("Pinned message #{}", count - index)
        };
        let can_pin = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_pin_messages());
        // Up to four segments, oldest on top; the window follows the
        // shown message.
        let shown = count.min(4);
        let position = count - 1 - index;
        let start = position.saturating_sub(shown - 1).min(count - shown);
        let segments = (0..shown).map(|segment| {
            div()
                .flex_1()
                .w(px(2.))
                .rounded_full()
                .bg(if start + segment == position {
                    accent()
                } else {
                    accent().opacity(0.35)
                })
        });
        let right = if count == 1 {
            Button::new("pinned-bar-close")
                .icon(gpui_kit::assets::IconName::X)
                .ghost()
                .small()
                .tooltip(if can_pin { "Unpin" } else { "Hide" })
                .accessibility_label(if can_pin {
                    "Unpin message"
                } else {
                    "Hide pinned message"
                })
                .on_click(cx.listener(move |_, _, window, cx| {
                    let app = cx.entity().downgrade();
                    if can_pin {
                        confirm(
                            window,
                            cx,
                            "Would you like to unpin this message?",
                            "Unpin",
                            move |cx| {
                                let _ = app.update(cx, |this, cx| {
                                    this.unpin_from_banner(chat_id, message_id, cx);
                                });
                            },
                        );
                    } else {
                        confirm_hide_pinned(window, cx, app, chat_id, newest);
                    }
                }))
        } else {
            Button::new("pinned-bar-list")
                .icon(gpui_kit::assets::IconName::List)
                .ghost()
                .small()
                .selected(self.history.pinned_list_open)
                .tooltip("Pinned messages")
                .accessibility_label("Pinned messages")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.history.pinned_list_open = !this.history.pinned_list_open;
                    cx.notify();
                }))
        };
        Some(
            div()
                .id("pinned-message-bar")
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1p5()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(bg_canvas())
                .child(
                    div()
                        .id("pinned-message-jump")
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .flex_1()
                        .role(gpui_kit::Role::Button)
                        .aria_label("Go to pinned message")
                        .tab_index(0)
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.jump_to_pinned_message(message_id, cx);
                            this.history
                                .pinned_cursor
                                .insert(chat_id.0, (index + 1) % count);
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .h(px(34.))
                                .children(segments),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(accent())
                                        .child(title),
                                )
                                .child(
                                    div().text_sm().truncate().text_color(text_primary()).child(
                                        super::bidi_line::one_line_plain(
                                            super::search_ui::one_line_preview(&preview),
                                        ),
                                    ),
                                ),
                        ),
                )
                .child(right)
                .into_any_element(),
        )
    }

    /// The pinned bar's list (Telegram Desktop's pinned-messages section,
    /// as a panel): every pinned message, newest first; a click jumps
    /// there. The footer unpins all, or for readers hides the bar.
    pub(in crate::ui) fn pinned_list_panel(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let list = session.pinned_list(chat_id);
        let newest = list.first()?.id;
        let count = list.len();
        let can_pin = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_pin_messages());
        let hover = cx.theme().accent;
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        let rows = list.iter().enumerate().map(|(index, message)| {
            let message_id = message.id;
            div()
                .id(("pinned-list-row", message_id.0 as u64))
                .flex()
                .flex_col()
                .px_3()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .hover(|style| style.bg(hover))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(quill::local_time::day_label(
                            &quill::local_time::civil_local(i64::from(message.date)),
                            &now,
                        )),
                )
                .child(div().text_sm().truncate().text_color(text_primary()).child(
                    super::bidi_line::one_line_plain(super::search_ui::one_line_preview(
                        &effective_preview(message),
                    )),
                ))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.jump_to_pinned_message(message_id, cx);
                    this.history.pinned_cursor.insert(chat_id.0, index);
                    this.history.pinned_list_open = false;
                    cx.notify();
                }))
                // Telegram Desktop's "Go To Message" on a pinned row.
                .context_menu({
                    let owner = cx.entity().downgrade();
                    move |menu, _, _| {
                        let owner = owner.clone();
                        menu.item(
                            gpui_kit::component::menu::PopupMenuItem::new("Go To Message")
                                .icon(gpui_kit::assets::IconName::MessageSquare)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        this.jump_to_pinned_message(message_id, cx);
                                        this.history.pinned_cursor.insert(chat_id.0, index);
                                        this.history.pinned_list_open = false;
                                        cx.notify();
                                    });
                                }),
                        )
                    }
                })
        });
        let noun = if count == 1 { "message" } else { "messages" };
        let footer = if can_pin {
            Button::new("pinned-list-unpin-all")
                .label(if count == 1 {
                    "Unpin 1 message".to_string()
                } else {
                    format!("Unpin all {count} messages")
                })
                .ghost()
                .on_click(cx.listener(move |_, _, window, cx| {
                    let app = cx.entity().downgrade();
                    confirm(
                        window,
                        cx,
                        quill::selection_pin::UNPIN_ALL_QUESTION,
                        "Unpin",
                        move |cx| {
                            let _ = app.update(cx, |this, cx| {
                                this.history.pinned_list_open = false;
                                this.unpin_all_messages(chat_id, cx);
                            });
                        },
                    );
                }))
        } else {
            Button::new("pinned-list-hide")
                .label("Don't show pinned messages")
                .ghost()
                .on_click(cx.listener(move |_, _, window, cx| {
                    let app = cx.entity().downgrade();
                    confirm_hide_pinned(window, cx, app, chat_id, newest);
                }))
        };
        Some(
            div()
                .id("pinned-list-panel")
                .flex()
                .flex_col()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(bg_canvas())
                .child(
                    div()
                        .px_3()
                        .pt_2()
                        .text_xs()
                        .font_medium()
                        .text_color(accent())
                        .child(format!("{count} pinned {noun}")),
                )
                .child(
                    div()
                        .id("pinned-list-rows")
                        .flex()
                        .flex_col()
                        .px_1()
                        .py_1()
                        .max_h(px(280.))
                        .overflow_y_scroll()
                        .children(rows),
                )
                .child(div().flex().justify_center().pb_1().child(footer))
                .into_any_element(),
        )
    }

    pub(in crate::ui) fn jump_to_replied_message(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.jump_to_replied_message(message_id) {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.connection.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }
}
