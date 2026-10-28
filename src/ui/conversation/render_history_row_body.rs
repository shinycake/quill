//! Methods moved out of `conversation.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};

impl QuillApp {
    pub(in crate::ui) fn history_row_seek_bar(
        &self,
        message: &HistoryMessage,
    ) -> Option<crate::ui::playback::SeekBarView> {
        let duration = match &message.content {
            MessageContent::VoiceNote(note) => note.duration,
            MessageContent::Audio(audio) => audio.duration,
            _ => return None,
        };
        Some(self.seek_bar_view(message.chat_id, message.id, f64::from(duration)))
    }

    pub(super) fn render_history_row_body(
        &self,
        row: &HistoryRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shared = &self.history.shared;
        // File state is read from the session at row-render time rather
        // than copied into a per-render snapshot.
        let no_files = quill::state::FileMap::default();
        let no_ids = quill::state::DownloadIntents::default();
        let session = self.session();
        let files = session.map_or(&no_files, |s| &s.media.files);
        let downloading = session.map_or(&no_ids, |s| &s.media.downloads);
        // Settings → Appearance: font size + bubble/plain style.
        let look = self.bubble_look(cx);
        match row {
            HistoryRow::Album {
                album_id,
                messages,
                chrome,
                ..
            } => {
                for message in messages {
                    self.tick_media_spoiler(message, cx);
                }
                let refs: Vec<&HistoryMessage> = messages.iter().collect();
                let sender_status = refs.first().and_then(|first| {
                    self.sender_status_element(chrome.sender.as_ref(), first.id.0, cx)
                });
                let avatar = self.history_sender_avatar(
                    chrome,
                    refs.first().and_then(|first| first.sender),
                    cx,
                );
                album_history_row(
                    *album_id,
                    &refs,
                    files,
                    downloading,
                    &shared.media_roots,
                    chrome.sender.clone(),
                    sender_status,
                    chrome.receipt,
                    avatar,
                    // Settings → Appearance: font size + bubble/plain style.
                    look,
                    session,
                    &self.message_ui.spoiler_revealed,
                    cx,
                )
            }
            HistoryRow::Single(inputs) => {
                let message = &inputs.message;
                let chrome = &inputs.chrome;
                let inline = self.inline_frame(message, cx);
                let animation_playing = self.playback.playing_animation == Some(message.id);
                let animation = animation_playing
                    .then(|| self.playback.gif_play())
                    .flatten();
                // A looping GIF outside an animation layer is redrawn with
                // the history, at the GIF's own cadence.
                if let Some(gif) = animation.as_ref()
                    && super::anim_layer::current().is_none()
                    && gif.fps() > 0
                {
                    self.request_animation_tick(gif.fps(), cx);
                }
                self.tick_media_spoiler(message, cx);
                let sender_status =
                    self.sender_status_element(chrome.sender.as_ref(), message.id.0, cx);
                let avatar = self.history_sender_avatar(chrome, message.sender, cx);
                let row = session_history_row(
                    message,
                    files,
                    downloading,
                    &shared.media_roots,
                    chrome.sender.clone(),
                    sender_status,
                    chrome.receipt,
                    avatar,
                    chrome.reply_header.clone(),
                    chrome.forward_header.clone(),
                    chrome.via_bot.clone(),
                    self.history_row_seek_bar(message),
                    animation_playing,
                    animation,
                    match &message.content {
                        MessageContent::Sticker(sticker) => {
                            // tdesktop `Sticker::mirrorHorizontal`.
                            let mirror = crate::ui::sticker_player::mirrors(
                                sticker.premium_animation.is_some(),
                                quill::chat_wide::has_right_layout(message.is_outgoing, look.wide),
                            );
                            let (w, h) = crate::ui::message_media::sticker_box(
                                sticker.width,
                                sticker.height,
                            );
                            let side = f32::from(w.max(h));
                            self.history_sticker(message, sticker, mirror, side, cx)
                        }
                        MessageContent::Text(_) => self.lone_emoji_view(message, cx),
                        _ => None,
                    },
                    match &message.content {
                        MessageContent::Dice(dice) => dice
                            .animation()
                            .map(|(phase, stickers)| {
                                stickers
                                    .layers()
                                    .iter()
                                    .enumerate()
                                    .map(|(index, s)| {
                                        self.history_dice_sticker(message, index, s, phase, cx)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    },
                    self.message_custom_emoji_frames(message, cx),
                    inline,
                    &self.message_ui.spoiler_revealed,
                    inputs.is_secret,
                    inputs.fact_check_expanded,
                    self.session(),
                    // Settings → Appearance: font size + bubble/plain style;
                    // grouping from the neighbouring rows.
                    BubbleLook {
                        joined_above: inputs.joined_above,
                        ..look
                    },
                    cx,
                );
                let row = self.with_suggested_post_block(message, row, cx);
                // M1: `cx.listener` closures must be `'static`, so the
                // row's ids are copied out of the message first.
                let (row_chat, row_msg) = (message.chat_id, message.id);
                let message_pending = message.pending;
                let wide_right = look.wide.then(|| {
                    let limit = crate::ui::history::bubble_width::message_width_limit(
                        cx,
                        &message.content,
                        look.font,
                        look.plain,
                    )
                    .map_or(quill::bubble_layout::MSG_MAX_WIDTH as f32, f32::from);
                    let width = self
                        .history
                        .viewport
                        .get()
                        .map_or(f32::MAX, |view| f32::from(view.size.width));
                    quill::chat_wide::selection_right(limit, width)
                });
                let selection =
                    self.selection_row(row_chat, row_msg, message.pending, wide_right, cx);
                let (selection_overlay, selection_tint, selection_slide) =
                    (selection.overlay, selection.tint, selection.slide);
                // The side the bubble sits on (`hasRightLayout`): the wide
                // layout keeps outgoing messages on the left.
                let right = quill::chat_wide::has_right_layout(message.is_outgoing, look.wide);
                let vanishing = inputs
                    .vanishing
                    .then(|| self.vanish_progress(row_msg))
                    .flatten();
                let highlighted = inputs.highlighted;
                let failed = message.failed;
                let run_start = inputs.run_start;
                let element = div()
                    .relative()
                    .child(super::vanish::row_tracker(row_msg.0))
                    .when(run_start, |this| this.pt_2())
                    // Jump target, forward selection and failed sends tint
                    // the whole row instead of outlining it: no border or
                    // padding, so the row never shifts as the state flips.
                    .rounded_md()
                    .when_some(
                        highlighted.then(|| self.jump_highlight_alpha(cx)).flatten(),
                        |this, alpha| {
                            this.bg(super::chat_palette::themed(cx.theme().primary).opacity(alpha))
                        },
                    )
                    .when(selection_tint > 0., |this| {
                        this.bg(cx.theme().selection.opacity(selection_tint))
                    })
                    // M1: failed sends stay visibly marked so the retry
                    // affordance is noticed (`updateMessageSendFailed`).
                    .when(failed, |this| this.bg(cx.theme().danger.opacity(0.08)))
                    // M1: right-click opens the message context menu at
                    // the click position (window coordinates).
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.open_message_menu(
                                MessageMenuState {
                                    chat_id: row_chat,
                                    message_id: row_msg,
                                    position: event.position,
                                },
                                window,
                                cx,
                            );
                        }),
                    )
                    // M1: swipe-to-reply — press on the row, release >24px
                    // to the left of the press point starts a reply.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.message_ui.swipe_reply_start =
                                Some((row_chat, row_msg, event.position.x));
                            // A press that starts on text selects text; any
                            // other press may become a message drag
                            // (`selection_drag`). In selection mode the
                            // row's overlay takes the press instead.
                            if !this.selecting_in(row_chat) {
                                this.selection_press_row(
                                    row_chat,
                                    row_msg,
                                    message_pending,
                                    event,
                                    window,
                                    cx,
                                );
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                            if let Some((chat_id, message_id, start_x)) =
                                this.message_ui.swipe_reply_start.take()
                                && chat_id == row_chat
                                && message_id == row_msg
                                && start_x - event.position.x > px(24.)
                            {
                                this.begin_reply_from_message(chat_id, message_id, window, cx);
                            }
                            // Double-click runs the quick action setting
                            // (tdesktop `chatQuickAction`); on selected
                            // text it selects the word instead.
                            if event.click_count == 2
                                && !this.selecting_in(row_chat)
                                && !gpui_kit::base::TextSelection::has_selection(window, cx)
                            {
                                this.double_click_action(row_chat, row_msg, window, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .when_some(
                        self.reaction_fly_for(row_chat.0, row_msg.0, cx),
                        |this, (glyph, frame)| {
                            this.child(
                                div()
                                    .absolute()
                                    .bottom(px(12. + frame.rise))
                                    .when(right, |this| this.right(px(56.)))
                                    .when(!right, |this| this.left(px(64.)))
                                    .opacity(frame.alpha)
                                    .text_size(px(26. * frame.scale))
                                    .child(glyph),
                            )
                        },
                    )
                    // Selecting slides an outgoing bubble aside for the check.
                    .child(if right && selection_slide > 0. {
                        div()
                            .relative()
                            .right(px(selection_slide))
                            .child(row)
                            .into_any_element()
                    } else {
                        row.into_any_element()
                    })
                    // M1: explicit failed-send notice with a retry hint.
                    .when(failed, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(danger())
                                .child(tr(keys::Q_CHAT_FAILED_TO_SEND_RIGHT_CLICK)),
                        )
                    })
                    .children(selection_overlay)
                    .into_any_element();
                match vanishing {
                    Some(t) => self.ghost_row(row_msg, t, element),
                    None => element,
                }
            }
        }
    }

    /// kit Phase 3: automatic older-history paging — called (deferred) when
    /// the top row of the virtualized history becomes visible. Notifies the
    /// frame only when a request was actually sent (the driver dedupes
    /// in-flight `getChatHistory` requests and stops at `loaded_complete`).
    /// The window stops short of the latest message and its end is near:
    /// load the next newer page (the driver dedupes in-flight requests).
    pub(in crate::ui) fn maybe_auto_load_newer(&mut self, cx: &mut Context<Self>) {
        let in_thread = self.thread_active();
        if let Some(live) = self.live.as_mut()
            && live.driver.session.open_topic.is_none()
            && (in_thread || live.driver.session.threads.thread.is_none())
            && live.driver.session.threads.saved.sublist.is_none()
            && live.driver.session.threads.saved.tag_search.is_none()
        {
            // A thread window pages forward on its own.
            let sent = if in_thread {
                live.driver.fetch_thread_history_newer()
            } else {
                live.driver.fetch_history_newer()
            };
            if sent.ok().flatten().is_some() {
                cx.notify();
            }
        }
    }

    /// The jump-to-latest button: a window that stops short of the latest
    /// message is replaced by the newest page (the scroller re-anchors at
    /// the bottom when it lands); otherwise just scroll down.
    pub(in crate::ui) fn jump_to_latest_messages(&mut self, cx: &mut Context<Self>) {
        self.forget_scroll_on_return_to_latest();
        let in_thread = self.thread_active();
        // Inside a thread the button addresses the thread's window: its
        // divider goes, and a window short of the newest replies is
        // replaced (the scroller lands at the end when the page arrives).
        let replaced = if in_thread {
            self.live
                .as_mut()
                .is_some_and(|live| matches!(live.driver.thread_jump_to_latest(), Ok(Some(_))))
        } else {
            self.live.as_mut().is_some_and(|live| {
                live.driver
                    .session
                    .open_chat
                    .is_some_and(|chat| live.driver.session.history_has_newer(chat))
                    && live.driver.jump_to_latest().is_ok()
            })
        };
        if !replaced {
            if !in_thread
                && let Some(live) = self.live.as_mut()
                && let Some(chat) = live.driver.session.open_chat
                && let Some(history) = live.driver.session.histories.get_mut(&chat.0)
            {
                history.end_unread_open();
            }
            self.history
                .scroller
                .update(cx, |state, cx| state.scroll_to_end(cx));
        }
        cx.notify();
    }

    /// The jump-down button: the first unread message under the "Unread
    /// messages" bar while any are left (the scroller re-anchors there, or
    /// when the window around it lands), else the latest. A thread keeps
    /// jumping to its latest replies.
    pub(in crate::ui) fn jump_down_messages(&mut self, cx: &mut Context<Self>) {
        let unread = !self.thread_active()
            && self.live.as_ref().is_some_and(|live| {
                let session = &live.driver.session;
                session
                    .open_chat
                    .and_then(|chat| session.chats.get(&chat.0))
                    .and_then(quill::state::ChatSummary::unread_anchor)
                    .is_some()
            });
        if !unread {
            self.jump_to_latest_messages(cx);
            return;
        }
        self.forget_scroll_on_return_to_latest();
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.jump_down();
        }
        cx.notify();
    }

    /// The "Couldn't load messages · Retry" row: ask for the first page
    /// again.
    pub(in crate::ui) fn retry_history_load(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.retry_history().is_err()
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_LOAD_HISTORY)));
        }
        cx.notify();
    }

    /// "Couldn't load messages · Retry" centred in the topic pane.
    pub(in crate::ui) fn topic_load_failed_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("topic-load-failed")
            .flex_1()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap_2()
            .p_6()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_CHAT_COULDN_T_LOAD_MESSAGES)),
            )
            .child(
                Button::new("topic-retry")
                    .label(tr(keys::Q_CHAT_RETRY))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.retry_topic_load(cx);
                    })),
            )
    }

    /// The topic pane's Retry row: ask for the topic's first page again.
    pub(in crate::ui) fn retry_topic_load(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.retry_topic_history().is_err()
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_LOAD_HISTORY)));
        }
        cx.notify();
    }

    /// The message's media is still behind its spoiler (not clicked yet):
    /// it neither shows nor autoplays.
    pub(in crate::ui) fn media_spoiler_covered(&self, message: &HistoryMessage) -> bool {
        matches!(
            super::spoiler_fx::media_cover(
                &message.content,
                super::spoiler_fx::media_key(message.chat_id, message.id),
                &self.message_ui.spoiler_revealed,
            ),
            Some(super::spoiler_fx::MediaCover::Covered)
        )
    }

    /// A covered media spoiler's dust shimmers (lib_ui draws its frames
    /// every 33 ms): outside an animation layer (`anim_layer`), and while
    /// a reveal fades the cover, the specks are redrawn with the history.
    fn tick_media_spoiler(&self, message: &HistoryMessage, cx: &mut Context<Self>) {
        let key = super::spoiler_fx::media_key(message.chat_id, message.id);
        let redraw = match super::spoiler_fx::media_cover(
            &message.content,
            key,
            &self.message_ui.spoiler_revealed,
        ) {
            Some(super::spoiler_fx::MediaCover::Fading(_)) => true,
            Some(super::spoiler_fx::MediaCover::Covered) => {
                super::anim_layer::current().is_none() && !super::spoiler_fx::still()
            }
            None => false,
        };
        if redraw {
            self.request_animation_tick(30, cx);
        }
    }

    pub(in crate::ui) fn maybe_auto_load_older(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            // Phase 5.1: a topic view pages its own history.
            let sent = if live.driver.session.threads.saved.tag_search.is_some() {
                live.driver.fetch_saved_tag_page()
            } else if live.driver.session.threads.saved.sublist.is_some() {
                live.driver.fetch_saved_sublist_history()
            } else if live.driver.session.threads.thread.is_some() {
                live.driver.fetch_thread_history()
            } else if live.driver.session.open_topic.is_some() {
                live.driver.fetch_topic_history()
            } else {
                live.driver.fetch_history()
            }
            .ok()
            .flatten()
            .is_some();
            if sent {
                cx.notify();
            }
        }
    }
}
