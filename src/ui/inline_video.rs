//! Inline autoplay of videos and GIFs in the history, as in Telegram
//! Desktop: each visible, downloaded clip plays muted and looped on the
//! native player (`NativeVideo`), drawn straight from its pixel buffers.
//! Players live while their row renders and are dropped a couple of
//! renders after it scrolls away (or the chat changes).

use super::app::QuillApp;
use gpui_kit::*;
use quill::state::HistoryMessage;
use quill::telegram::envelope::MessageContent;
use std::collections::HashMap;
use std::path::PathBuf;

/// `(chat id, message id)`.
type Key = (i64, i64);

/// A frame for an inline tile, and the clip's remaining time.
pub(super) struct InlineFrame {
    #[cfg(target_os = "macos")]
    pub(super) buffer: core_video::pixel_buffer::CVPixelBuffer,
    pub(super) remaining_secs: Option<f64>,
}

#[derive(Default)]
pub(super) struct InlineVideos {
    #[cfg(target_os = "macos")]
    players: HashMap<Key, Slot>,
    #[cfg(not(target_os = "macos"))]
    players: HashMap<Key, ()>,
    render: u64,
}

#[cfg(target_os = "macos")]
struct Slot {
    video: super::native_video::NativeVideo,
    seen: u64,
}

impl InlineVideos {
    /// Start a render pass; players whose rows didn't render in the last
    /// pass stop.
    pub(super) fn begin_render(&mut self) {
        self.render += 1;
        #[cfg(target_os = "macos")]
        {
            let render = self.render;
            self.players.retain(|_, slot| slot.seen + 1 >= render);
        }
    }

    /// Stop everything (viewer opened, autoplay turned off).
    pub(super) fn clear(&mut self) {
        self.players.clear();
    }

    /// The current frame of this message's clip, starting its muted,
    /// looping player on first use (`path` is only resolved then).
    #[cfg(target_os = "macos")]
    pub(super) fn frame(
        &mut self,
        chat_id: i64,
        message_id: i64,
        path: impl FnOnce() -> Option<PathBuf>,
    ) -> Option<InlineFrame> {
        let render = self.render;
        let slot = match self.players.entry((chat_id, message_id)) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut video = super::native_video::NativeVideo::open(&path()?).ok()?;
                video.set_volume(0.0);
                video.play();
                entry.insert(Slot {
                    video,
                    seen: render,
                })
            }
        };
        slot.seen = render;
        if slot.video.error().is_some() {
            return None;
        }
        // Loop: `play` rewinds a clip that reached its end.
        if !slot.video.is_playing() {
            slot.video.play();
        }
        let buffer = slot.video.frame()?;
        let remaining_secs = slot
            .video
            .duration_secs()
            .map(|total| (total - slot.video.position_secs()).max(0.0));
        Some(InlineFrame {
            buffer,
            remaining_secs,
        })
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn frame(
        &mut self,
        _chat_id: i64,
        _message_id: i64,
        _path: &Path,
    ) -> Option<InlineFrame> {
        None
    }
}

impl QuillApp {
    /// The inline frame for a history row's video or GIF, when it should
    /// autoplay: the native player is available, autoplay is on for its
    /// kind, data saver is off, it isn't secret or behind a spoiler, the
    /// clip is downloaded, and no viewer covers the chat.
    pub(super) fn inline_frame(
        &self,
        message: &HistoryMessage,
        cx: &mut Context<Self>,
    ) -> Option<InlineFrame> {
        if !super::native_video::SUPPORTED
            || self.media_viewer.is_open()
            || self.story_viewer.is_open()
        {
            return None;
        }
        let session = self.session()?;
        let prefs = &session.media_prefs;
        if prefs.data_saver {
            return None;
        }
        let file_id = match &message.content {
            MessageContent::Video(video)
                if prefs.autoplay_videos && !video.is_secret && !video.has_spoiler =>
            {
                video.play_file_id()?
            }
            // AVFoundation plays Telegram's MP4 GIFs; true `image/gif`
            // files keep the frame-extraction path.
            MessageContent::Animation(animation)
                if prefs.autoplay_gifs
                    && !animation.is_secret
                    && !animation.has_spoiler
                    && animation.mime_type != "image/gif" =>
            {
                animation.play_file_id()?
            }
            _ => return None,
        };
        if session.files.get(&file_id.0)?.usable_path().is_none() {
            return None;
        }
        let (chat_id, message_id) = (message.chat_id, message.id);
        let frame = self
            .inline_videos
            .borrow_mut()
            .frame(chat_id.0, message_id.0, || {
                self.playable_clip_path(chat_id, message_id, file_id)
            });
        if frame.is_some() {
            self.request_animation_tick(30, cx);
        }
        frame
    }
}
