//! Inline autoplay of videos and GIFs in the history, as in Telegram
//! Desktop: each visible, downloaded clip plays muted and looped on the
//! shared player core (`video_player::VideoPlayer`, `Purpose::Inline`:
//! AVPlayer on macOS, the bundled FFmpeg on Linux and Windows). The
//! players live in `inline_players::InlineVideos`; this module decides
//! which clips autoplay, sizes them and hands their tiles a frame.
//! A muted loop is drawn by the conversation's animation layer
//! (`anim_layer::tile`), whose `video_frame` asks for its own frames on
//! the display's vsync, so a playing clip redraws only its tile.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::inline_players::{Demand, InlineVideos};
use super::play_intent::PlayTarget;
use super::video_player::VideoPlayer;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::state::HistoryMessage;
use quill::telegram::envelope::MessageContent;
use std::rc::Rc;

/// Largest clip fetched in the background so it can autoplay.
const AUTOPLAY_PREFETCH_MAX: i64 = 20 * 1024 * 1024;

/// A playing inline clip for its tile, and the clip's remaining time.
pub(super) struct InlineFrame {
    /// The clip's player; the tile draws `video_frame` from it.
    pub(super) player: VideoPlayer,
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
    videos: Rc<InlineVideos>,
    key: (i64, i64),
    backdrop: Hsla,
}

impl LiveSource {
    /// The clip's current frame; `None` once its player stopped.
    pub(super) fn frame(&self) -> Option<InlineFrame> {
        let frame = self.videos.current(self.key)?;
        Some(InlineFrame {
            backdrop: self.backdrop,
            ..frame
        })
    }
}

/// Pictures of an inline clip are decoded at most this many px on their
/// longer side: the tallest tile (`media_frame`'s 400 pt) at 2x. Below
/// that they are decoded at the tile's own size in device pixels
/// ([`InlineTile::decode_edge`]); the AVPlayer backend draws its own
/// buffers and ignores it.
pub(super) const INLINE_MAX_EDGE: u32 = 800;
const INLINE_MIN_EDGE: u32 = 16;

/// Telegram Desktop's `kMaxInlineArea` (`history_view_gif.cpp`): a clip
/// whose frames are larger than 1080p doesn't play inline (its still
/// shows; the viewer plays it). Applied where clips are decoded in
/// software (FFmpeg), as tdesktop does everywhere.
const MAX_INLINE_AREA: i64 = 1920 * 1080;

/// Where an inline clip is drawn: its media's size in pixels (Telegram's
/// metadata; 0 when unknown) and its tile in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct InlineTile {
    media: (i32, i32),
    tile: (f32, f32),
}

impl InlineTile {
    /// A video or GIF tile: the media fitted by `media_frame_for`, widened
    /// by a caption of `caption_width` as the bubble draws it, so the
    /// pictures are decoded for the frame they fill. Sized for a wide
    /// history: a narrower one draws the same pictures smaller.
    pub(super) fn media(
        kind: crate::ui::message_media::MediaFrameKind,
        width: i32,
        height: i32,
        caption_width: i32,
    ) -> Self {
        let (w, h) = super::message_media::media_frame_for(
            kind,
            width,
            height,
            caption_width,
            quill::bubble_layout::MAX_MEDIA_SIZE,
        );
        Self {
            media: (width, height),
            tile: (f32::from(w), f32::from(h)),
        }
    }

    /// A clip of `width` x `height` pixels in a panel's grid cell of
    /// `cell` points (the GIF panel, the inline results).
    pub(super) fn cell(width: i32, height: i32, cell: (f32, f32)) -> Self {
        Self {
            media: (width, height),
            tile: cell,
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
    pub(super) fn decode_edge(self, scale: f32) -> u32 {
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
    pub(super) fn decode_bytes(self, edge: u32, queued: usize) -> usize {
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

impl QuillApp {
    /// The caption's width for a video or GIF tile: it widens the frame
    /// (`media_frame_for`); its skip block is counted as if the caption
    /// ended the bubble, so the decoded pictures are never smaller than
    /// the frame they fill. 0 for other content.
    pub(super) fn inline_caption_width(
        &self,
        message: &HistoryMessage,
        cx: &mut Context<Self>,
    ) -> i32 {
        if !matches!(
            message.content,
            MessageContent::Video(_) | MessageContent::Animation(_)
        ) {
            return 0;
        }
        super::history::bubble_width::caption_width(
            cx,
            &message.content,
            self.bubble_look(cx).font,
            Some(
                super::message_text::FooterMeta::of(
                    message,
                    quill::state::OutboxReceipt::None,
                    message
                        .interaction_info
                        .as_ref()
                        .map(|info| info.view_count)
                        .filter(|&count| count > 0),
                    None,
                )
                .reserve(super::synthetic::footer_reserve(message.is_outgoing)),
            ),
        )
    }

    /// The inline frame for a history row's video or GIF, when it should
    /// play: the native player is available, no viewer covers the chat,
    /// it isn't secret or behind an unrevealed spoiler, the clip is downloaded, and
    /// either autoplay is on for its kind with data saver off, or a click
    /// asked for it ([`Demand`]: a round video, an MP4 GIF).
    pub(super) fn inline_frame(
        &self,
        message: &HistoryMessage,
        cx: &mut Context<Self>,
    ) -> Option<InlineFrame> {
        if !super::video_player::supported()
            || self.viewer.state.is_open()
            || self.stories.viewer.is_open()
        {
            return None;
        }
        let session = self.session()?;
        let prefs = &session.settings.media_prefs;
        let key = (message.chat_id.0, message.id.0);
        let demand = self.playback.inline_videos.demand(key);
        let autoplay_videos = prefs.autoplay_videos && !prefs.data_saver;
        let autoplay_gifs = prefs.autoplay_gifs && !prefs.data_saver;
        let caption_width = self.inline_caption_width(message, cx);
        let spoiler_covered = self.media_spoiler_covered(message);
        // `viewed`: a round video, whether it was watched.
        let (file_id, tile, duration, autoplay, viewed) = match &message.content {
            MessageContent::Video(video)
                if autoplay_videos && !video.is_secret && !spoiler_covered =>
            {
                (
                    video.play_file_id()?,
                    InlineTile::media(
                        crate::ui::message_media::MediaFrameKind::Video,
                        video.width,
                        video.height,
                        caption_width,
                    ),
                    video.duration,
                    true,
                    None,
                )
            }
            MessageContent::VideoNote(note)
                if !note.is_secret && (autoplay_videos || demand.is_some()) =>
            {
                (
                    note.play_file_id()?,
                    InlineTile::round(note.length),
                    note.duration,
                    autoplay_videos,
                    Some(note.is_viewed),
                )
            }
            // The player takes Telegram's MP4 GIFs (true `image/gif` files
            // loop in `inline_playback`, unless their bytes turned out to
            // be a video, which asks for them here on demand).
            MessageContent::Animation(animation)
                if !animation.is_secret
                    && !spoiler_covered
                    && (demand.is_some()
                        || (autoplay_gifs && animation.mime_type != "image/gif")) =>
            {
                (
                    animation.play_file_id()?,
                    InlineTile::media(
                        crate::ui::message_media::MediaFrameKind::Gif,
                        animation.width,
                        animation.height,
                        caption_width,
                    ),
                    animation.duration,
                    autoplay_gifs && animation.mime_type != "image/gif",
                    None,
                )
            }
            _ => return None,
        };
        // Software decoding (FFmpeg) leaves clips above 1080p to the viewer.
        if super::video_player::decodes_in_process() && !tile.within_inline_area() {
            return None;
        }
        let file = session.media.files.get(&file_id.0)?;
        if file.usable_path().is_none() {
            // Like Telegram Desktop, fetch a clip that should autoplay
            // (within a size cap) in the background; it starts once local.
            // A clip asked for by a click was asked to download then.
            let size = file.size.max(file.expected_size);
            if autoplay
                && size > 0
                && size <= AUTOPLAY_PREFETCH_MAX
                && session.should_download(file_id)
            {
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
        let duration = (duration > 0).then_some(f64::from(duration));
        let videos = &self.playback.inline_videos;
        let mut frame = videos.frame(
            key,
            tile,
            duration,
            || self.playable_clip_path(chat_id, message_id, file_id),
            cx,
        );
        if demand == Some(Demand::Sound) && frame.is_some() {
            if videos.start_demand(key) {
                // Clicked: once from the start with sound, as a click on a
                // muted loop does.
                if videos.toggle_sound(key.0, key.1) {
                    let app = cx.weak_entity();
                    let viewed = viewed.unwrap_or(true);
                    // The history drawing it is the window's.
                    let scope = self.windows.scope;
                    cx.defer(move |cx| {
                        let _ = app.update(cx, |this, cx| {
                            this.in_window_scope(scope, cx, |this, cx| {
                                this.round_sound_started(key, cx);
                            });
                            if !viewed {
                                this.mark_voice_opened(chat_id, message_id);
                            }
                            cx.notify();
                        });
                    });
                }
                frame = videos.current(key);
            } else if frame.as_ref().is_some_and(|frame| !frame.sound) {
                // Played to its end: done; with autoplay off it rests on
                // its still again.
                videos.end_demand(key);
                if !autoplay {
                    return None;
                }
            }
        }
        // A muted loop in the conversation is drawn by its animation layer
        // while the history replays: its `video_frame` asks the layer for
        // frames. A clip with sound or a seek ring stays with the history,
        // whose `video_frame` redraws it.
        let layered = frame
            .as_ref()
            .is_some_and(|frame| !frame.sound && frame.seek_shown <= 0.)
            && self.slices.in_conversation()
            && super::anim_layer::current().is_some();
        if let Some(frame) = frame.as_ref().filter(|frame| frame.sound) {
            // Its end (back to the muted loop) redraws the history.
            frame
                .player
                .watch(self.slices.current().unwrap_or_else(|| cx.entity_id()));
        }
        // Seek rings spring in and out while the clip holds still.
        if self.playback.inline_videos.seek_animating() {
            self.request_animation_tick(60, cx);
        }
        // Masks over the video blend into the history behind it.
        let backdrop = self.wallpaper_backdrop(cx);
        let live = layered.then(|| LiveSource {
            videos: self.playback.inline_videos.clone(),
            key,
            backdrop,
        });
        frame.map(|frame| InlineFrame {
            backdrop,
            live,
            ..frame
        })
    }

    /// Play `message_id`'s clip on the inline player on demand (a click
    /// with autoplay off or data saver on, or on a clip not downloaded
    /// yet): downloaded first if need be, it starts once its row renders
    /// it local. Without an in-process player the viewer opens instead,
    /// and says why it can't play.
    pub(super) fn play_inline_on_demand(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        file_id: FileId,
        demand: Demand,
        cx: &mut Context<Self>,
    ) {
        if !super::video_player::supported() {
            self.open_media_viewer(chat_id, message_id, cx);
            return;
        }
        if !self.file_is_local(file_id) {
            let target = PlayTarget::Inline {
                chat: chat_id,
                message: message_id,
                demand,
            };
            self.play_when_ready(file_id, target, cx);
            self.notify_conversation(cx);
            return;
        }
        self.playback.intent = None;
        self.playback
            .inline_videos
            .request((chat_id.0, message_id.0), demand);
        self.notify_conversation(cx);
        cx.notify();
    }

    /// A click (or Space) on a round video message: a playing one plays
    /// once with sound, pauses or resumes ([`InlineVideos::toggle_sound`]);
    /// one not playing inline yet plays with sound on demand.
    pub(super) fn play_video_note(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        note: &quill::telegram::envelope::VideoNoteContent,
        cx: &mut Context<Self>,
    ) {
        if note.is_secret {
            return;
        }
        let key = (chat_id.0, message_id.0);
        if self.playback.inline_videos.toggle_sound(key.0, key.1) {
            self.round_sound_started(key, cx);
            if !note.is_viewed {
                self.mark_voice_opened(chat_id, message_id);
            }
            cx.notify();
            return;
        }
        let Some(play_id) = note.play_file_id() else {
            return;
        };
        self.press_media(play_id, cx, |this, cx| {
            this.play_inline_on_demand(chat_id, message_id, play_id, Demand::Sound, cx);
        });
    }
}

/// An image of `color` the size of a `width` x `height` tile with each
/// corner's outside cut away in an anti-aliased quarter circle of its radius
/// (`[top-left, top-right, bottom-right, bottom-left]`, in points): laid
/// over a native video surface, which GPUI can't clip, it rounds the video
/// to match the bubble. Rendered at twice the size for Retina edges.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
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
            let (Some(&corner_radius), Some(&color)) = (radii.get(corner), bgra.get(corner)) else {
                continue;
            };
            let radius = corner_radius as f32 * 2.0;
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
    use super::{INLINE_MAX_EDGE, InlineTile, corner_mask};

    #[test]
    fn clips_decode_at_their_tile_size_in_device_pixels() {
        // A 720p GIF fills a 320 x 180 pt tile (`maxGifSize`): 320 px wide
        // at 1x, 640 at 2x.
        let gif = InlineTile::media(crate::ui::message_media::MediaFrameKind::Gif, 1280, 720, 0);
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
            InlineTile::media(
                crate::ui::message_media::MediaFrameKind::Video,
                1080,
                1920,
                0
            )
            .decode_edge(2.),
            INLINE_MAX_EDGE
        );
        // A very wide clip cropped into the minimum height, or a clip of
        // unknown proportions, decodes at the cap.
        assert_eq!(
            InlineTile::media(
                crate::ui::message_media::MediaFrameKind::Video,
                1000,
                100,
                0
            )
            .decode_edge(1.),
            INLINE_MAX_EDGE
        );
        assert_eq!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 0, 0, 0)
                .decode_edge(1.),
            INLINE_MAX_EDGE
        );
        // Before the first frame reports a scale, assume 2x.
        assert_eq!(gif.decode_edge(0.), 640);
        assert_eq!(gif.decode_edge(f32::NAN), 640);
    }

    #[test]
    fn a_caption_widened_clip_decodes_for_its_wider_frame() {
        use crate::ui::message_media::MediaFrameKind::Video;
        // 320 x 180 fills its own frame; a caption 380 px wide widens the
        // frame to 402 (`media_frame_for`), and the pictures follow it.
        assert_eq!(InlineTile::media(Video, 320, 180, 0).decode_edge(1.), 320);
        assert_eq!(InlineTile::media(Video, 320, 180, 380).decode_edge(1.), 402);
        assert_eq!(
            InlineTile::media(Video, 320, 180, 380).decode_edge(2.),
            INLINE_MAX_EDGE
        );
    }

    #[test]
    fn small_clips_are_never_upscaled() {
        let tiny = InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 100, 80, 0);
        assert_eq!(tiny.decoded_size(tiny.decode_edge(2.)), (100, 80));
        assert_eq!(tiny.decode_bytes(720, 3), 100 * 80 * 4 * 4);
        assert_eq!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 0, 0, 0)
                .decoded_size(300),
            (300, 300)
        );
    }

    #[test]
    fn clips_above_1080p_stay_still_in_software() {
        assert!(
            InlineTile::media(
                crate::ui::message_media::MediaFrameKind::Video,
                1920,
                1080,
                0
            )
            .within_inline_area()
        );
        assert!(
            InlineTile::media(
                crate::ui::message_media::MediaFrameKind::Video,
                1080,
                1920,
                0
            )
            .within_inline_area()
        );
        assert!(
            !InlineTile::media(
                crate::ui::message_media::MediaFrameKind::Video,
                2560,
                1440,
                0
            )
            .within_inline_area()
        );
        assert!(
            !InlineTile::media(
                crate::ui::message_media::MediaFrameKind::Video,
                3840,
                2160,
                0
            )
            .within_inline_area()
        );
        // Unknown sizes may try.
        assert!(
            InlineTile::media(crate::ui::message_media::MediaFrameKind::Video, 0, 0, 0)
                .within_inline_area()
        );
        assert!(InlineTile::round(640).within_inline_area());
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
}
