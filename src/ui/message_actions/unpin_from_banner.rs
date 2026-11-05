//! Methods moved out of `message_actions.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::lean_buttons::ghost_text_button;
use crate::ui::status_note::StatusNote;
use crate::ui::surface_reveal::{self, PinnedShown};
use quill::lang::{keys, tr, tr_with, trn};

/// The pinned bar's height until it was laid out once.
const PINNED_BAR_GUESS: Pixels = px(48.);
/// `st::msgReplyBarSkip`: how far the swapped text slides.
const PINNED_SWAP_SHIFT: f32 = 10.;

/// The bar's title over its one-line preview.
fn pinned_bar_lines_block(title: String, preview: String) -> Div {
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
            div()
                .text_sm()
                .truncate()
                .text_color(text_primary())
                .child(crate::ui::bidi_line::one_line_plain(
                    crate::ui::search_ui::one_line_preview(&preview),
                )),
        )
}

impl QuillApp {
    pub(in crate::ui) fn unpin_from_banner(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.unpin_chat_message(chat_id, message_id);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_CHAT_UNPINNING)),
                Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_UNPIN)),
            });
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
        // The bar slides open and shut (`Ui::SlideWrap` in tdesktop's
        // `PinnedBar`, `slideWrapDuration` 150 ms) and keeps drawing the
        // last pinned message while it slides shut (`surface_reveal`).
        let state = self.pinned_bar_state(chat_id);
        let now = surface_reveal::now();
        let animate = surface_reveal::animate(cx);
        let (drawn, moving) = {
            let mut presence = self.frame.surfaces.pinned_bar.borrow_mut();
            let drawn = presence.follow(state, chat_id.0 as u64, animate, now);
            (drawn, presence.moving(now))
        };
        if moving {
            self.request_animation_tick(60, cx);
        }
        let drawn = drawn?;
        let content = self.pinned_message_banner_body(chat_id, &drawn, cx)?;
        Some(surface_reveal::slide_open(
            content,
            drawn.shown,
            self.frame.surfaces.pinned_bar_size.height.clone(),
            PINNED_BAR_GUESS,
            px(0.),
            surface_reveal::Attached::Top,
        ))
    }

    /// Which pinned message the bar shows, `None` while it shows none:
    /// nothing pinned, or the newest pin hidden.
    fn pinned_bar_state(&self, chat_id: ChatId) -> Option<PinnedShown> {
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
            .min(count.saturating_sub(1));
        let item = list.get(index)?;
        let title = if index == 0 {
            tr(keys::PINNED_MESSAGE).to_string()
        } else if count == 2 {
            tr(keys::Q_CHAT_PREVIOUS_MESSAGE).to_string()
        } else {
            tr_with(
                keys::Q_CHAT_PINNED_MESSAGE,
                &[("count_index", &count.saturating_sub(index).to_string())],
            )
        };
        Some(PinnedShown {
            chat: chat_id.0,
            message: item.id.0,
            index,
            count,
            title,
            preview: effective_preview(item),
        })
    }

    /// `pinned_message_banner` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    #[inline(never)]
    fn pinned_message_banner_body(
        &self,
        chat_id: ChatId,
        drawn: &surface_reveal::Drawn<PinnedShown>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let shown = &drawn.value;
        let list = session.pinned_list(chat_id);
        // While the bar slides shut after an unpin the list may be empty.
        let newest = list.first().map_or(MessageId(shown.message), |item| item.id);
        let count = shown.count;
        let index = shown.index;
        let message_id = MessageId(shown.message);
        let (title, preview) = (shown.title.clone(), shown.preview.clone());
        let can_pin = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_pin_messages());
        // Up to four segments, oldest on top; the window follows the
        // shown message.
        let shown_segments = count.min(4);
        let position = count.saturating_sub(1).saturating_sub(index);
        let start = position
            .saturating_sub(shown_segments.saturating_sub(1))
            .min(count.saturating_sub(shown_segments));
        let segments = (0..shown_segments).map(|segment| {
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
            pinned_bar_close_button(can_pin, chat_id, message_id, newest, cx)
        } else {
            pinned_bar_list_button(self.history.pinned_list_open, cx)
        };
        // Stepping to another pinned message swaps the text the way
        // tdesktop's `MessageBar` does: the old lines slide out towards the
        // message that was shown, the new ones slide in from the other
        // side, cross-fading over `defaultMessageBar.duration` (160 ms).
        let previous = drawn
            .previous
            .as_ref()
            .filter(|_| drawn.swap < 1.)
            .map(|previous| {
                let lines = (previous.title.clone(), previous.preview.clone());
                (lines, previous.index < index)
            });
        let text = match previous {
            None => pinned_bar_lines_block(title, preview).into_any_element(),
            Some(((old_title, old_preview), up)) => {
                let swap = drawn.swap;
                let sign = if up { -1. } else { 1. };
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h(px(34.))
                    .child(
                        pinned_bar_lines_block(old_title, old_preview)
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px(sign * PINNED_SWAP_SHIFT * swap))
                            .opacity(1. - swap),
                    )
                    .child(
                        pinned_bar_lines_block(title, preview)
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px(-sign * PINNED_SWAP_SHIFT * (1. - swap)))
                            .opacity(swap),
                    )
                    .into_any_element()
            }
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
                        .aria_label(tr(keys::Q_CHAT_GO_TO_PINNED_MESSAGE))
                        .tab_index(0)
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.jump_to_pinned_message(message_id, cx);
                            this.history
                                .pinned_cursor
                                .insert(chat_id.0, (index + 1) % count.max(1));
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
                        .child(text),
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
            let row = div()
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
                }));
            // Telegram Desktop's "Go To Message" on a pinned row.
            if !quill::go_to_message::go_to_message_offered(
                quill::go_to_message::MessageListContext::Pinned,
                message_id,
                message.pending,
            ) {
                return row.into_any_element();
            }
            let owner = cx.entity().downgrade();
            row.context_menu(move |menu, _, _| {
                let owner = owner.clone();
                menu.item(
                    gpui_kit::component::menu::PopupMenuItem::new(tr(keys::Q_CHAT_GO_TO_MESSAGE))
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
            })
            .into_any_element()
        });
        let noun = if count == 1 { "message" } else { "messages" };
        let footer = if can_pin {
            ghost_text_button(
                "pinned-list-unpin-all",
                trn(keys::PINNED_UNPIN_ALL, count),
                cx.listener(move |_, _, window, cx| {
                    let app = cx.entity().downgrade();
                    confirm(
                        window,
                        cx,
                        quill::selection_pin::UNPIN_ALL_QUESTION,
                        tr(keys::CONTEXT_UNPIN_FROM_TOP),
                        move |cx| {
                            let _ = app.update(cx, |this, cx| {
                                this.history.pinned_list_open = false;
                                this.unpin_all_messages(chat_id, cx);
                            });
                        },
                    );
                }),
            )
        } else {
            ghost_text_button(
                "pinned-list-hide",
                tr(keys::PINNED_HIDE_ALL),
                cx.listener(move |_, _, window, cx| {
                    let app = cx.entity().downgrade();
                    confirm_hide_pinned(window, cx, app, chat_id, newest);
                }),
            )
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
            self.connection
                .set_note(match live.driver.jump_to_replied_message(message_id) {
                    Ok(_) => chat_search_jump_note(&live.driver.session),
                    Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_JUMP_TO_MESSAGE)),
                });
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.connection.set_note(chat_search_jump_note(session));
        }
        cx.notify();
    }
}

/// The pinned bar's close button for a lone pin: unpins it, or for readers
/// hides the bar. Built in a frame of its own, like `lean_buttons`.
fn pinned_bar_close_button(
    can_pin: bool,
    chat_id: ChatId,
    message_id: MessageId,
    newest: MessageId,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    Button::new("pinned-bar-close")
        .icon(gpui_kit::assets::IconName::X)
        .ghost()
        .small()
        .tooltip(if can_pin {
            tr(keys::CONTEXT_UNPIN_FROM_TOP)
        } else {
            tr(keys::Q_CHAT_HIDE)
        })
        .accessibility_label(if can_pin {
            tr(keys::Q_CHAT_UNPIN_MESSAGE)
        } else {
            tr(keys::Q_CHAT_HIDE_PINNED_MESSAGE)
        })
        .on_click(cx.listener(move |_, _, window, cx| {
            let app = cx.entity().downgrade();
            if can_pin {
                confirm(
                    window,
                    cx,
                    tr(keys::PINNED_UNPIN_SURE),
                    tr(keys::CONTEXT_UNPIN_FROM_TOP),
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
        .into_any_element()
}

/// The pinned bar's list toggle when several messages are pinned.
fn pinned_bar_list_button(open: bool, cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("pinned-bar-list")
        .icon(gpui_kit::assets::IconName::List)
        .ghost()
        .small()
        .selected(open)
        .tooltip(tr(keys::Q_CHAT_PINNED_MESSAGES))
        .accessibility_label(tr(keys::Q_CHAT_PINNED_MESSAGES))
        .on_click(cx.listener(|this, _, _, cx| {
            this.history.pinned_list_open = !this.history.pinned_list_open;
            cx.notify();
        }))
        .into_any_element()
}
