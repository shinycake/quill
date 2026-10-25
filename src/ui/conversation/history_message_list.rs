//! Methods moved out of `conversation.rs` to keep files under 1000 lines.

use super::*;
use quill::lang::{keys, tr, tr_with};

/// The open thread's window, read once per render: it drives the unread
/// divider, the jump button's badge, forward paging and scroll anchoring
/// the way `HistoryState` does for the main history.
struct ThreadWindow {
    thread_id: i64,
    unread_anchor: Option<MessageId>,
    unread_count: i32,
    has_newer: bool,
    window_epoch: u64,
    /// The first page still loads around the saved scroll position.
    saved_open: bool,
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
        // Build the rows, patch messages changed in place or keep them
        // (`HistoryRowsKey`).
        messages: RowsInput,
        chat: Option<&ChatSummary>,
        sender_name: &str,
        highlight_id: Option<MessageId>,
        media_roots: std::rc::Rc<[PathBuf]>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Where the lone emoji and effect badges are painted is noted again
        // by the rows. Effects play over the main window only, so a chat
        // window's history leaves its notes alone.
        if self.windows.scope.is_none() {
            super::super::emoji_effect::begin_pass();
        }
        let session = self.session();
        let jump_serial = session.map_or(0, |s| s.search.chat_search.jump_serial);
        // Phase B4: secret chats word the timer-change service row as
        // "Self-destruct".
        let is_secret = chat.is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }));
        // kit Phase 3: the scroller resets when the open chat/topic
        // changes. Read the key up front — the `session` borrow must end
        // before the state below is assigned.
        let viewed_chat = self.viewed_chat();
        let viewed_topic = self.viewed_topic();
        let thread_window = session
            .and_then(|s| viewed_chat.and_then(|chat| s.thread_for_chat(chat)))
            .map(|thread| ThreadWindow {
                thread_id: thread.thread_id,
                unread_anchor: thread.unread_anchor,
                unread_count: thread.unread_count,
                has_newer: thread.has_newer,
                window_epoch: thread.window_epoch,
                saved_open: thread.saved_anchor.is_some(),
            });
        let history_key = (
            viewed_chat.map(|chat| chat.0).unwrap_or(-1),
            viewed_topic,
            thread_window.as_ref().map_or(0, |thread| thread.thread_id),
        );
        // The loaded window: the thread's own (it opens around its read
        // position and pages forward like the main history) or the main
        // history's (topic views page their own history and have none of
        // this).
        let (unread_anchor, has_newer, window_epoch) = match &thread_window {
            Some(thread) => (thread.unread_anchor, thread.has_newer, thread.window_epoch),
            None => session
                .filter(|_| viewed_topic.is_none())
                .and_then(|s| {
                    let chat = viewed_chat?;
                    s.histories
                        .get(&chat.0)
                        .map(|h| (h.unread_anchor, s.history_has_newer(chat), h.window_epoch))
                })
                .unwrap_or((None, false, 0)),
        };
        // A history opened at its saved position anchors once the window
        // around that message is in, so the position comes with the first rows.
        let saved_open_pending = match &thread_window {
            Some(thread) => thread.saved_open,
            None => {
                viewed_topic.is_none()
                    && viewed_chat
                        .is_some_and(|chat| session.is_some_and(|s| s.saved_open_pending(chat)))
            }
        };
        let unread_count = match &thread_window {
            Some(thread) => thread.unread_count,
            None => chat.map_or(0, |chat| chat.unread_count),
        };
        let rows_started = std::time::Instant::now();
        let mark = self.chrome_mark();
        let (build, splice, patch) = match messages {
            RowsInput::Build(messages) => (Some(messages), None, Vec::new()),
            RowsInput::Splice(splice) => (None, Some(splice), Vec::new()),
            RowsInput::Patch(patch) => (None, None, patch),
            RowsInput::Keep => (None, None, Vec::new()),
        };
        let cloned = build.as_ref().map_or(0, |list| list.len() as u64);
        let chrome_due = if build.is_none() && splice.is_none() {
            self.chrome_due_rows()
        } else {
            Vec::new()
        };
        let chrome_inputs = super::row_chrome::ChromeInputs {
            session,
            chat,
            sender_name,
            media_roots: &media_roots,
            wide: self.chat_wide(),
        };
        let row_build = super::row_build::RowBuild {
            chrome: &chrome_inputs,
            epoch: mark.epoch,
            highlight: highlight_id,
            unread_anchor,
            is_secret,
        };
        if let Some(mut messages) = build {
            // Deleted messages dissolve in place before they go.
            self.merge_vanishing(&mut messages);
            let (mut rows, seed) = self.build_rows(messages, Default::default(), None, &row_build);
            if let Some(row) = self.streaming_bot_row(&seed, &row_build, cx) {
                rows.push(row);
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
            let media_signature = self.history_media_signature();
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
                // The window of a chat already open was replaced (jump to
                // the latest, a date jump): the position left behind is not
                // where the reader goes back to.
                if self.history.key == Some(history_key) {
                    self.history.scroll_memory.forget(history_key);
                }
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
                        // The appended rows are in the new list; the stored
                        // one is still the old, shorter list.
                        let outgoing =
                            rows.get(prev_count.min(count)..count).is_some_and(|added| {
                                added.iter().any(super::chat_palette::row_is_outgoing)
                            });
                        self.append_tail_rows(prev_count, count, outgoing, cx);
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
            self.history.rows = rows;
            self.history.chrome_epoch = mark.epoch;
            self.history.chrome_seen = mark.revision;
            if let Ok(mut due) = self.history.chrome_due.try_borrow_mut() {
                due.clear();
            }
            self.history.row_stats.note_built(rows_started);
            self.history.row_stats.messages_cloned += cloned;
            self.rows_replaced(history_key, highlight_id, saved_open_pending, has_newer, cx);
        } else if let Some(splice) = splice {
            // Messages came or went: build the rows around them again and
            // keep the rest.
            let spliced = self.splice_rows(splice, &row_build);
            self.commit_spliced_rows(spliced, mark, rows_started, cx);
            self.rows_replaced(history_key, highlight_id, saved_open_pending, has_newer, cx);
        } else {
            // Kept rows: patch the messages changed in place and bring the
            // chrome around the viewport up to date.
            let patches = chrome_inputs.row_patches(&self.history.rows, patch);
            let fresh =
                chrome_inputs.stale_rows(&self.history.rows, &chrome_due, &patches, mark.epoch);
            self.apply_row_patches(patches, fresh, mark, rows_started, cx);
        }
        self.remember_scroll(history_key, cx);
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
        // Those not painted in view this pass pause once it has painted.
        self.playback
            .inline_videos
            .begin_render(self.slices.current(), cx);
        self.playback.userpic_videos.begin_render(
            super::userpic_videos::Surface::History,
            self.slices.current(),
            cx,
        );
        let weak = cx.weak_entity();
        let look = self.chat_look(self.viewed_chat().map(|c| c.0), cx);
        let palette = self.history_palette(&look, cx);
        self.history.palette.set(palette);
        let date_pill =
            super::chat_palette::with_palette(palette, false, || self.scroll_date_pill(cx));
        let probe = self.history.scroll_probe.clone();
        let top_probe = self.history.scroll_top_probe.clone();
        let view_probe = self.history.scroll_view_probe.clone();
        let hit_rows = self.history.hit_rows.clone();
        let viewport = self.history.viewport.clone();
        let history_focus = self.message_ui.history_focus.clone();
        let drag_app = cx.weak_entity();
        let reveal = self.history_reveal(cx);
        // The pull to the next channel lifts the rows over its indicator.
        let (pull_shift, pull_indicator, pull_listener) = self.pull_layer(cx);
        let reveal = match (reveal, pull_shift > 0.) {
            (reveal, false) => reveal,
            (Some((shift, extra)), true) => Some((shift - pull_shift, extra)),
            (None, true) => Some((-pull_shift, 0.)),
        };
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
        // Built for a chat window: the work the rows defer (paging, GIF
        // autoplay, the jump button) runs with that window's state in
        // place (`in_window_scope`).
        let window_chat = self.windows.scope;
        super::selectable_text::selection_viewport(
            super::wallpaper::paint_wallpaper(
                div()
                    .id(id)
                    // Selecting messages gives the rows the keyboard
                    // (`selection_drag`): Space, Up and Down act on them,
                    // typing goes on to the composer.
                    .track_focus(&history_focus)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.history_key_down(event, window, cx);
                    }))
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    // kit Phase 7: screen-reader landmark for the message history.
                    .role(Role::Log)
                    .aria_label(tr_with(
                        keys::Q_CHAT_MESSAGE_HISTORY_2,
                        &[("sender_name", sender_name)],
                    ))
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
                super::wallpaper_cache::Slot::History(cx.entity_id()),
            )
            .children(self.wallpaper_fade(&look, cx))
            .child(super::history_fx::reveal_viewport(
                reveal,
                MessageScroller::new(id, self.history.scroller.clone(), move |ix, _window, cx| {
                    let gif_view = weak.clone();
                    cx.defer(move |cx| {
                        let _ = gif_view.update(cx, |this, cx| {
                            this.in_window_scope(window_chat, cx, |this, cx| {
                                this.maybe_autoplay_gif(ix, cx)
                            })
                        });
                    });
                    if ix == 0 {
                        let weak = weak.clone();
                        cx.defer(move |cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.in_window_scope(window_chat, cx, |this, cx| {
                                    this.maybe_auto_load_older(cx)
                                })
                            });
                        });
                    }
                    // Prefetch the next newer page a few rows before the
                    // window's end so reading on rarely waits.
                    if has_newer && ix + NEWER_PREFETCH_ROWS >= count {
                        let weak = weak.clone();
                        cx.defer(move |cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.in_window_scope(window_chat, cx, |this, cx| {
                                    this.maybe_auto_load_newer(cx)
                                })
                            });
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
                        // tdesktop `lng_jump_to_bottom`; the badge's count
                        // follows the name, as on the other jump buttons.
                        let name = if unread_count > 0 {
                            tr_with(
                                keys::Q_CHAT_JUMP_TO_BOTTOM,
                                &[("unread_count", &unread_count.to_string())],
                            )
                        } else {
                            tr(keys::Q_CHAT_JUMP_TO_BOTTOM_2).to_string()
                        };
                        button
                            .accessibility_label(name)
                            .when(unread_count > 0, |button| {
                                button.label(unread_count.to_string())
                            })
                            .on_click(move |_, _, cx| {
                                let _ = jump.update(cx, |this, cx| {
                                    this.in_window_scope(window_chat, cx, |this, cx| {
                                        this.jump_to_latest_messages(cx)
                                    })
                                });
                            })
                    }
                })
                .size_full()
                .min_h_0(),
            ))
            // Paints after the rows: the first row reaching below the
            // top edge is what the floating date describes. The same pass
            // keeps the rows' bounds for drag selection and listens to the
            // window's pointer for it: a drag runs over rows whose
            // selection overlays block hover, so element listeners on the
            // list would miss it.
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        // Take the rows out so no guard is held across the
                        // calls below; a re-entrant borrow skips one frame.
                        let Ok(mut slot) = probe.try_borrow_mut() else {
                            return;
                        };
                        let mut rows = std::mem::take(&mut *slot);
                        drop(slot);
                        reveal_probe.note(&rows, count, bounds.top());
                        if let Ok(mut hits) = hit_rows.try_borrow_mut() {
                            *hits = rows.iter().map(|(ix, row, _)| (*ix, *row)).collect();
                        }
                        viewport.set(Some(bounds));
                        let moved = drag_app.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase.bubble() && event.pressed_button == Some(MouseButton::Left) {
                                let _ = moved.update(cx, |this, cx| {
                                    if this.message_ui.selection_drag.is_some() {
                                        this.selection_drag_pointer(event.position, window, cx);
                                    }
                                });
                            }
                        });
                        let released = drag_app.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase.bubble() && event.button == MouseButton::Left {
                                let position = event.position;
                                let _ = released.update(cx, |this, cx| {
                                    this.selection_release(position, window, cx)
                                });
                            }
                        });
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
                        // Hand the buffer back to reuse its allocation.
                        if let Ok(mut slot) = probe.try_borrow_mut()
                            && slot.is_empty()
                        {
                            *slot = rows;
                        }
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
            .children(pull_indicator)
            .child(pull_listener)
            .children(date_pill)
            .child(self.history_width_probe(cx))
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
        // A row scrolled into view that hasn't been reported yet: redraw
        // the conversation (its window's view) so `report_visible_history`
        // sees it promptly (list scrolling alone doesn't re-render the app
        // view). The other slices keep their frame.
        self.note_stale_chrome(ix, row, cx);
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
            self.notify_tick(crate::ui::app_slice::SliceKind::Conversation, cx);
        }
        let palette = self.history.palette.get();
        let outgoing = super::chat_palette::row_is_outgoing(row);
        let element = super::chat_palette::with_palette(palette, outgoing, || {
            self.render_history_row_body(row, cx)
        });
        let has_day = row.day_label().is_some();
        // The wide layout centres dates and the unread label in its column.
        let wide = self.chat_wide();
        let probe = self.history.scroll_probe.clone();
        // Notes where the row is painted, for the floating date pill.
        let tracker = canvas(
            |_, _, _| {},
            move |bounds, _, _, _| {
                // A re-entrant borrow drops this row's probe for one frame.
                if let Ok(mut rows) = probe.try_borrow_mut() {
                    rows.push((ix, bounds, has_day));
                }
            },
        )
        .absolute()
        .inset_0()
        .size_full();
        // Screen-reader focus (`history_a11y`): the focused message, or the
        // unread bar, is the focused history's active descendant.
        let (row_focused, bar_focused) = self.history_a11y_row_focus(ix);
        let a11y_focus = row_focused.then(|| row.first_id()).flatten().map(|id| {
            (
                super::super::history_a11y::row_element_id(id),
                self.history_row_a11y_name(row),
            )
        });
        let element = match a11y_focus {
            Some((id, name)) => div()
                .id(id)
                .role(Role::ListItem)
                .aria_label(name)
                .aria_active_descendant()
                .flex()
                .flex_col()
                .child(element)
                .into_any_element(),
            None => element,
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .child(tracker)
            .when_some(row.day_label(), |this, label| {
                this.child(super::chat_palette::with_palette(palette, false, || {
                    super::chat_wide_ui::wide_column(wide, day_separator(label, cx))
                }))
            })
            .when(row.unread_divider(), |this| {
                let viewed = self.viewed_chat();
                let unread_at_open = self
                    .session()
                    .and_then(|s| viewed.and_then(|chat| s.histories.get(&chat.0)))
                    .map_or(0, |h| h.unread_at_open);
                this.child(super::chat_palette::with_palette(palette, false, || {
                    unread_divider(unread_at_open, wide, bar_focused, cx)
                }))
            })
            .child(element)
            .into_any_element()
    }
}

impl QuillApp {
    /// New rows at the tail: tail-follow keeps the view pinned when the
    /// user is at the bottom. A newer page must not be skipped that way:
    /// keep the old last row in view and read on from there. `outgoing`:
    /// one of the new rows is the user's own.
    pub(super) fn append_tail_rows(
        &mut self,
        prev_count: usize,
        count: usize,
        outgoing: bool,
        cx: &mut Context<Self>,
    ) {
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
        // tdesktop reveals a few new rows at the bottom of a history that
        // is pinned there (`itemRevealDuration`); an inactive window just
        // snaps.
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
        // A sent message turns a freeform wallpaper.
        if following && !newer_page && outgoing {
            self.rotate_wallpaper_on_send(cx);
        }
    }

    /// After rows were built or spliced: note the list's ends, anchor a
    /// new window and scroll to a new jump target.
    fn rows_replaced(
        &mut self,
        history_key: quill::state::ScrollKey,
        highlight_id: Option<MessageId>,
        saved_open_pending: bool,
        has_newer: bool,
        cx: &mut Context<Self>,
    ) {
        let rows = &self.history.rows;
        self.history.ends = rows
            .first()
            .and_then(HistoryRow::first_id)
            .zip(rows.last().and_then(HistoryRow::last_id));
        self.history.had_newer = has_newer;
        self.history_a11y_rows_changed(cx);
        // A new or replaced window: start at the saved position, else the
        // jump target, else the "Unread messages" divider, else the bottom.
        if self.history.anchor_pending && !self.history.rows.is_empty() && !saved_open_pending {
            self.history.anchor_pending = false;
            self.anchor_opened_history(history_key, highlight_id, cx);
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
}
