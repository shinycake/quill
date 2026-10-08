//! Inline autoplay of videos and GIFs in the history, as in Telegram
//! Desktop: each visible, downloaded clip plays muted and looped on the
//! native player (`NativeVideo`), drawn straight from its pixel buffers.
//! Players live while their row renders and are dropped a couple of
//! renders after it scrolls away (or the chat changes). A round video
//! message plays once with sound when clicked, then loops muted again.

use super::app::QuillApp;
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use quill::state::HistoryMessage;
use quill::telegram::envelope::MessageContent;
use std::collections::HashMap;
use std::path::PathBuf;

/// Largest clip fetched in the background so it can autoplay.
const AUTOPLAY_PREFETCH_MAX: i64 = 20 * 1024 * 1024;

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
    /// Position through the clip, `0.0..=1.0`, once its duration is known.
    pub(super) progress: Option<f32>,
    /// A round video message's seek ring: how far it has sprung out
    /// (0 hidden, 1 shown, briefly above 1 while springing) and how far
    /// its dot has grown under a drag.
    pub(super) seek_shown: f32,
    pub(super) seek_grabbed: f32,
}

/// tdesktop `VideoMessageSeek`: the ring shows in 220 ms (ease-out-back),
/// hides in 150 ms, and the dot grows or shrinks in 150 ms.
const SEEK_SHOW: f32 = 0.22;
const SEEK_HIDE: f32 = 0.15;
const SEEK_GRAB: f32 = 0.15;

/// `anim::easeOutBack`: overshoots a little, then settles.
fn ease_out_back(t: f32) -> f32 {
    const S: f32 = 1.70158;
    let t = t - 1.0;
    t * t * ((S + 1.0) * t + S) + 1.0
}

/// A value that eases between 0 and 1 when its target flips.
#[derive(Clone, Copy)]
struct Toggle {
    on: bool,
    since: std::time::Instant,
    from: f32,
}

impl Toggle {
    fn new() -> Self {
        Self {
            on: false,
            since: std::time::Instant::now(),
            from: 0.0,
        }
    }

    /// Point it at `on`, starting from wherever it is now.
    fn set(&mut self, on: bool, show: f32, hide: f32, spring: bool) {
        if self.on != on {
            self.from = self.value(show, hide, spring).clamp(0.0, 1.0);
            self.on = on;
            self.since = std::time::Instant::now();
        }
    }

    fn value(&self, show: f32, hide: f32, spring: bool) -> f32 {
        let elapsed = self.since.elapsed().as_secs_f32();
        if self.on {
            let t = (elapsed / show).min(1.0);
            let eased = if spring { ease_out_back(t) } else { t };
            self.from + (1.0 - self.from) * eased
        } else {
            let t = (elapsed / hide).min(1.0);
            self.from * (1.0 - t)
        }
    }

    fn settled(&self, show: f32, hide: f32) -> bool {
        self.since.elapsed().as_secs_f32() >= if self.on { show } else { hide }
    }
}

#[derive(Default)]
pub(super) struct InlineVideos {
    #[cfg(target_os = "macos")]
    players: HashMap<Key, Slot>,
    #[cfg(not(target_os = "macos"))]
    players: HashMap<Key, ()>,
    render: u64,
    /// The history rendered (and swept the players) this frame.
    swept: bool,
}

#[cfg(target_os = "macos")]
struct Slot {
    video: super::native_video::NativeVideo,
    seen: u64,
    sound: bool,
    /// Paused by a click while playing with sound.
    paused: bool,
    /// Dragging along the seek ring: whether it was playing before.
    seeking: Option<bool>,
    seek_shown: Toggle,
    seek_grabbed: Toggle,
}

impl InlineVideos {
    /// Start a render pass; players whose rows didn't render in the last
    /// pass stop.
    pub(super) fn begin_render(&mut self) {
        self.swept = true;
        self.render += 1;
        #[cfg(target_os = "macos")]
        {
            let render = self.render;
            self.players.retain(|_, slot| slot.seen + 1 >= render);
        }
    }

    /// Called at the start of every app frame: when the last frame
    /// rendered no history (no chat open, Contacts, Calls…), nothing can
    /// show a clip, so every player stops. Without this, a clip that was
    /// playing when you left the chat kept decoding and kept the frame
    /// clock at 30 fps forever.
    pub(super) fn frame_start(&mut self) {
        if !self.swept {
            self.clear();
        }
        self.swept = false;
    }

    /// Whether any clip is playing: the frame clock keeps ticking, also
    /// for a player that hasn't produced its first frame yet.
    pub(super) fn active(&self) -> bool {
        !self.players.is_empty()
    }

    /// Stop everything (viewer opened, autoplay turned off).
    pub(super) fn clear(&mut self) {
        self.players.clear();
    }

    /// A click on a running clip (tdesktop's round video message): a
    /// muted loop plays once from the start with sound (muting any other),
    /// a sounding one pauses, a paused one resumes. False when it isn't
    /// running.
    #[cfg(target_os = "macos")]
    pub(super) fn toggle_sound(&mut self, chat_id: i64, message_id: i64) -> bool {
        let key = (chat_id, message_id);
        let Some(slot) = self.players.get_mut(&key) else {
            return false;
        };
        if slot.sound {
            slot.paused = !slot.paused;
            if slot.paused {
                slot.video.pause();
            } else {
                slot.video.play();
            }
            return true;
        }
        for (other, slot) in &mut self.players {
            if *other != key && slot.sound {
                slot.sound = false;
                slot.paused = false;
                slot.video.set_volume(0.0);
                slot.video.play();
            }
        }
        if let Some(slot) = self.players.get_mut(&key) {
            slot.sound = true;
            slot.paused = false;
            slot.video.set_volume(1.0);
            slot.video.seek(0.0);
            slot.video.play();
        }
        true
    }

    /// Drag along a sounding clip's seek ring to `fraction` of it: the
    /// clip holds still while dragged and resumes on release if it was
    /// playing.
    #[cfg(target_os = "macos")]
    pub(super) fn seek_to(&mut self, chat_id: i64, message_id: i64, fraction: f32) -> bool {
        let Some(slot) = self.players.get_mut(&(chat_id, message_id)) else {
            return false;
        };
        if !slot.sound {
            return false;
        }
        if slot.seeking.is_none() {
            slot.seeking = Some(!slot.paused);
            slot.video.pause();
        }
        if let Some(total) = slot.video.duration_secs() {
            slot.video.seek(f64::from(fraction.clamp(0.0, 1.0)) * total);
        }
        true
    }

    #[cfg(target_os = "macos")]
    pub(super) fn end_seek(&mut self, chat_id: i64, message_id: i64) {
        if let Some(slot) = self.players.get_mut(&(chat_id, message_id))
            && let Some(was_playing) = slot.seeking.take()
        {
            slot.paused = !was_playing;
            if was_playing {
                slot.video.play();
            }
        }
    }

    pub(super) fn is_seeking(&self, chat_id: i64, message_id: i64) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.players
                .get(&(chat_id, message_id))
                .is_some_and(|slot| slot.seeking.is_some())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (chat_id, message_id);
            false
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn seek_to(&mut self, _chat_id: i64, _message_id: i64, _fraction: f32) -> bool {
        false
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn end_seek(&mut self, _chat_id: i64, _message_id: i64) {}

    /// Whether a seek ring is still springing in or out.
    pub(super) fn seek_animating(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.players.values().any(|slot| {
                !slot.seek_shown.settled(SEEK_SHOW, SEEK_HIDE)
                    || !slot.seek_grabbed.settled(SEEK_GRAB, SEEK_GRAB)
            })
        }
        #[cfg(not(target_os = "macos"))]
        false
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
                    paused: false,
                    seeking: None,
                    seek_shown: Toggle::new(),
                    seek_grabbed: Toggle::new(),
                })
            }
        };
        slot.seen = render;
        if slot.video.error().is_some() {
            return None;
        }
        // Loop: `play` rewinds a clip that reached its end. A clip played
        // with sound goes back to its muted loop; a paused or dragged one
        // holds still.
        let held = slot.paused || slot.seeking.is_some();
        if !held && !slot.video.is_playing() {
            if slot.sound {
                slot.sound = false;
                slot.video.set_volume(0.0);
            }
            slot.video.play();
        }
        slot.seek_shown
            .set(slot.sound && held, SEEK_SHOW, SEEK_HIDE, true);
        slot.seek_grabbed
            .set(slot.seeking.is_some(), SEEK_GRAB, SEEK_GRAB, false);
        let buffer = slot.video.frame()?;
        let remaining_secs = slot
            .video
            .duration_secs()
            .map(|total| (total - slot.video.position_secs()).max(0.0));
        let progress = slot
            .video
            .duration_secs()
            .map(|total| (slot.video.position_secs() / total).clamp(0.0, 1.0) as f32);
        Some(InlineFrame {
            buffer,
            remaining_secs,
            sound: slot.sound,
            backdrop: gpui_kit::black(),
            progress,
            seek_shown: slot.seek_shown.value(SEEK_SHOW, SEEK_HIDE, true),
            seek_grabbed: slot.seek_grabbed.value(SEEK_GRAB, SEEK_GRAB, false),
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
        let file = session.files.get(&file_id.0)?;
        if file.usable_path().is_none() {
            // Like Telegram Desktop, fetch a clip that should autoplay
            // (within a size cap) in the background; it starts once local.
            let size = file.size.max(file.expected_size);
            if size > 0 && size <= AUTOPLAY_PREFETCH_MAX && session.should_download(file_id) {
                let app = cx.weak_entity();
                cx.defer(move |cx| {
                    let _ = app.update(cx, |this, _| {
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.download_file(file_id, 1);
                        }
                    });
                });
            }
            return None;
        }
        let (chat_id, message_id) = (message.chat_id, message.id);
        let frame = self
            .inline_videos
            .borrow_mut()
            .frame(chat_id.0, message_id.0, || {
                self.playable_clip_path(chat_id, message_id, file_id)
            });
        {
            let videos = self.inline_videos.borrow();
            if videos.active() {
                // Seek rings spring in and out more smoothly at 60.
                self.request_animation_tick(if videos.seek_animating() { 60 } else { 30 }, cx);
            }
        }
        // Masks over the video blend into the history behind it.
        let backdrop = self
            .appearance
            .wallpaper_rgb
            .map_or(cx.theme().background, |color| rgb(color).into());
        frame.map(|frame| InlineFrame { backdrop, ..frame })
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
