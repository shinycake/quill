//! Inline autoplay of videos and GIFs in the history, as in Telegram
//! Desktop: each visible, downloaded clip plays muted and looped on the
//! native player (`NativeVideo`), drawn straight from its pixel buffers.
//! Players live while their row renders and are dropped a couple of
//! renders after it scrolls away (or the chat changes). A round video
//! message plays once with sound when clicked, then loops muted again.

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
    /// Playing with sound (a clicked round video message).
    pub(super) sound: bool,
    /// The history's background, for masks drawn over the video.
    pub(super) backdrop: Hsla,
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
    sound: bool,
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

    /// Play a running clip once from the start with sound (muting any
    /// other), or back to its muted loop. False when it isn't running.
    #[cfg(target_os = "macos")]
    pub(super) fn toggle_sound(&mut self, chat_id: i64, message_id: i64) -> bool {
        let key = (chat_id, message_id);
        let Some(on) = self.players.get(&key).map(|slot| !slot.sound) else {
            return false;
        };
        for (other, slot) in &mut self.players {
            if *other != key && slot.sound {
                slot.sound = false;
                slot.video.set_volume(0.0);
            }
        }
        if let Some(slot) = self.players.get_mut(&key) {
            slot.sound = on;
            slot.video.set_volume(if on { 1.0 } else { 0.0 });
            if on {
                slot.video.seek(0.0);
                slot.video.play();
            }
        }
        true
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn toggle_sound(&mut self, _chat_id: i64, _message_id: i64) -> bool {
        false
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
                    sound: false,
                })
            }
        };
        slot.seen = render;
        if slot.video.error().is_some() {
            return None;
        }
        // Loop: `play` rewinds a clip that reached its end. A clip played
        // with sound goes back to its muted loop.
        if !slot.video.is_playing() {
            if slot.sound {
                slot.sound = false;
                slot.video.set_volume(0.0);
            }
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
            sound: slot.sound,
            backdrop: gpui_kit::black(),
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
            MessageContent::VideoNote(note) if prefs.autoplay_videos && !note.is_secret => {
                note.play_file_id()?
            }
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

/// An image of `color` with a transparent, anti-aliased circle cut out of
/// it: laid over a square video it rounds the video, which GPUI's surface
/// can't clip itself. Cached per size and color.
pub(super) fn circle_mask(edge: u32, color: Hsla) -> std::sync::Arc<RenderImage> {
    use std::cell::RefCell;
    use std::sync::Arc;
    thread_local! {
        static MASKS: RefCell<HashMap<(u32, [u8; 4]), Arc<RenderImage>>> = RefCell::new(HashMap::new());
    }
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let bgra = [
        channel(rgba.b),
        channel(rgba.g),
        channel(rgba.r),
        channel(rgba.a),
    ];
    MASKS.with(|masks| {
        masks
            .borrow_mut()
            .entry((edge, bgra))
            .or_insert_with(|| {
                let radius = edge as f32 / 2.0;
                let mut image = image::RgbaImage::new(edge, edge);
                for (x, y, pixel) in image.enumerate_pixels_mut() {
                    let dx = x as f32 + 0.5 - radius;
                    let dy = y as f32 + 0.5 - radius;
                    // 0 inside the circle, 1 outside, a pixel's ramp between.
                    let cover = ((dx * dx + dy * dy).sqrt() - radius + 0.5).clamp(0.0, 1.0);
                    let alpha = (f32::from(bgra[3]) * cover).round() as u8;
                    *pixel = image::Rgba([bgra[0], bgra[1], bgra[2], alpha]);
                }
                Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
                    image::Frame::new(image),
                ])))
            })
            .clone()
    })
}

#[cfg(test)]
mod tests {
    use super::circle_mask;

    #[test]
    fn circle_mask_covers_corners_and_clears_the_circle() {
        let mask = circle_mask(40, gpui_kit::white());
        let bytes = mask.as_bytes(0).expect("one frame");
        let alpha = |x: usize, y: usize| bytes[(y * 40 + x) * 4 + 3];
        // Corners keep the backdrop; the middle and the edge midpoints
        // show the video.
        assert_eq!(alpha(0, 0), 255);
        assert_eq!(alpha(39, 39), 255);
        assert_eq!(alpha(20, 20), 0);
        assert_eq!(alpha(20, 1), 0);
        // White stays white (straight BGRA).
        assert_eq!(&bytes[0..3], &[255, 255, 255]);
    }
}
