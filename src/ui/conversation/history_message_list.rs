//! Methods moved out of `conversation.rs` to keep files under 1000 lines.

use super::*;

/// The open thread's window, read once per render: it drives the unread
/// divider, the jump button's badge, forward paging and scroll anchoring
/// the way `HistoryState` does for the main history.
struct ThreadWindow {
    thread_id: i64,
    unread_anchor: Option<MessageId>,
    unread_count: i32,
    has_newer: bool,
    window_epoch: u64,
}

impl QuillApp {
    /// kit Phase 3: the shared history row list used by both chat history
    /// and per-topic history, virtualized through the kit `MessageScroller`
    /// — only visible rows build elements. `messages` are oldest-first.
    /// Per-row inputs are snapshotted into `history_rows` each render;
    /// `render_history_row` rebuilds the visible window from them, so all
    /// existing row behavior (albums, replies, reactions, media, swipe,
    /// context menu, failed sends) is unchanged.
    pub(in crate::ui) fn history_message_list(
        &mut self,
        id: &'static str,
        // `None`: nothing feeding the rows changed since the last build
        // (`HistoryRowsKey`) — keep `history_rows` and the scroller as is.
        messages: Option<Vec<HistoryMessage>>,
        chat: Option<&ChatSummary>,
        sender_name: &str,
        highlight_id: Option<MessageId>,
        media_roots: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let jump_serial = session.map_or(0, |s| s.search.chat_search.jump_serial);
        // Phase B4: secret chats word the timer-change service row as
        // "Self-destruct".
        let is_secret = chat.is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }));
        // kit Phase 3: the scroller resets when the open chat/topic
        // changes. Read the key up front — the `session` borrow must end
        // before the state below is assigned.
        let thread_window = session
            .and_then(|s| s.open_chat.and_then(|chat| s.thread_for_chat(chat)))
            .map(|thread| ThreadWindow {
                thread_id: thread.thread_id,
                unread_anchor: thread.unread_anchor,
                unread_count: thread.unread_count,
                has_newer: thread.has_newer,
                window_epoch: thread.window_epoch,
            });
        let history_key = (
            session
                .and_then(|s| s.open_chat)
                .map(|chat| chat.0)
                .unwrap_or(-1),
            session.and_then(|s| s.open_topic),
            thread_window.as_ref().map_or(0, |thread| thread.thread_id),
        );
        // The loaded window: the thread's own (it opens around its read
        // position and pages forward like the main history) or the main
        // history's (topic views page their own history and have none of
        // this).
        let (unread_anchor, has_newer, window_epoch) = match &thread_window {
            Some(thread) => (thread.unread_anchor, thread.has_newer, thread.window_epoch),
            None => session
                .filter(|s| s.open_topic.is_none())
                .and_then(|s| s.open_chat.and_then(|chat| s.histories.get(&chat.0)))
                .map(|h| (h.unread_anchor, h.has_newer, h.window_epoch))
                .unwrap_or((None, false, 0)),
        };
        let unread_count = match &thread_window {
            Some(thread) => thread.unread_count,
            None => chat.map_or(0, |chat| chat.unread_count),
        };
        if let Some(mut messages) = messages {
            // Deleted messages dissolve in place before they go.
            self.merge_vanishing(&mut messages, cx);
            let mut divider_placed = false;
            let mut first_unread = |message: &HistoryMessage| {
                let first = !divider_placed
                    && !message.is_outgoing
                    && unread_anchor.is_some_and(|anchor| message.id.0 > anchor.0);
                divider_placed |= first;
                first
            };
            let groups = quill::album::group_media_albums(
                &messages,
                |message| message.media_album_id,
                |message| message.is_outgoing,
                |message| quill::album::is_album_media(&message.content),
            );
            let mut rows: Vec<HistoryRow> = Vec::with_capacity(groups.len());
            let is_group = matches!(
                chat.map(|summary| &summary.kind),
                Some(
                    ChatKind::BasicGroup { .. }
                        | ChatKind::Supergroup {
                            is_channel: false,
                            ..
                        }
                )
            );
            // Sender of each row (first item for albums): a run is consecutive
            // rows from one sender. The name heads a run; the avatar sits
            // beside its last row, with a spacer keeping earlier rows aligned.
            let identities: Vec<(bool, Option<MessageSender>)> = groups
                .iter()
                .map(|group| match group {
                    quill::album::HistoryGroup::Album { messages, .. } => messages
                        .first()
                        .map(|m| (m.is_outgoing, m.sender))
                        .unwrap_or((false, None)),
                    quill::album::HistoryGroup::Single(message) => {
                        (message.is_outgoing, message.sender)
                    }
                })
                .collect();
            // Each group's span over `messages` (album id, item count): the
            // rows below take the messages by value instead of cloning them.
            let spans: Vec<(Option<i64>, usize)> = groups
                .iter()
                .map(|group| match group {
                    quill::album::HistoryGroup::Album { album_id, messages } => {
                        (Some(*album_id), messages.len())
                    }
                    quill::album::HistoryGroup::Single(_) => (None, 1),
                })
                .collect();
            drop(groups);
            let mut owned = messages.into_iter();
            let mut previous = None;
            let mut row_chrome = |message: &HistoryMessage, continues: bool| {
                let identity = (message.is_outgoing, message.sender);
                let service = matches!(
                    message.content,
                    MessageContent::ScreenshotTaken | MessageContent::ChatJoinFromCommunity { .. }
                );
                let show_sender = previous != Some(identity);
                previous = Some(identity);
                let accent = match message.sender {
                    Some(MessageSender::User { user_id }) => session
                        .and_then(|s| s.user(user_id))
                        .map(|u| u.accent_color_id),
                    // Chats carry no parsed name color: derive one from the id
                    // the way Telegram assigns defaults.
                    Some(MessageSender::Chat { chat_id }) => Some(chat_id.rem_euclid(7) as i32),
                    None => None,
                };
                let name = match message.sender {
                    Some(MessageSender::User { user_id }) => session
                        .and_then(|s| s.user(user_id))
                        .map(|u| u.display_name()),
                    Some(MessageSender::Chat { chat_id }) => session
                        .and_then(|s| s.chats.get(&chat_id))
                        .map(|c| c.title.clone()),
                    None => None,
                }
                .unwrap_or_else(|| {
                    if is_group {
                        "Member".into()
                    } else {
                        sender_name.to_string()
                    }
                });
                let sender =
                    (!message.is_outgoing && (service || (is_group && show_sender))).then(|| {
                        SenderLabel {
                            name: name.clone(),
                            accent: accent.filter(|_| is_group),
                        }
                    });
                let receipt = if message.is_outgoing {
                    chat.map(|summary| summary.outbox_receipt(message))
                        .unwrap_or(OutboxReceipt::Sent)
                } else {
                    OutboxReceipt::None
                };
                let photo = match message.sender {
                    Some(MessageSender::User { user_id }) => {
                        session.and_then(|s| s.user_photo_path(user_id))
                    }
                    Some(MessageSender::Chat { chat_id }) => {
                        session.and_then(|s| s.chat_photo_path(ChatId(chat_id)))
                    }
                    None => None,
                }
                .and_then(|path| sandboxed_display_path(path, &media_roots));
                let sender_avatar = (!message.is_outgoing && is_group).then(|| {
                    if continues {
                        // Spacer: same column width, no avatar.
                        (String::new(), None)
                    } else {
                        (name, photo)
                    }
                });
                (sender, receipt, sender_avatar, show_sender)
            };
            let now = quill::local_time::civil_local(quill::local_time::now_unix());
            let mut previous_day: Option<i64> = None;
            let mut day_label = |date: i32| {
                if date <= 0 {
                    return None;
                }
                let civil = quill::local_time::civil_local(i64::from(date));
                let day = civil.day_number();
                (previous_day.replace(day) != Some(day))
                    .then(|| quill::local_time::day_label(&civil, &now))
            };
            for (index, (album, len)) in spans.into_iter().enumerate() {
                let continues = identities
                    .get(index + 1)
                    .is_some_and(|next| Some(next) == identities.get(index));
                match album {
                    Some(album_id) => {
                        let album_messages: Vec<HistoryMessage> =
                            owned.by_ref().take(len).collect();
                        // Same chrome rule as single rows, from the first item.
                        let (sender, receipt, sender_avatar, _) = album_messages
                            .first()
                            .map(|message| row_chrome(message, continues))
                            .unwrap_or((None, OutboxReceipt::None, None, false));
                        let day_label = album_messages.first().and_then(|m| day_label(m.date));
                        let unread_divider = album_messages.first().is_some_and(&mut first_unread);
                        rows.push(HistoryRow::Album {
                            album_id,
                            messages: album_messages,
                            sender,
                            receipt,
                            sender_avatar,
                            day_label,
                            unread_divider,
                        })
                    }
                    None => {
                        let Some(message) = owned.next() else {
                            break;
                        };
                        let (sender, receipt, sender_avatar, run_start) =
                            row_chrome(&message, continues);
                        // Phase 4.6: audio/voice rows get a seek-bar view model.
                        let seek_bar = match &message.content {
                            MessageContent::VoiceNote(note) => Some(self.seek_bar_view(
                                message.chat_id,
                                message.id,
                                f64::from(note.duration),
                            )),
                            MessageContent::Audio(audio) => Some(self.seek_bar_view(
                                message.chat_id,
                                message.id,
                                f64::from(audio.duration),
                            )),
                            _ => None,
                        };
                        let animation_playing = self.playback.playing_animation == Some(message.id);
                        let animation_frame = if animation_playing {
                            self.playback
                                .animation_frames
                                .get(self.playback.animation_frame)
                                .cloned()
                                .or_else(|| self.playback.animation_frames.first().cloned())
                        } else {
                            None
                        };
                        let video_playing = self.playback.playing_video == Some(message.id);
                        let video_frame = if video_playing {
                            self.playback
                                .video_frames
                                .get(self.playback.video_frame)
                                .cloned()
                                .or_else(|| self.playback.video_frames.first().cloned())
                        } else {
                            None
                        };
                        let row_day_label = day_label(message.date);
                        let row_unread_divider = first_unread(&message);
                        rows.push(HistoryRow::Single(Box::new(HistoryRowInputs {
                            sender,
                            receipt,
                            sender_avatar,
                            highlighted: highlight_id == Some(message.id),
                            run_start: run_start && index > 0,
                            joined_above: !run_start
                                && row_day_label.is_none()
                                && !row_unread_divider,
                            day_label: row_day_label,
                            unread_divider: row_unread_divider,
                            reply_header: session.and_then(|s| s.reply_header(&message)),
                            forward_header: session.and_then(|s| s.forward_header(&message)),
                            via_bot: session.and_then(|s| s.via_bot_label(&message)),
                            seek_bar,
                            animation_playing,
                            animation_frame,
                            video_playing,
                            video_frame,
                            is_secret,
                            vanishing: self.vanish_progress(message.id),
                            message,
                        })));
                    }
                }
            }
            // A bot's streaming reply (`updatePendingMessage`) is its next
            // incoming message, growing as it arrives (Telegram Desktop).
            if let Some(message) = self.streaming_bot_message(cx) {
                let (sender, receipt, sender_avatar, run_start) = row_chrome(&message, true);
                rows.push(HistoryRow::Single(Box::new(HistoryRowInputs {
                    sender,
                    receipt,
                    sender_avatar,
                    highlighted: false,
                    run_start,
                    joined_above: !run_start,
                    day_label: None,
                    unread_divider: false,
                    reply_header: None,
                    forward_header: None,
                    via_bot: None,
                    seek_bar: None,
                    animation_playing: false,
                    animation_frame: None,
                    video_playing: false,
                    video_frame: None,
                    is_secret: false,
                    vanishing: None,
                    message,
                })));
            }
            // kit Phase 3: sync the scroller state with the new row list.
            let count = rows.len();
            let (first, last) = (
                rows.first().and_then(HistoryRow::first_id),
                rows.last().and_then(HistoryRow::last_id),
            );
            // The list caches each row's measured height. A row can grow in
            // place — a photo finishes downloading (placeholder → image), a
            // message is edited, reactions arrive — and would then be clipped,
            // so remeasure rows whose inputs changed. File readiness affects
            // any media row, so a change there remeasures every row.
            let media_signature = self
                .session()
                .map(|s| {
                    (
                        s.media
                            .files
                            .values()
                            .filter(|file| file.usable_path().is_some())
                            .count(),
                        s.media.downloading.len(),
                    )
                })
                .unwrap_or_default();
            if self.history.key == Some(history_key)
                && count == self.history.rows.len()
                && count == self.history.scroller.read(cx).item_count()
            {
                if media_signature != self.history.media_signature {
                    self.history
                        .scroller
                        .update(cx, |state, cx| state.remeasure(cx));
                } else {
                    let changed: Vec<usize> = rows
                        .iter()
                        .zip(&self.history.rows)
                        .enumerate()
                        .filter(|(_, (new, old))| !new.renders_like(old))
                        .map(|(ix, _)| ix)
                        .collect();
                    if !changed.is_empty() {
                        self.history.scroller.update(cx, |state, cx| {
                            for ix in changed {
                                let _ = state.remeasure_items(ix..ix + 1, cx);
                            }
                        });
                    }
                }
            }
            self.history.media_signature = media_signature;
            if self.history.key != Some(history_key) || self.history.window_epoch != window_epoch {
                // A new or replaced window anchors once its rows are in.
                self.history.window_epoch = window_epoch;
                self.history.anchor_pending = true;
            }
            if self.history.key != Some(history_key) {
                // New chat/topic (or first render): reset; the anchor below
                // picks the position.
                self.history.key = Some(history_key);
                // Message ids are chat-local: a stale highlight id in the new
                // chat must not suppress its search-jump scroll.
                self.history.last_highlight = None;
                self.history.scroll_date = Default::default();
                self.history.scroll_top_probe.set(None);
                self.history.scroller.update(cx, |state, cx| {
                    state.reset(count, cx);
                });
            } else if count != self.history.scroller.read(cx).item_count() {
                let prev_count = self.history.scroller.read(cx).item_count();
                // R6: a page landed and the window trimmed its far end —
                // the rows slid. Remove exactly the dropped rows (so the
                // reader's scroll anchor shifts with them) instead of
                // splicing the whole list.
                let slide = (prev_count == self.history.rows.len())
                    .then(|| {
                        let ids = |rows: &[HistoryRow]| -> Vec<(i64, i64)> {
                            rows.iter()
                                .map(|row| {
                                    (
                                        row.first_id().map_or(0, |id| id.0),
                                        row.last_id().map_or(0, |id| id.0),
                                    )
                                })
                                .collect()
                        };
                        quill::state::row_window_shift(&ids(&self.history.rows), &ids(&rows))
                    })
                    .flatten()
                    .filter(|shift| shift.front_removed + shift.tail_removed > 0);
                match (first, last, self.history.ends) {
                    _ if slide.is_some() => {
                        if let Some(shift) = slide {
                            let newer_page = self.history.had_newer;
                            self.history.scroller.update(cx, |state, cx| {
                                let following = state.is_following_tail();
                                if shift.tail_removed > 0 {
                                    state.splice(
                                        prev_count - shift.tail_removed..prev_count,
                                        0,
                                        cx,
                                    );
                                }
                                if shift.front_removed > 0 {
                                    state.splice(0..shift.front_removed, 0, cx);
                                }
                                if shift.front_added > 0 {
                                    state.prepend(shift.front_added, cx);
                                }
                                if shift.tail_added > 0 {
                                    state.append(shift.tail_added, cx);
                                    // Keep the old last row in view and read
                                    // on from there (as for any newer page).
                                    if newer_page && following {
                                        state.scroll_to_item(count - shift.tail_added - 1, cx);
                                    }
                                }
                            });
                        }
                    }
                    (Some(_), _, Some((prev_first, _)))
                        if first == Some(prev_first) && count > prev_count =>
                    {
                        // New messages appended at the tail — tail-follow keeps
                        // the view pinned when the user is at the bottom. A
                        // newer page must not be skipped that way: keep the old
                        // last row in view and read on from there.
                        let added = count.saturating_sub(prev_count);
                        let newer_page = self.history.had_newer;
                        let mut following = false;
                        self.history.scroller.update(cx, |state, cx| {
                            following = state.is_following_tail();
                            state.append(added, cx);
                            if newer_page && following {
                                state.scroll_to_item(prev_count.saturating_sub(1), cx);
                            }
                        });
                        // tdesktop reveals a few new rows at the bottom of a
                        // history that is pinned there (`itemRevealDuration`);
                        // an inactive window just snaps.
                        if following
                            && !newer_page
                            && prev_count > 0
                            && added <= NEW_ROW_REVEAL_MAX
                            && self.frame.window_active.get()
                            && self.frame.motion.list_fills.get()
                        {
                            self.frame
                                .motion
                                .start_reveal(prev_count, std::time::Instant::now());
                        }
                    }
                    (_, Some(_), Some((_, prev_last)))
                        if last == Some(prev_last) && count > prev_count =>
                    {
                        // Older history prepended — the visible anchor stays.
                        let added = count.saturating_sub(prev_count);
                        self.history.scroller.update(cx, |state, cx| {
                            state.prepend(added, cx);
                        });
                    }
                    _ => {
                        // Shrink or reorder (delete/edit): splice preserves the
                        // scroll anchor (reset() would yank to the top and arm
                        // tail-follow); stay at the tail only if the user was
                        // following it.
                        let follow = self.history.scroller.read(cx).is_following_tail();
                        self.history.scroller.update(cx, |state, cx| {
                            state.splice(0..prev_count, count, cx);
                            if follow {
                                state.scroll_to_end(cx);
                            }
                        });
                    }
                }
            } else if let (Some(first), Some(last)) = (first, last)
                && self.history.ends != Some((first, last))
            {
                // Same row count but row identity changed (e.g. a second album
                // photo turned a Single row into a taller Album row): cached
                // measured heights are stale, so remeasure. This converges —
                // history_ends is updated below — and must not run
                // unconditionally or remeasure's notify() would loop.
                self.history.scroller.update(cx, |state, cx| {
                    state.remeasure(cx);
                });
            }
            self.history.ends = match (first, last) {
                (Some(first), Some(last)) => Some((first, last)),
                _ => None,
            };
            self.history.rows = rows;
            self.history.had_newer = has_newer;
            // A new or replaced window: start at the jump target, else at the
            // "Unread messages" divider, else at the bottom.
            if self.history.anchor_pending && !self.history.rows.is_empty() {
                self.history.anchor_pending = false;
                let highlighted = highlight_id.and_then(|target| {
                    self.history
                        .rows
                        .iter()
                        .position(|row| row.contains(target))
                });
                if highlighted.is_some() {
                    self.history.last_highlight = highlight_id;
                }
                let target = highlighted.or_else(|| {
                    self.history
                        .rows
                        .iter()
                        .position(HistoryRow::unread_divider)
                        // A little context above the divider.
                        .map(|ix| ix.saturating_sub(1))
                });
                self.history.scroller.update(cx, |state, cx| match target {
                    Some(ix) => {
                        state.scroll_to_item(ix, cx);
                    }
                    None => state.scroll_to_end(cx),
                });
            }
            // Chat-search jump: scroll the highlight into view once per new
            // `highlight_id` (current code only outlined the message).
            if highlight_id != self.history.last_highlight {
                self.history.last_highlight = highlight_id;
                if let Some(target) = highlight_id
                    && let Some(ix) = self
                        .history
                        .rows
                        .iter()
                        .position(|row| row.contains(target))
                {
                    self.history.scroller.update(cx, |state, cx| {
                        state.scroll_to_item(ix, cx);
                    });
                }
            }
        }
        self.history.shared = HistoryShared { media_roots };
        // A new jump (search hit, reply, pinned message) restarts the
        // highlight fade, even onto the message highlighted before.
        match (highlight_id, jump_serial) {
            (Some(_), serial) if self.history.highlight_fade.map(|f| f.0) != Some(serial) => {
                self.history.highlight_fade = Some((serial, std::time::Instant::now()));
            }
            (None, _) => self.history.highlight_fade = None,
            _ => {}
        }
        let count = self.history.rows.len();
        let corner_buttons = self.jump_corner_buttons(
            chat.as_ref().map_or(0, |c| c.unread_mention_count),
            chat.as_ref().map_or(0, |c| c.unread_reaction_count),
            chat.as_ref().map_or(0, |c| c.unread_poll_vote_count),
            cx,
        );
        // kit Phase 3: only visible rows render. Row 0 becoming visible
        // pages older history (the driver dedupes in-flight requests and
        // reports exhaustion; the loader notifies only when a request was
        // actually sent, so this cannot notify-loop while pinned at top).
        // Inline players for rows that don't render this pass stop.
        self.playback.inline_videos.borrow_mut().begin_render();
        let weak = cx.weak_entity();
        let date_pill = self.scroll_date_pill(cx);
        let probe = self.history.scroll_probe.clone();
        let top_probe = self.history.scroll_top_probe.clone();
        let view_probe = self.history.scroll_view_probe.clone();
        let reveal = self.history_reveal(cx);
        let reveal_probe = self.frame.motion.reveal_probe();
        let jump_zone = self.history.scroller.read(cx).is_scrolled_up().then(|| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(56.))
                .flex()
                .justify_center()
                .child(super::anim_layer::occluder(div().w(px(128.)).h_full()))
        });
        let look = self.chat_look(self.open_chat_id().map(|c| c.0), cx);
        super::selectable_text::selection_viewport(
            super::wallpaper::paint_wallpaper(
                div()
                    .id(id)
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    // kit Phase 7: screen-reader landmark for the message history.
                    .role(Role::Log)
                    .aria_label(format!("Message history — {sender_name}"))
                    // Middle-click autoscroll (`ui/widgets/middle_click_autoscroll`).
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.autoscroll_press(event.position, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(|this, _: &MouseUpEvent, _, cx| {
                            this.autoscroll_release(cx);
                        }),
                    ),
                // The chat's wallpaper, else the account's (None keeps the
                // theme background).
                look.wallpaper.as_ref(),
                look.dimming,
            )
            .child(super::history_fx::reveal_viewport(
                reveal,
                MessageScroller::new(id, self.history.scroller.clone(), move |ix, _window, cx| {
                    let gif_view = weak.clone();
                    cx.defer(move |cx| {
                        let _ = gif_view.update(cx, |this, cx| this.maybe_autoplay_gif(ix, cx));
                    });
                    if ix == 0 {
                        let weak = weak.clone();
                        cx.defer(move |cx| {
                            let _ = weak.update(cx, |this, cx| this.maybe_auto_load_older(cx));
                        });
                    }
                    // Prefetch the next newer page a few rows before the
                    // window's end so reading on rarely waits.
                    if has_newer && ix + NEWER_PREFETCH_ROWS >= count {
                        let weak = weak.clone();
                        cx.defer(move |cx| {
                            let _ = weak.update(cx, |this, cx| this.maybe_auto_load_newer(cx));
                        });
                    }
                    weak.update(cx, |this, cx| this.render_history_row(ix, cx))
                        .unwrap_or_else(|_| div().into_any_element())
                })
                // The kit's default row wrapper pads every non-last row with
                // pb_8 (32px); override to pb_1 to restore the old gap_1
                // density. The kit also supplies row px and list py, so the
                // outer div needs neither.
                .with_row_style(StyleRefinement::default().pb_1())
                // Jump to latest: shows the unread count, and replaces a
                // window that stops short of the latest message instead of
                // only scrolling to its end.
                .with_jump_button_renderer({
                    let jump = cx.weak_entity();
                    move |button| {
                        button
                            .when(unread_count > 0, |button| {
                                button.label(unread_count.to_string())
                            })
                            .on_click(move |_, _, cx| {
                                let _ =
                                    jump.update(cx, |this, cx| this.jump_to_latest_messages(cx));
                            })
                    }
                })
                .size_full()
                .min_h_0(),
            ))
            // Paints after the rows: the first row reaching below the
            // top edge is what the floating date describes.
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, _, _| {
                        let mut rows = probe.borrow_mut();
                        reveal_probe.note(&rows, count, bounds.top());
                        let top = rows
                            .iter()
                            .filter(|(_, row, _)| row.bottom() > bounds.top())
                            .min_by_key(|(ix, _, _)| *ix)
                            .map(|(ix, row, has_day)| (*ix, *has_day && row.top() >= bounds.top()));
                        let on_screen = rows
                            .iter()
                            .filter(|(_, row, _)| {
                                row.bottom() > bounds.top() && row.top() < bounds.bottom()
                            })
                            .map(|(ix, _, _)| *ix);
                        let range = on_screen.clone().min().zip(on_screen.max());
                        rows.clear();
                        if top.is_some() {
                            top_probe.set(top);
                        }
                        if range.is_some() {
                            view_probe.set(range);
                        }
                    },
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
            .children(date_pill)
            // tdesktop's corner "@" / heart buttons.
            .children(corner_buttons.map(super::anim_layer::occluder))
            // Where the scroller's jump-to-latest button floats while
            // scrolled up (it is drawn inside the kit's scroller): what
            // animates under it is drawn by the conversation.
            .children(jump_zone),
        )
        .into_any_element()
    }

    /// kit Phase 3: resolve one virtualized history row to its element —
    /// the per-render snapshot in `history_rows`/`history_shared`, with the
    /// row's highlight, selected-forward, failed-send and mouse behavior
    /// unchanged from the pre-virtualization list.
    pub(in crate::ui) fn render_history_row(
        &self,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = self.history.rows.get(ix) else {
            return div().into_any_element();
        };
        {
            let mut rendered = self.history.rendered_rows.borrow_mut();
            if !rendered.contains(&ix) {
                rendered.push(ix);
            }
        }
        // A row scrolled into view that hasn't been reported yet: render
        // the app once more so `report_visible_history` sees it promptly
        // (list scrolling alone doesn't re-render the app view).
        let unreported = self.live.is_some()
            && self.history.window_active
            && row.message_ids().iter().any(|id| {
                !self
                    .history
                    .reported_visible
                    .as_ref()
                    .is_some_and(|(_, ids)| ids.contains(id))
            });
        if unreported {
            cx.notify();
        }
        let element = self.render_history_row_body(row, cx);
        let has_day = row.day_label().is_some();
        let probe = self.history.scroll_probe.clone();
        // Notes where the row is painted, for the floating date pill.
        let tracker = canvas(
            |_, _, _| {},
            move |bounds, _, _, _| probe.borrow_mut().push((ix, bounds, has_day)),
        )
        .absolute()
        .inset_0()
        .size_full();
        div()
            .relative()
            .flex()
            .flex_col()
            .child(tracker)
            .when_some(row.day_label(), |this, label| {
                this.child(day_separator(label, cx))
            })
            .when(row.unread_divider(), |this| {
                let unread_at_open = self
                    .session()
                    .and_then(|s| s.open_chat.and_then(|chat| s.histories.get(&chat.0)))
                    .map_or(0, |h| h.unread_at_open);
                this.child(unread_divider(unread_at_open, cx))
            })
            .child(element)
            .into_any_element()
    }
}
