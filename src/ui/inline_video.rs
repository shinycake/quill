//! Inline autoplay of videos and GIFs in the history, as in Telegram
//! Desktop: each visible, downloaded clip plays muted and looped on the
//! native player (`NativeVideo`: AVPlayer on macOS, the bundled FFmpeg on
//! Linux and Windows).
//! Players live while their row renders and are dropped a couple of
//! renders after it scrolls away (or the chat changes). A round video
//! message plays once with sound when clicked, then loops muted again.

use super::app::QuillApp;
use super::native_video::{NativeVideo, Purpose, VideoPicture};
use gpui_kit::*;
use quill::state::HistoryMessage;
use quill::telegram::envelope::MessageContent;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// Largest clip fetched in the background so it can autoplay.
const AUTOPLAY_PREFETCH_MAX: i64 = 20 * 1024 * 1024;

/// `(chat id, message id)`.
type Key = (i64, i64);

/// A frame for an inline tile, and the clip's remaining time.
pub(super) struct InlineFrame {
    pub(super) picture: VideoPicture,
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
    /// Set when the conversation's animation layer draws this clip (a
    /// muted loop in the history): where it gets the next frames.
    pub(super) live: Option<LiveSource>,
}

/// A playing clip the history's animation layer redraws every frame
/// (`anim_layer::tile`), while the conversation replays its cached frame.
#[derive(Clone)]
pub(super) struct LiveSource {
    videos: Rc<RefCell<InlineVideos>>,
    key: Key,
    backdrop: Hsla,
}

impl LiveSource {
    /// The clip's current frame; `None` once its player stopped.
    pub(super) fn frame(&self) -> Option<InlineFrame> {
        let frame = self.videos.borrow_mut().current(self.key.0, self.key.1)?;
        Some(InlineFrame {
            backdrop: self.backdrop,
            ..frame
        })
    }
}

/// tdesktop `VideoMessageSeek`: the ring shows in 220 ms (ease-out-back),
/// hides in 150 ms, and the dot grows or shrinks in 150 ms.
const SEEK_SHOW: f32 = 0.22;
const SEEK_HIDE: f32 = 0.15;
const SEEK_GRAB: f32 = 0.15;

/// Pictures of an inline clip are decoded at most this many px on their
/// longer side: the tallest tile (`media_frame`'s 400 pt) at 2x. Below
/// that they are decoded at the tile's own size in device pixels
/// ([`InlineTile::decode_edge`]); the AVPlayer backend draws its own
/// buffers and ignores it.
const INLINE_MAX_EDGE: u32 = 800;
const INLINE_MIN_EDGE: u32 = 16;

/// Telegram Desktop's `kMaxInlineArea` (`history_view_gif.cpp`): a clip
/// whose frames are larger than 1080p doesn't play inline (its still
/// shows; the viewer plays it). Applied where clips are decoded in
/// software (FFmpeg), as tdesktop does everywhere.
const MAX_INLINE_AREA: i64 = 1920 * 1080;

/// Decoded picture memory (queued pictures plus the one shown) all inline
/// players together may hold. Past it a further tile keeps its still until
/// a running clip stops; with tiles sized to the screen it takes a dozen
/// or more large clips on screen at once to get there.
const INLINE_DECODE_BUDGET: usize = 64 * 1024 * 1024;

/// Where an inline clip is drawn: its media's size in pixels (Telegram's
/// metadata; 0 when unknown) and its tile in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct InlineTile {
    media: (i32, i32),
    tile: (f32, f32),
}

impl InlineTile {
    /// A video or GIF tile: the media fitted by `media_frame`.
    pub(super) fn media(
        kind: crate::ui::message_media::MediaFrameKind,
        width: i32,
        height: i32,
    ) -> Self {
        let (w, h) = super::message_media::media_frame(kind, width, height);
        Self {
            media: (width, height),
            tile: (f32::from(w), f32::from(h)),
        }
    }

    /// A round video message: a square clip in a fixed circle.
    pub(super) fn round(length: i32) -> Self {
        let diameter = super::message_media::VIDEO_NOTE_DIAMETER;
        Self {
            media: (length, length),
            tile: (diameter, diameter),
        }
    }

    /// The longer side, in device pixels, to decode pictures at so that,
    /// drawn with `ObjectFit::Cover`, they fill the tile without upscaling
    /// on a `scale` display; bounded by [`INLINE_MAX_EDGE`].
    fn decode_edge(self, scale: f32) -> u32 {
        let scale = if scale.is_finite() && scale > 0. {
            scale
        } else {
            2.
        };
        let (tw, th) = (self.tile.0 * scale, self.tile.1 * scale);
        let longer = match self.media {
            (w, h) if w > 0 && h > 0 => {
                let (w, h) = (w as f32, h as f32);
                w.max(h) * (tw / w).max(th / h)
            }
            // Unknown proportions: the cap keeps any shape sharp.
            _ => INLINE_MAX_EDGE as f32,
        };
        longer
            .ceil()
            .clamp(INLINE_MIN_EDGE as f32, INLINE_MAX_EDGE as f32) as u32
    }

    /// The decoded picture size: the media scaled down (never up) to fit
    /// an `edge` box, as the decoder does.
    fn decoded_size(self, edge: u32) -> (u32, u32) {
        match self.media {
            (w, h) if w > 0 && h > 0 => {
                let (w, h) = (w as u32, h as u32);
                let longer = w.max(h);
                if longer <= edge {
                    (w, h)
                } else {
                    let fit = |side: u32| {
                        ((u64::from(side) * u64::from(edge)).div_ceil(u64::from(longer)) as u32)
                            .max(1)
                    };
                    (fit(w), fit(h))
                }
            }
            _ => (edge, edge),
        }
    }

    /// Bytes of BGRA a player decoding at `edge` holds at most: `queued`
    /// pictures ahead plus the one shown.
    fn decode_bytes(self, edge: u32, queued: usize) -> usize {
        let (w, h) = self.decoded_size(edge);
        w as usize * h as usize * 4 * (queued + 1)
    }

    /// Whether the clip is small enough to play inline in software
    /// ([`MAX_INLINE_AREA`]); unknown sizes may try.
    pub(super) fn within_inline_area(self) -> bool {
        let (w, h) = self.media;
        w <= 0 || h <= 0 || i64::from(w) * i64::from(h) <= MAX_INLINE_AREA
    }
}

/// Whether a new player holding `more` bytes fits next to players holding
/// `held` ([`INLINE_DECODE_BUDGET`]). The first one always does.
fn admits(held: usize, more: usize) -> bool {
    held == 0 || held.saturating_add(more) <= INLINE_DECODE_BUDGET
}

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
    players: HashMap<Key, Slot>,
    render: u64,
    /// The history rendered (and swept the players) this frame.
    swept: bool,
    /// The window is in the background: muted loops hold still.
    inactive: bool,
    /// The window's display scale (device pixels per point); 0 until the
    /// first frame reports it.
    scale: f32,
}

struct Slot {
    video: NativeVideo,
    /// Decoded picture memory it may hold (0 for AVPlayer, whose buffers
    /// aren't ours).
    bytes: usize,
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
        let render = self.render;
        self.players.retain(|_, slot| slot.seen + 1 >= render);
    }

    /// Called at the start of every app frame: when the last frame
    /// rendered no history (no chat open, Contacts, Calls…), nothing can
    /// show a clip, so every player stops. Without this, a clip that was
    /// playing when you left the chat kept decoding and kept the frame
    /// clock at 30 fps forever.
    /// `scale` is the window's display scale, for the size new players
    /// decode at.
    pub(super) fn frame_start(&mut self, history_drawn: bool, scale: f32) {
        if history_drawn && !self.swept {
            self.clear();
        }
        // The display or interface scale changed: running players decode
        // at the old size, so restart them at the new one.
        if self.scale > 0. && (self.scale - scale).abs() > f32::EPSILON {
            self.clear();
        }
        self.swept = false;
        self.scale = scale;
    }

    /// Decoded picture memory the running players may hold.
    fn held_bytes(&self) -> usize {
        self.players.values().map(|slot| slot.bytes).sum()
    }

    /// Whether any clip is playing: the frame clock keeps ticking, also
    /// for a player that hasn't produced its first frame yet.
    pub(super) fn active(&self) -> bool {
        !self.players.is_empty()
    }

    /// The window became active or inactive: muted loops pause behind
    /// another app (tdesktop pauses GIFs and round loops there) and resume
    /// on return; a clip playing with sound keeps playing.
    pub(super) fn set_window_active(&mut self, active: bool) {
        self.inactive = !active;
        for slot in self.players.values_mut() {
            if slot.sound || slot.paused {
                continue;
            }
            if active {
                slot.video.play();
            } else {
                slot.video.pause();
            }
        }
    }

    /// Whether a clip plays with sound (it keeps drawing in the background).
    pub(super) fn sounding(&self) -> bool {
        self.players.values().any(|slot| slot.sound && !slot.paused)
    }

    /// Stop everything (viewer opened, autoplay turned off).
    pub(super) fn clear(&mut self) {
        self.players.clear();
    }

    /// A click on a running clip (tdesktop's round video message): a
    /// muted loop plays once from the start with sound (muting any other),
    /// a sounding one pauses, a paused one resumes. False when it isn't
    /// running.
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
        self.players
            .get(&(chat_id, message_id))
            .is_some_and(|slot| slot.seeking.is_some())
    }

    /// Whether a seek ring is still springing in or out.
    pub(super) fn seek_animating(&self) -> bool {
        self.players.values().any(|slot| {
            !slot.seek_shown.settled(SEEK_SHOW, SEEK_HIDE)
                || !slot.seek_grabbed.settled(SEEK_GRAB, SEEK_GRAB)
        })
    }

    /// The current frame of this message's clip, starting its muted,
    /// looping player on first use (`path` is only resolved then), sized
    /// for `tile`. A clip that would take the decoded memory past
    /// [`INLINE_DECODE_BUDGET`] doesn't start (its still shows).
    pub(super) fn frame(
        &mut self,
        chat_id: i64,
        message_id: i64,
        tile: InlineTile,
        path: impl FnOnce() -> Option<PathBuf>,
    ) -> Option<InlineFrame> {
        let render = self.render;
        let inactive = self.inactive;
        let held = self.held_bytes();
        let scale = self.scale;
        let slot = match self.players.entry((chat_id, message_id)) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let max_edge = tile.decode_edge(scale);
                let bytes = if super::native_video::decodes_in_process() {
                    let queued = quill::video_decode::OpenOptions::inline(max_edge).video_frames;
                    tile.decode_bytes(max_edge, queued)
                } else {
                    0
                };
                if !admits(held, bytes) {
                    return None;
                }
                let purpose = Purpose::Inline { max_edge };
                let mut video = NativeVideo::open(&path()?, purpose).ok()?;
                video.set_volume(0.0);
                // Behind another app a new muted loop waits for activation.
                if !inactive {
                    video.play();
                }
                entry.insert(Slot {
                    video,
                    bytes,
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
        Self::slot_frame(slot, inactive)
    }

    /// The current frame of a running clip, for the history's animation
    /// layer: [`Self::frame`] without starting a player or marking its row
    /// rendered.
    pub(super) fn current(&mut self, chat_id: i64, message_id: i64) -> Option<InlineFrame> {
        let inactive = self.inactive;
        let slot = self.players.get_mut(&(chat_id, message_id))?;
        Self::slot_frame(slot, inactive)
    }

    fn slot_frame(slot: &mut Slot, inactive: bool) -> Option<InlineFrame> {
        if slot.video.error().is_some() {
            return None;
        }
        // Loop: `play` rewinds a clip that reached its end. A clip played
        // with sound goes back to its muted loop; a paused or dragged one
        // holds still.
        // Behind another app muted loops hold still (`set_window_active`);
        // a render there must not restart them.
        let held = slot.paused || slot.seeking.is_some() || (inactive && !slot.sound);
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
        let picture = slot.video.frame()?;
        let remaining_secs = slot
            .video
            .duration_secs()
            .map(|total| (total - slot.video.position_secs()).max(0.0));
        let progress = slot
            .video
            .duration_secs()
            .map(|total| (slot.video.position_secs() / total).clamp(0.0, 1.0) as f32);
        Some(InlineFrame {
            picture,
            remaining_secs,
            sound: slot.sound,
            backdrop: gpui_kit::black(),
            progress,
            seek_shown: slot.seek_shown.value(SEEK_SHOW, SEEK_HIDE, true),
            seek_grabbed: slot.seek_grabbed.value(SEEK_GRAB, SEEK_GRAB, false),
            live: None,
        })
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
        if !super::native_video::supported()
            || self.viewer.state.is_open()
            || self.stories.viewer.is_open()
        {
            return None;
        }
        let session = self.session()?;
        let prefs = &session.media_prefs;
        if prefs.data_saver {
            return None;
        }
        let (file_id, tile) = match &message.content {
            MessageContent::Video(video)
                if prefs.autoplay_videos && !video.is_secret && !video.has_spoiler =>
            {
                (
                    video.play_file_id()?,
                    InlineTile::media(
                        crate::ui::message_media::MediaFrameKind::Video,
                        video.width,
                        video.height,
                    ),
                )
            }
            // The player takes Telegram's MP4 GIFs; true `image/gif`
            // files keep the frame-extraction path.
            MessageContent::VideoNote(note) if prefs.autoplay_videos && !note.is_secret => {
                (note.play_file_id()?, InlineTile::round(note.length))
            }
            MessageContent::Animation(animation)
                if prefs.autoplay_gifs
                    && !animation.is_secret
                    && !animation.has_spoiler
                    && animation.mime_type != "image/gif" =>
            {
                (
                    animation.play_file_id()?,
                    InlineTile::media(
                        crate::ui::message_media::MediaFrameKind::Gif,
                        animation.width,
                        animation.height,
                    ),
                )
            }
            _ => return None,
        };
        // Software decoding (FFmpeg) leaves clips above 1080p to the viewer.
        if super::native_video::decodes_in_process() && !tile.within_inline_area() {
            return None;
        }
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
        let frame =
            self.playback
                .inline_videos
                .borrow_mut()
                .frame(chat_id.0, message_id.0, tile, || {
                    self.playable_clip_path(chat_id, message_id, file_id)
                });
        // A muted loop in the conversation is drawn by its animation layer
        // (which keeps it playing) while the history replays; a clip with
        // sound or a seek ring stays with the history, ticking it.
        let layered = frame
            .as_ref()
            .is_some_and(|frame| !frame.sound && frame.seek_shown <= 0.)
            && self.slices.in_conversation()
            && super::anim_layer::current().is_some();
        if !layered {
            let videos = self.playback.inline_videos.borrow();
            if videos.active() {
                // Seek rings spring in and out more smoothly at 60.
                let fps = if videos.seek_animating() { 60 } else { 30 };
                if videos.sounding() {
                    self.request_media_tick(fps, cx);
                } else {
                    self.request_animation_tick(fps, cx);
                }
            }
        }
        // Masks over the video blend into the history behind it.
        let backdrop = self.wallpaper_backdrop(cx);
        let live = layered.then(|| LiveSource {
            videos: self.playback.inline_videos.clone(),
            key: (chat_id.0, message_id.0),
            backdrop,
        });
        frame.map(|frame| InlineFrame {
            backdrop,
            live,
            ..frame
        })
    }
}

/// An image of `color` with a transparent, anti-aliased circle cut out of
/// it: laid over a square video it rounds the video, which GPUI's surface
/// can't clip itself. Cached per size and color.
pub(super) fn circle_mask(edge: u32, color: Hsla) -> std::sync::Arc<RenderImage> {
    use super::lru::Lru;
    use std::cell::RefCell;
    use std::sync::Arc;
    thread_local! {
        static MASKS: RefCell<Lru<(u32, [u8; 4]), Arc<RenderImage>>> = RefCell::new(Lru::new(32));
    }
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let bgra = [
        channel(rgba.b),
        channel(rgba.g),
        channel(rgba.r),
        channel(rgba.a),
    ];
    let key = (edge, bgra);
    if let Some(hit) = MASKS.with(|masks| masks.borrow_mut().get(&key)) {
        return hit;
    }
    let build = || {
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
    };
    let image = build();
    MASKS.with(|masks| {
        if let Some(old) = masks.borrow_mut().insert(key, image.clone()) {
            super::image_budget::retire_all([old]);
        }
    });
    image
}

/// An image of `color` the size of a `width` x `height` tile with each
/// corner's outside cut away in an anti-aliased quarter circle of its radius
/// (`[top-left, top-right, bottom-right, bottom-left]`, in points): laid
/// over a native video surface, which GPUI can't clip, it rounds the video
/// to match the bubble. Rendered at twice the size for Retina edges.
#[cfg(any(target_os = "macos", test))]
pub(super) fn corner_mask(
    width: u32,
    height: u32,
    radii: [u32; 4],
    colors: [Hsla; 4],
) -> std::sync::Arc<RenderImage> {
    use super::lru::Lru;
    use std::cell::RefCell;
    use std::sync::Arc;
    type Key = (u32, u32, [u32; 4], [[u8; 4]; 4]);
    thread_local! {
        static MASKS: RefCell<Lru<Key, Arc<RenderImage>>> = RefCell::new(Lru::new(32));
    }
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let bgra = colors.map(|color| {
        let rgba = Rgba::from(color);
        [
            channel(rgba.b),
            channel(rgba.g),
            channel(rgba.r),
            channel(rgba.a),
        ]
    });
    let key = (width, height, radii, bgra);
    if let Some(hit) = MASKS.with(|masks| masks.borrow_mut().get(&key)) {
        return hit;
    }
    let build = || {
        let (w, h) = (width * 2, height * 2);
        let mut image = image::RgbaImage::new(w.max(1), h.max(1));
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let left = px < w as f32 / 2.0;
            let top = py < h as f32 / 2.0;
            let corner = match (top, left) {
                (true, true) => 0,
                (true, false) => 1,
                (false, false) => 2,
                (false, true) => 3,
            };
            let radius = radii[corner] as f32 * 2.0;
            let color = bgra[corner];
            // Distance from this corner's edges.
            let dx = if left { px } else { w as f32 - px };
            let dy = if top { py } else { h as f32 - py };
            let cover = if dx >= radius || dy >= radius {
                0.0
            } else {
                let ox = radius - dx;
                let oy = radius - dy;
                ((ox * ox + oy * oy).sqrt() - radius + 0.5).clamp(0.0, 1.0)
            };
            let alpha = (f32::from(color[3]) * cover).round() as u8;
            *pixel = image::Rgba([color[0], color[1], color[2], alpha]);
        }
        Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
            image::Frame::new(image),
        ])))
    };
    let image = build();
    MASKS.with(|masks| {
        if let Some(old) = masks.borrow_mut().insert(key, image.clone()) {
            super::image_budget::retire_all([old]);
        }
    });
    image
}

#[cfg(test)]
mod tests {
    use super::{
        INLINE_DECODE_BUDGET, INLINE_MAX_EDGE, InlineTile, admits, circle_mask, corner_mask,
    };

    #[test]
    fn clips_decode_at_their_tile_size_in_device_pixels() {
        // A 720p GIF fills a 320 x 180 pt tile (`maxGifSize`): 320 px wide
        // at 1x, 640 at 2x.
        let gif = InlineTile::media(crate::ui::message_media::MediaFrameKind::Gif, 1280, 720);
        assert_eq!(gif.decode_edge(1.), 320);
        assert_eq!(gif.decode_edge(2.), 640);
        assert_eq!(gif.decoded_size(320), (320, 180));
        // A round video message: the 240 pt circle (`maxVideoMessageSize`).
        let round = InlineTile::round(640);
        assert_eq!(round.decode_edge(1.), 240);
        assert_eq!(round.decode_edge(1.5), 360);
        assert_eq!(round.decode_edge(2.), 480);
        // A tall portrait clip covers its 400 pt height at 2x, at the cap.
        assert_eq!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 1080, 1920)
                .decode_edge(2.),
            INLINE_MAX_EDGE
        );
        // A very wide clip cropped into the minimum height, or a clip of
        // unknown proportions, decodes at the cap.
        assert_eq!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 1000, 100)
                .decode_edge(1.),
            INLINE_MAX_EDGE
        );
        assert_eq!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 0, 0)
                .decode_edge(1.),
            INLINE_MAX_EDGE
        );
        // Before the first frame reports a scale, assume 2x.
        assert_eq!(gif.decode_edge(0.), 640);
        assert_eq!(gif.decode_edge(f32::NAN), 640);
    }

    #[test]
    fn small_clips_are_never_upscaled() {
        let tiny = InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 100, 80);
        assert_eq!(tiny.decoded_size(tiny.decode_edge(2.)), (100, 80));
        assert_eq!(tiny.decode_bytes(720, 3), 100 * 80 * 4 * 4);
        assert_eq!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 0, 0)
                .decoded_size(300),
            (300, 300)
        );
    }

    #[test]
    fn clips_above_1080p_stay_still_in_software() {
        assert!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 1920, 1080)
                .within_inline_area()
        );
        assert!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 1080, 1920)
                .within_inline_area()
        );
        assert!(
            !InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 2560, 1440)
                .within_inline_area()
        );
        assert!(
            !InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 3840, 2160)
                .within_inline_area()
        );
        // Unknown sizes may try.
        assert!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 0, 0)
                .within_inline_area()
        );
        assert!(InlineTile::round(640).within_inline_area());
    }

    #[test]
    fn players_share_a_decoded_memory_budget() {
        // The first clip always plays, however large.
        assert!(admits(0, INLINE_DECODE_BUDGET * 2));
        // Players at 1x 720p-GIF size: dozens fit.
        let each = InlineTile::media(crate::ui::message_media::MediaFrameKind::Gif, 1280, 720)
            .decode_bytes(360, 3);
        let fit = (1..).take_while(|n| admits(each * n, each)).count();
        assert!(fit >= 40, "{fit}");
        // At 2x they cost four times as much, still a dozen.
        let each = InlineTile::media(crate::ui::message_media::MediaFrameKind::Gif, 1280, 720)
            .decode_bytes(720, 3);
        let fit = (1..).take_while(|n| admits(each * n, each)).count();
        assert!((12..40).contains(&fit), "{fit}");
        // AVPlayer players hold nothing of ours.
        assert!(admits(INLINE_DECODE_BUDGET, 0));
        assert!(!admits(INLINE_DECODE_BUDGET, 1));
    }

    #[test]
    fn corner_mask_cuts_only_the_corner_outsides() {
        let colors = [
            gpui_kit::white(),
            gpui_kit::white(),
            gpui_kit::black(),
            gpui_kit::white(),
        ];
        let mask = corner_mask(40, 30, [10, 0, 10, 0], colors);
        let bytes = mask.as_bytes(0).expect("one frame");
        let alpha = |x: usize, y: usize| bytes[(y * 80 + x) * 4 + 3];
        // Rounded corners keep the backdrop at the extreme pixel; the square
        // ones and the middle show the video.
        assert_eq!(alpha(0, 0), 255);
        assert_eq!(alpha(79, 59), 255);
        assert_eq!(alpha(79, 0), 0);
        assert_eq!(alpha(0, 59), 0);
        assert_eq!(alpha(40, 30), 0);
        // Straight BGRA: each corner keeps its own color.
        assert_eq!(&bytes[0..3], &[255, 255, 255]);
        let br = (59 * 80 + 79) * 4;
        assert_eq!(&bytes[br..br + 3], &[0, 0, 0]);
    }

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
