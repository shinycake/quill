//! Opening, closing and stepping through the viewer, and resolving the
//! files it shows.

use super::*;

impl QuillApp {
    /// Phase 4.5: open the fullscreen media viewer on the clicked message.
    /// Items are the chat's photo/video messages (oldest first); the clicked
    /// message becomes the current item. When nothing viewable is local yet
    /// the viewer shows a loading state and `downloadFile` is triggered —
    /// the 40ms poll loop re-renders when `updateFile` lands.
    pub(in crate::ui) fn open_media_viewer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
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
        let index = items.iter().position(|item| item.message_id == message_id);
        // The clicked message may not be viewer-openable (e.g. an album
        // tile for a non-photo/video part) — then stay closed instead of
        // opening on an unrelated item.
        let Some(index) = index else {
            return;
        };
        self.viewer.state = MediaViewer::open(items, index);
        self.viewer.open_gen += 1;
        self.viewer_note_activity(false, cx);
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
        let tab_state = &state.tabs[tab.index()];
        // The panel lists newest first; the viewer pages oldest first.
        let messages: Vec<HistoryMessage> = tab_state
            .items
            .iter()
            .rev()
            .filter_map(|item| item.message.as_deref().cloned())
            .collect();
        let loaded = tab_state.items.len();
        let total = tab_state.total_count.max(0) as usize;
        let items = collect_media_items(&messages);
        let Some(index) = items.iter().position(|item| item.message_id == message_id) else {
            return false;
        };
        debug_assert!(items.iter().all(|item| item.chat_id == chat_id));
        self.viewer.state = MediaViewer::open_shared(items, index, total);
        self.viewer.extra.shared_tab = tab;
        self.viewer.extra.shared_seen = loaded;
        self.viewer.open_gen += 1;
        self.viewer_note_activity(false, cx);
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
            let state = &session.media.shared_media.tabs[tab.index()];
            (state.items.len() != seen).then(|| {
                let messages: Vec<HistoryMessage> = state
                    .items
                    .iter()
                    .rev()
                    .filter_map(|item| item.message.as_deref().cloned())
                    .collect();
                (
                    collect_media_items(&messages),
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
        // MED1: orientation and playback error are per-item state.
        self.viewer.orientation = Default::default();
        self.viewer.rotated = None;
        self.playback.error = None;
        self.viewer.extra.inactive_paused = false;
        self.stop_viewer_video();
        self.ensure_viewer_download(cx);
        self.maybe_autoplay_viewer_video(cx);
        // What TDLib allows for this message (Delete in the toolbar).
        // Profile photos are not messages.
        if self.viewer.state.source() != ViewerSource::Profile
            && let (Some(item), Some(live)) = (self.viewer.state.current(), self.live.as_mut())
        {
            let _ = live
                .driver
                .fetch_message_menu_actions(item.chat_id, item.message_id);
        }
    }

    pub(in crate::ui) fn close_media_viewer(&mut self, cx: &mut Context<Self>) {
        self.viewer_leave_video_fullscreen();
        self.stop_viewer_video();
        self.viewer.extra.saved_toast = None;
        self.viewer.state.close();
        cx.notify();
    }

    pub(in crate::ui) fn step_media_viewer(&mut self, delta: i32, cx: &mut Context<Self>) {
        if delta < 0 {
            self.viewer.state.prev();
        } else {
            self.viewer.state.next();
        }
        self.viewer_note_activity(false, cx);
        self.reset_viewer_item_state(cx);
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
