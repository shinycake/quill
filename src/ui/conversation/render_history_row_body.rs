//! Methods moved out of `conversation.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(super) fn render_history_row_body(
        &self,
        row: &HistoryRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shared = &self.history.shared;
        // File state is read from the session at row-render time rather
        // than copied into a per-render snapshot.
        let no_files = HashMap::new();
        let no_ids = std::collections::HashSet::new();
        let session = self.session();
        let files = session.map_or(&no_files, |s| &s.media.files);
        let downloading = session.map_or(&no_ids, |s| &s.media.downloading);
        let failed = session.map_or(&no_ids, |s| &s.media.failed_downloads);
        // Settings → Appearance: font size + bubble/plain style.
        let look = self.bubble_look(cx);
        match row {
            HistoryRow::Album {
                album_id,
                messages,
                sender,
                receipt,
                sender_avatar,
                ..
            } => {
                let refs: Vec<&HistoryMessage> = messages.iter().collect();
                album_history_row(
                    *album_id,
                    &refs,
                    files,
                    downloading,
                    &shared.media_roots,
                    sender.clone(),
                    *receipt,
                    sender_avatar.clone(),
                    // Settings → Appearance: font size + bubble/plain style.
                    look,
                    cx,
                )
            }
            HistoryRow::Single(inputs) => {
                let message = &inputs.message;
                let inline = self.inline_frame(message, cx);
                // A covered spoiler's dust shimmers (gently: 20 fps).
                let spoiler = match &message.content {
                    MessageContent::Photo(photo) => photo.has_spoiler,
                    MessageContent::Video(video) => video.has_spoiler,
                    MessageContent::Animation(animation) => animation.has_spoiler,
                    _ => false,
                };
                let key = (message.chat_id.0, message.id.0 as u64, u64::MAX, false);
                // lib_ui draws spoiler frames every 33 ms; a reveal fades
                // the cover out.
                // Outside an animation layer (`anim_layer`), and while a
                // reveal fades the cover, the specks are redrawn with the
                // history.
                let covered = !self.message_ui.spoiler_revealed.contains(&key);
                let fading = super::spoiler_fx::reveal_fade(key).is_some();
                if spoiler
                    && (fading
                        || (covered
                            && super::anim_layer::current().is_none()
                            && !super::spoiler_fx::still()))
                {
                    self.request_animation_tick(30, cx);
                }
                let row = session_history_row(
                    message,
                    files,
                    downloading,
                    failed,
                    &shared.media_roots,
                    inputs.sender.clone(),
                    inputs.receipt,
                    inputs.sender_avatar.clone(),
                    inputs.reply_header.clone(),
                    inputs.forward_header.clone(),
                    inputs.via_bot.clone(),
                    inputs.seek_bar.clone(),
                    inputs.animation_playing,
                    inputs.animation_frame.clone(),
                    match &message.content {
                        MessageContent::Sticker(sticker) => {
                            self.history_sticker(sticker.file_id, sticker.format, cx)
                        }
                        MessageContent::Dice(dice) => dice
                            .final_sticker
                            .as_ref()
                            .and_then(|s| self.history_dice_sticker(s.file_id, s.format, cx)),
                        _ => None,
                    },
                    match &message.content {
                        MessageContent::Dice(dice) => dice
                            .slot_layers
                            .iter()
                            .map(|s| self.history_dice_sticker(s.file_id, s.format, cx))
                            .collect(),
                        _ => Vec::new(),
                    },
                    self.message_custom_emoji_frames(message, cx),
                    inputs.video_playing,
                    inputs.video_frame.clone(),
                    inline,
                    &self.message_ui.spoiler_revealed,
                    inputs.is_secret,
                    self.session(),
                    // Settings → Appearance: font size + bubble/plain style;
                    // grouping from the neighbouring rows.
                    BubbleLook {
                        joined_above: inputs.joined_above,
                        ..look
                    },
                    cx,
                );
                // M1: `cx.listener` closures must be `'static`, so the
                // row's ids are copied out of the message first.
                let (row_chat, row_msg) = (message.chat_id, message.id);
                let message_pending = message.pending;
                let selection = self.selection_row(row_chat, row_msg, message.pending, cx);
                let (selection_overlay, selection_tint, selection_slide) =
                    (selection.overlay, selection.tint, selection.slide);
                let outgoing = message.is_outgoing;
                let vanishing = inputs.vanishing;
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
                        |this, alpha| this.bg(cx.theme().primary.opacity(alpha)),
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
                            // B11: double-click reacts with the quick
                            // reaction (tdesktop `toggleFavoriteReaction`);
                            // on selected text it selects the word instead.
                            if event.click_count == 2
                                && !this.selecting_in(row_chat)
                                && !gpui_kit::base::TextSelection::has_selection(window, cx)
                            {
                                this.quick_react(row_chat, row_msg, cx);
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
                                    .when(outgoing, |this| this.right(px(56.)))
                                    .when(!outgoing, |this| this.left(px(64.)))
                                    .opacity(frame.alpha)
                                    .text_size(px(26. * frame.scale))
                                    .child(glyph),
                            )
                        },
                    )
                    // Selecting slides an outgoing bubble aside for the check.
                    .child(if outgoing && selection_slide > 0. {
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
                                .child("⚠ Failed to send — right-click → Retry send"),
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
                    .and_then(|chat| live.driver.session.histories.get(&chat.0))
                    .is_some_and(|h| h.has_newer)
                    && live.driver.jump_to_latest().is_ok()
            })
        };
        if !replaced {
            if !in_thread
                && let Some(live) = self.live.as_mut()
                && let Some(chat) = live.driver.session.open_chat
                && let Some(history) = live.driver.session.histories.get_mut(&chat.0)
            {
                history.unread_anchor = None;
            }
            self.history
                .scroller
                .update(cx, |state, cx| state.scroll_to_end(cx));
        }
        cx.notify();
    }

    /// The "Couldn't load messages · Retry" row: ask for the first page
    /// again.
    pub(in crate::ui) fn retry_history_load(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.retry_history().is_err()
        {
            self.connection.status_note = "could not load history".into();
        }
        cx.notify();
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
