//! Opening, closing and stepping through the viewer, and resolving the
//! files it shows.

use super::*;
use std::time::Instant;

impl QuillApp {
    /// Phase 4.5: open the fullscreen media viewer on the clicked message.
    /// Items are the chat's photo/video messages (oldest first); the clicked
    /// message becomes the current item. When nothing viewable is local yet
    /// the viewer shows a loading state and `downloadFile` is triggered —
    /// the poll loop re-renders when `updateFile` lands.
    pub(in crate::ui) fn open_media_viewer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        self.open_media_viewer_at(chat_id, message_id, 0, cx);
    }

    /// Like [`Self::open_media_viewer`], on the `offset`-th item of a
    /// message that holds several (a bought paid-media album).
    pub(in crate::ui) fn open_media_viewer_at(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        offset: usize,
        cx: &mut Context<Self>,
    ) {
        // A photo or video with a self-destruct timer opens alone and
        // starts its timer (tdesktop `markTimedMediaRead`).
        let timed = self
            .session()
            .and_then(|session| {
                session
                    .histories
                    .get(&chat_id.0)?
                    .messages
                    .get(&message_id.0)
            })
            .and_then(quill::media_viewer::timed_viewer_item);
        if let Some(item) = timed {
            self.viewer.state = MediaViewer::open(vec![item], 0);
            self.viewer.open_gen += 1;
            self.mark_timed_media_read(chat_id, message_id);
            self.viewer_note_activity(cx);
            self.reset_viewer_item_state(cx);
            cx.notify();
            return;
        }
        let items = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .map(|history| {
                collect_media_items(&history.ordered().into_iter().cloned().collect::<Vec<_>>())
            })
            .unwrap_or_default();
        if items.is_empty() {
            return;
        }
        let index = items
            .iter()
            .position(|item| item.message_id == message_id)
            .map(|first| {
                // `first` came from `position`, so the tail is never empty
                // and `run` is at least 1 (the 1 fallback is unreachable).
                let run = items.get(first..).map_or(1, |tail| {
                    tail.iter()
                        .take_while(|item| item.message_id == message_id)
                        .count()
                });
                first + offset.min(run.saturating_sub(1))
            });
        // The clicked message may not be viewer-openable (e.g. an album
        // tile for a non-photo/video part) — then stay closed instead of
        // opening on an unrelated item.
        let Some(index) = index else {
            return;
        };
        self.viewer.state = MediaViewer::open(items, index);
        self.viewer.open_gen += 1;
        self.viewer_note_activity(cx);
        self.reset_viewer_item_state(cx);
        cx.notify();
    }

    /// Open the viewer on the picture or clip of a poll option. The item is
    /// not a message of its own, so the message actions stay out of it.
    pub(in crate::ui) fn open_poll_option_viewer(
        &mut self,
        item: MediaViewerItem,
        cx: &mut Context<Self>,
    ) {
        self.viewer.state = MediaViewer::open_poll(item);
        self.viewer.open_gen += 1;
        self.viewer_note_activity(cx);
        self.reset_viewer_item_state(cx);
        cx.notify();
    }

    /// Open the viewer on a photo, video or GIF of the Shared Media panel,
    /// paging over that tab's list (tdesktop `SharedMediaWithLastSlice`).
    /// The list loads older pages as the user pages toward its start.
    /// `false` when the message is not viewer-openable (a secret photo):
    /// the caller then jumps to it in the chat.
    pub(in crate::ui) fn open_shared_media_viewer(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let state = &session.media.shared_media;
        let Some(chat_id) = state.chat_id else {
            return false;
        };
        let tab = state.active_tab;
        let Some(tab_state) = state.tabs.get(tab.index()) else {
            return false;
        };
        // The panel lists newest first; the viewer pages oldest first.
        let messages: Vec<HistoryMessage> = tab_state
            .items
            .iter()
            .rev()
            .filter_map(|item| item.message.as_deref().cloned())
            .collect();
        let loaded = tab_state.items.len();
        let total = tab_state.total_count.max(0) as usize;
        let items = shared_tab_items(tab, &messages);
        let Some(index) = items.iter().position(|item| item.message_id == message_id) else {
            return false;
        };
        // In Files, only a picture opens the viewer (tdesktop
        // `ResolveDocument`); other files keep jumping to their message.
        if tab == SharedMediaTab::Files
            && items.get(index).map(|item| item.kind) != Some(MediaViewerKind::ImageFile)
        {
            return false;
        }
        debug_assert!(items.iter().all(|item| item.chat_id == chat_id));
        self.viewer.state = MediaViewer::open_shared(items, index, total);
        self.viewer.extra.shared_tab = tab;
        self.viewer.extra.shared_seen = loaded;
        self.viewer.open_gen += 1;
        self.viewer_note_activity(cx);
        self.reset_viewer_item_state(cx);
        self.sync_shared_media_viewer(cx);
        cx.notify();
        true
    }

    /// Keep a Shared Media viewer in step with its tab: merge older pages
    /// that landed, and ask for the next one when the viewer nears the
    /// start of the loaded list.
    pub(in crate::ui) fn sync_shared_media_viewer(&mut self, cx: &mut Context<Self>) {
        if self.viewer.state.source() != ViewerSource::SharedMedia {
            return;
        }
        let tab = self.viewer.extra.shared_tab;
        let seen = self.viewer.extra.shared_seen;
        let grown = self.session().and_then(|session| {
            let state = session.media.shared_media.tabs.get(tab.index())?;
            (state.items.len() != seen).then(|| {
                let messages: Vec<HistoryMessage> = state
                    .items
                    .iter()
                    .rev()
                    .filter_map(|item| item.message.as_deref().cloned())
                    .collect();
                (
                    shared_tab_items(tab, &messages),
                    state.total_count.max(0) as usize,
                    state.items.len(),
                )
            })
        });
        if let Some((items, total, loaded)) = grown {
            self.viewer.extra.shared_seen = loaded;
            if self.viewer.state.merge_older(items, total) > 0 {
                cx.notify();
            }
        }
        if self.viewer.state.wants_older()
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.fetch_more_shared_media(tab);
        }
    }

    /// Per-item viewer state: zoom, orientation, playback error, video
    /// and the download / delete-permission lookups for the new current
    /// item. Shared by open, step, and "the current item was deleted".
    pub(in crate::ui) fn reset_viewer_item_state(&mut self, cx: &mut Context<Self>) {
        self.viewer.zoom.reset();
        self.viewer.drag = None;
        self.viewer.wheel.new_item();
        self.viewer.swipe.new_item();
        self.viewer_leave_video_fullscreen();
        // MED1: orientation and playback error are per-item state.
        self.viewer.orientation = Default::default();
        self.viewer.rotated = None;
        self.viewer.cancel_reorient();
        self.viewer.hdr.reset();
        self.playback.error = None;
        self.viewer.extra.share_at = None;
        if let Some(item) = self.viewer.state.current()
            && let Some(turns) = self
                .viewer
                .extra
                .rotations
                .get(&(item.chat_id.0, item.message_id.0))
        {
            self.viewer.orientation.turns = *turns;
        }
        self.apply_viewer_quality_pref();
        self.viewer.extra.inactive_paused = false;
        self.stop_viewer_video();
        self.ensure_viewer_download(cx);
        self.download_viewer_strip_thumbs();
        self.maybe_autoplay_viewer_video(cx);
        // What TDLib allows for this message (Delete in the toolbar).
        // Profile photos and poll option media are not messages; an
        // earlier group or channel photo has its service message.
        self.ask_profile_photo_message_actions();
        if !matches!(
            self.viewer.state.source(),
            ViewerSource::Profile | ViewerSource::Poll
        ) && let (Some(item), Some(live)) = (self.viewer.state.current(), self.live.as_mut())
        {
            let _ = live
                .driver
                .fetch_message_menu_actions(item.chat_id, item.message_id);
        }
    }

    /// A click on the black around the media. In video full screen it is
    /// part of the picture (tdesktop forces `Over::Video` there), so it
    /// toggles play instead of closing.
    pub(in crate::ui) fn viewer_backdrop_click(&mut self, cx: &mut Context<Self>) {
        if self.viewer.extra.video_fullscreen.is_on() {
            self.toggle_viewer_video(cx);
        } else {
            self.close_media_viewer(cx);
        }
    }

    pub(in crate::ui) fn close_media_viewer(&mut self, cx: &mut Context<Self>) {
        self.viewer_leave_video_fullscreen();
        self.stop_viewer_video();
        self.viewer.extra.saved_toast = None;
        // Reopening the same item starts with its caption collapsed.
        self.viewer.extra.caption = Default::default();
        self.viewer.state.close();
        self.viewer.cancel_reorient();
        // A turned photo is a full-size picture of its own (up to about
        // 20 MB, and as much again on the GPU); opening the next item
        // replaced it, but nothing did until then.
        self.viewer.rotated = None;
        self.viewer.hdr.reset();
        self.viewer.over_controls = false;
        cx.notify();
    }

    pub(in crate::ui) fn step_media_viewer(&mut self, delta: i32, cx: &mut Context<Self>) {
        if delta < 0 {
            self.viewer.state.prev();
        } else {
            self.viewer.state.next();
        }
        self.viewer_note_activity(cx);
        self.reset_viewer_item_state(cx);
        cx.notify();
    }

    /// A wheel or trackpad scroll over the viewer moves to the next or
    /// previous item, or zooms at the pointer (`WheelNav`). `origin` is
    /// the window position of the visual's top-left.
    pub(in crate::ui) fn viewer_scroll_wheel(
        &mut self,
        event: &ScrollEvent,
        origin: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let (dx, dy, source) = match event.delta {
            ScrollDelta::Pixels(p) => (
                f32::from(p.x),
                f32::from(p.y),
                WheelSource::Trackpad {
                    begins_gesture: event.touch_phase == TouchPhase::Started,
                },
            ),
            ScrollDelta::Lines(l) => (
                l.x * VIEWER_WHEEL_NOTCH_PX,
                l.y * VIEWER_WHEEL_NOTCH_PX,
                WheelSource::Wheel,
            ),
        };
        self.viewer_note_activity(cx);
        let at = cx.background_executor().now();
        if self.viewer_swipe(dx, dy, source, event.touch_phase, at, cx) {
            return;
        }
        let action = self.viewer.wheel.feed(WheelInput {
            dx,
            dy,
            primary: event.modifiers.secondary(cx),
            source,
            zoomed: self.viewer.zoom.is_zoomed(),
            at,
        });
        match action {
            WheelAction::None => {}
            WheelAction::Zoom { delta_y } => {
                self.viewer_zoom_scroll(delta_y, viewer_anchor(event.position, origin), cx);
            }
            WheelAction::Next if self.viewer.state.has_next() => self.step_media_viewer(1, cx),
            WheelAction::Previous if self.viewer.state.has_prev() => {
                self.step_media_viewer(-1, cx);
            }
            WheelAction::Next | WheelAction::Previous => {}
        }
    }

    /// A horizontal trackpad swipe pulls the next or previous arrow in from
    /// the edge, and letting go once it is fully in moves one item
    /// (`SwipeNav`). `true` when the event belongs to the swipe, so
    /// `WheelNav` must not see it (tdesktop `_swipeNavigating`).
    fn viewer_swipe(
        &mut self,
        dx: f32,
        dy: f32,
        source: WheelSource,
        phase: TouchPhase,
        at: Instant,
        cx: &mut Context<Self>,
    ) -> bool {
        let had_timeout = self.viewer.swipe.timeout_at().is_some();
        let action = self.viewer.swipe.feed(SwipeInput {
            dx,
            dy,
            source,
            at,
            can_next: self.viewer.state.has_next(),
            can_prev: self.viewer.state.has_prev(),
        });
        let mut step = match action {
            SwipeAction::None => None,
            SwipeAction::Capture => {
                cx.notify();
                None
            }
            SwipeAction::Step(dir) => Some(dir),
        };
        match phase {
            TouchPhase::Ended => {
                let released = self.viewer.swipe.end(at);
                step = step.or(released);
            }
            // The system took the touch: it never committed.
            TouchPhase::Cancelled => {
                self.viewer.swipe.end(at);
            }
            TouchPhase::Started | TouchPhase::Moved => {}
        }
        if let Some(dir) = step {
            self.step_viewer_swipe(dir, cx);
        }
        if !had_timeout && self.viewer.swipe.timeout_at().is_some() {
            self.arm_viewer_swipe_timeout(cx);
        }
        action != SwipeAction::None
    }

    fn step_viewer_swipe(&mut self, dir: SwipeDir, cx: &mut Context<Self>) {
        match dir {
            SwipeDir::Next if self.viewer.state.has_next() => self.step_media_viewer(1, cx),
            SwipeDir::Previous if self.viewer.state.has_prev() => self.step_media_viewer(-1, cx),
            SwipeDir::Next | SwipeDir::Previous => cx.notify(),
        }
    }

    /// A swipe without phases (Wayland, X11, some Windows drivers) ends
    /// after the trackpad pause, since no end phase will come.
    fn arm_viewer_swipe_timeout(&mut self, cx: &mut Context<Self>) {
        self.spawn_scoped(cx, async move |this, cx| {
            let mut wait = VIEWER_TRACKPAD_GESTURE_GAP;
            loop {
                cx.background_executor().timer(wait).await;
                let again = this
                    .update(cx, |this, cx| {
                        let deadline = this.viewer.swipe.timeout_at()?;
                        let now = cx.background_executor().now();
                        if now < deadline {
                            return Some(deadline.saturating_duration_since(now));
                        }
                        match this.viewer.swipe.end(now) {
                            Some(dir) => this.step_viewer_swipe(dir, cx),
                            None => cx.notify(),
                        }
                        None
                    })
                    .ok()
                    .flatten();
                match again {
                    Some(left) => wait = left,
                    None => break,
                }
            }
        })
        .detach();
    }

    /// A trackpad pinch zooms at the event position (tdesktop
    /// `handleNativeGesture`).
    pub(in crate::ui) fn viewer_pinch(
        &mut self,
        event: &PinchEvent,
        origin: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        self.viewer_note_activity(cx);
        self.viewer.zoom.zoom_at(
            pinch_zoom_factor(event.delta),
            viewer_anchor(event.position, origin),
            self.viewer.frame,
        );
        cx.notify();
    }

    /// Trigger `downloadFile` for the current viewer item when no display
    /// candidate is local yet (photo: largest size; video: thumbnail, else
    /// the clip itself). Reuses `request_media_download`; no live request
    /// happens in demo mode (it only sets a status note).
    pub(in crate::ui) fn ensure_viewer_download(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        // A file downloads only when opened as a picture Quill shows (a
        // thumbnail being local is not enough), and only under tdesktop's
        // `kReadBytesLimit`; other files wait for the card's Download.
        if matches!(
            item.kind,
            MediaViewerKind::ImageFile | MediaViewerKind::File
        ) {
            let file = self
                .session()
                .and_then(|s| s.media.files.get(&item.download_file_id.0));
            let wanted = item.kind == MediaViewerKind::ImageFile
                && file.is_none_or(|file| {
                    file.usable_path().is_none()
                        && file.display_size() <= crate::ui::image_decode::READ_BYTES_LIMIT
                });
            if wanted {
                self.request_media_download(item.download_file_id, None, cx);
            }
            return;
        }
        let roots = self.media_display_roots();
        let local = self.session().is_some_and(|session| {
            item.display_file_ids.iter().any(|id| {
                session
                    .media
                    .files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
                    .is_some()
            })
        });
        if !local {
            self.request_media_download(item.download_file_id, None, cx);
        }
    }

    /// Parity slice 5: in-viewer video playback. The clip's frames are
    /// extracted with ffmpeg, pre-decoded into GPUI image handles, and
    /// rendered in-place in the viewer overlay (no GPUI video element in
    /// this stack — same frame-cycling approach as the row video preview,
    /// but full-clip); the audio engine plays the audio track. The
    /// overlay keeps Play/Pause and elapsed/total; the thumbnail shows
    /// until frames are ready. Closing or stepping the viewer stops
    /// playback and drops the frame cache.
    ///
    /// Sandbox-checked local path of the current item's full video clip.
    pub(in crate::ui) fn viewer_clip_path(&self, item: &MediaViewerItem) -> Option<PathBuf> {
        self.playable_clip_path(item.chat_id, item.message_id, item.play_file_id?)
    }

    /// A message's clip as a local path the player may open: inside the
    /// account's media folders, or for a video you sent, the original file
    /// you picked (TDLib's local copy of an upload, which it won't download
    /// again). Like Telegram Desktop, that original plays while it exists.
    pub(in crate::ui) fn playable_clip_path(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        play_id: FileId,
    ) -> Option<PathBuf> {
        let roots = self.media_display_roots();
        let session = self.session()?;
        let path = session.media.files.get(&play_id.0)?.usable_path()?;
        if let Some(path) = sandboxed_display_path(path, &roots) {
            return Some(path.to_path_buf());
        }
        let outgoing = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.is_outgoing);
        outgoing
            .then(|| std::fs::canonicalize(path).ok())
            .flatten()
            .filter(|path| path.is_file())
    }
}

/// The viewer items of a Shared Media tab: the files of Files, the photos,
/// videos and GIFs of the others.
fn shared_tab_items(tab: SharedMediaTab, messages: &[HistoryMessage]) -> Vec<MediaViewerItem> {
    if tab == SharedMediaTab::Files {
        quill::media_viewer::collect_file_items(messages)
    } else {
        collect_media_items(messages)
    }
}

/// A window point relative to the visual's top-left (`origin`).
fn viewer_anchor(position: Point<Pixels>, origin: (f32, f32)) -> (f32, f32) {
    (
        f32::from(position.x) - origin.0,
        f32::from(position.y) - origin.1,
    )
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "wheel_ui_tests.rs"]
pub(super) mod wheel_ui_tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "zoom_ui_tests.rs"]
mod zoom_ui_tests;
