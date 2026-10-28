//! Shared bounded animated sticker images for history and the picker.
#![deny(clippy::indexing_slicing)]
use super::anim_layer::LayeredClip;
use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::FileId;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::power_saving;
use quill::sticker_playback::Tint;
use quill::telegram::envelope::{DicePhase, StickerFormat};
use smallvec::SmallVec;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub(super) struct StickerClip {
    /// One single-frame image per animation frame; the frame clock picks
    /// the current one (`frame_clock`), instead of GPUI animating a
    /// multi-frame image at the display's refresh rate.
    frames: Arc<[Arc<RenderImage>]>,
    /// Decoded size of all frames (the cache's byte budget).
    bytes: usize,
    fps: f64,
    started: Instant,
    duration: Duration,
    /// When the clip was last shown; an animation layer drawing it keeps
    /// this fresh while its row replays (`anim_layer`).
    used: Rc<Cell<Instant>>,
}
pub(super) struct StickerJob {
    cancel: Arc<AtomicBool>,
    child: Arc<Mutex<Option<std::process::Child>>>,
}
/// Which playback cache. Stickers decode at 224 px, the box Telegram
/// Desktop draws them in (`maxStickerSize`; at most 16 clips). Custom emoji are small and many — every visible one animates,
/// as in Telegram Desktop — so 56 px, at most 36 frames, 160 clips and
/// 32 MB. Dice draw at 112 px, so 224 px covers 2x screens, in a cache of
/// their own: a slot machine plays five clips at once, which the sticker
/// budget can't hold next to other stickers. All play at most 30 fps
/// (`MAX_PLAYBACK_FPS`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PlaybackSize {
    Sticker,
    Emoji,
    Dice,
}

impl PlaybackSize {
    fn edge(self) -> u32 {
        match self {
            Self::Sticker => 224,
            Self::Emoji => 56,
            Self::Dice => 224,
        }
    }

    fn capacity(self) -> usize {
        match self {
            Self::Sticker => 16,
            Self::Emoji => 160,
            Self::Dice => 16,
        }
    }

    /// Decoded frames kept, in bytes.
    pub(super) fn budget_bytes(self) -> usize {
        match self {
            Self::Sticker => 48 * 1024 * 1024,
            Self::Emoji => 32 * 1024 * 1024,
            // A slot machine (about 60 MB) and two other dice.
            Self::Dice => 96 * 1024 * 1024,
        }
    }

    /// The power-saving switch for this kind of clip shown in a chat.
    fn chat_flag(self) -> power_saving::Flag {
        match self {
            Self::Sticker | Self::Dice => power_saving::Flag::StickersChat,
            Self::Emoji => power_saving::Flag::EmojiChat,
        }
    }

    fn max_frames(self) -> usize {
        match self {
            Self::Sticker | Self::Dice => quill::sticker_playback::MAX_STICKER_FRAMES,
            Self::Emoji => 36,
        }
    }

    /// Concurrent decodes.
    fn decoders(self) -> usize {
        match self {
            Self::Sticker | Self::Dice => 2,
            Self::Emoji => 3,
        }
    }
}

/// A chat-list preview's custom emoji: stills (and emoji still decoding)
/// as images, decoded animations for the sidebar's animation layer.
#[derive(Default)]
pub(super) struct PreviewEmoji {
    pub(super) still: HashMap<i64, ImageSource>,
    pub(super) layered: HashMap<i64, LayeredClip>,
}

/// A decoded animation in a history row: drawn by the conversation's
/// animation layer when the row is built inside the conversation slice
/// (`anim_layer`), else the current frame, redrawn with the slice.
pub(super) enum AnimatedVisual {
    Image(Arc<RenderImage>),
    Layered(LayeredClip),
}

/// A clip's place in a playback cache: its file, and the colour its frames
/// are tinted with, if any (the live Stars burst's gold). A tinted clip is
/// decoded and tinted by its own background job, and is budgeted and
/// evicted like any other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ClipKey {
    pub(super) file: FileId,
    pub(super) tint: Option<Tint>,
}

impl ClipKey {
    pub(super) fn plain(file: FileId) -> Self {
        Self { file, tint: None }
    }
}

#[derive(Default)]
pub(super) struct StickerPlayback {
    clips: HashMap<ClipKey, StickerClip>,
    jobs: HashMap<ClipKey, StickerJob>,
    failed: std::collections::HashSet<ClipKey>,
    epoch: u64,
}

/// Decoded straight-alpha BGRA frames and their rate.
type DecodedFrames = Result<(Vec<image::RgbaImage>, f64), String>;
impl Drop for StickerJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.child.lock()
            && let Some(child) = slot.as_mut()
        {
            let _ = child.kill();
        }
    }
}
impl QuillApp {
    pub(super) fn stop_sticker_playback(&mut self) {
        for size in [
            PlaybackSize::Sticker,
            PlaybackSize::Emoji,
            PlaybackSize::Dice,
        ] {
            let cache = self.playback_cache_mut(size);
            for (_, clip) in cache.clips.drain() {
                super::image_budget::retire_all(clip.frames.iter().cloned());
            }
            cache.jobs.clear();
            cache.failed.clear();
            cache.epoch = cache.epoch.wrapping_add(1);
        }
    }

    fn playback_cache(&self, size: PlaybackSize) -> &StickerPlayback {
        match size {
            PlaybackSize::Sticker => &self.playback.sticker_playback,
            PlaybackSize::Emoji => &self.playback.emoji_playback,
            PlaybackSize::Dice => &self.playback.dice_playback,
        }
    }

    fn playback_cache_mut(&mut self, size: PlaybackSize) -> &mut StickerPlayback {
        match size {
            PlaybackSize::Sticker => &mut self.playback.sticker_playback,
            PlaybackSize::Emoji => &mut self.playback.emoji_playback,
            PlaybackSize::Dice => &mut self.playback.dice_playback,
        }
    }

    /// Drop the least recently shown clip; its frames leave the atlas
    /// once nothing can show them (`image_budget`). False when every clip
    /// was shown just now: evicting one would only decode it again next
    /// frame, forever (more animations on screen than the cache holds).
    fn evict_oldest_clip(&mut self, size: PlaybackSize) -> bool {
        const ON_SCREEN: Duration = Duration::from_millis(500);
        let oldest = self
            .playback_cache(size)
            .clips
            .iter()
            .filter(|(_, clip)| clip.used.get().elapsed() >= ON_SCREEN)
            .min_by_key(|(_, clip)| clip.used.get())
            .map(|(id, _)| *id);
        match oldest.and_then(|id| self.playback_cache_mut(size).clips.remove(&id)) {
            Some(clip) => {
                super::image_budget::retire_all(clip.frames.iter().cloned());
                true
            }
            None => false,
        }
    }

    /// Evict least recently shown clips until one more of `bytes` fits
    /// the cache's count and byte budget. False when the rest were all
    /// shown just now.
    fn make_room(&mut self, size: PlaybackSize, bytes: usize) -> bool {
        loop {
            let cache = self.playback_cache(size);
            let held: usize = cache.clips.values().map(|clip| clip.bytes).sum();
            if !over_budget(
                cache.clips.len(),
                held,
                bytes,
                size.capacity(),
                size.budget_bytes(),
            ) {
                return true;
            }
            if !self.evict_oldest_clip(size) {
                return false;
            }
        }
    }

    fn ensure_sticker_playback(
        &mut self,
        key: ClipKey,
        format: StickerFormat,
        size: PlaybackSize,
        cx: &mut Context<Self>,
    ) {
        // ponytail: sixteen resident clips and two decoders; larger visible grids may re-decode evicted images.
        if let Some(clip) = self.playback_cache_mut(size).clips.get_mut(&key) {
            clip.used.set(Instant::now());
            return;
        }
        let cache = self.playback_cache(size);
        if cache.jobs.contains_key(&key)
            || cache.failed.contains(&key)
            || cache.jobs.len() >= size.decoders()
        {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        let id = key.file;
        let path = session
            .media
            .files
            .get(&id.0)
            .and_then(|f| f.usable_path())
            .and_then(|p| sandboxed_display_path(p, &self.media_display_roots()));
        let Some(path) = path else {
            if !session.settings.media_prefs.data_saver
                && session.should_download(id)
                && let Some(live) = self.live.as_mut()
            {
                let _ = live.driver.download_file(id, 1);
            }
            return;
        };
        let edge = size.edge();
        let max_frames = size.max_frames();
        let tint = key.tint.map_or_else(String::new, |Tint([b, g, r])| {
            format!("-{r:02x}{g:02x}{b:02x}")
        });
        let dir = quill::animation::gif_frame_cache_dir(id.0)
            .join(format!("sticker-{edge}-{}{tint}", cache.epoch));
        self.spawn_clip_job(
            key,
            size,
            move |cancel, child| {
                let decoded = decode_frames(format, &path, &dir, edge, max_frames, child, cancel);
                let _ = std::fs::remove_dir_all(dir);
                decoded
            },
            cx,
        );
    }

    /// Runs `decode` in the background, tints its frames there when `key`
    /// asks for a tint, and stores the clip. Nothing is stored when the
    /// cache is full of clips on screen (the caller shows a still).
    pub(super) fn spawn_clip_job(
        &mut self,
        key: ClipKey,
        size: PlaybackSize,
        decode: impl FnOnce(&AtomicBool, &Arc<Mutex<Option<std::process::Child>>>) -> DecodedFrames
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        let epoch = self.playback_cache(size).epoch;
        if !self.make_room(size, 0) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(None));
        self.playback_cache_mut(size).jobs.insert(
            key,
            StickerJob {
                cancel: cancel.clone(),
                child: child.clone(),
            },
        );
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    decode(&cancel, &child)
                        .and_then(|(frames, fps)| finish_clip(frames, fps, key.tint))
                })
                .await;
            let duration = this
                .update(cx, |this, cx| {
                    if this.playback_cache(size).epoch != epoch {
                        return None;
                    }
                    this.playback_cache_mut(size).jobs.remove(&key);
                    let duration = match result {
                        Ok((frames, bytes, fps, duration)) => {
                            let clip = StickerClip {
                                frames,
                                bytes,
                                fps,
                                used: Rc::new(Cell::new(Instant::now())),
                                started: Instant::now(),
                                duration,
                            };
                            // Clips shown meanwhile keep their place: the
                            // cache then runs over until the next decode
                            // finds room.
                            let _ = this.make_room(size, bytes);
                            this.playback_cache_mut(size).clips.insert(key, clip);
                            Some(duration)
                        }
                        Err(error) => {
                            this.playback_cache_mut(size).failed.insert(key);
                            this.connection.set_note(StatusNote::error(error));
                            None
                        }
                    };
                    cx.notify();
                    duration
                })
                .ok()
                .flatten();
            if let Some(duration) = duration {
                cx.background_executor().timer(duration).await;
                let _ = this.update(cx, |this, cx| {
                    if this.playback_cache(size).epoch == epoch {
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}

/// An animated sticker's frames at `edge` px, straight-alpha BGRA.
fn decode_frames(
    format: StickerFormat,
    path: &std::path::Path,
    dir: &std::path::Path,
    edge: u32,
    max_frames: usize,
    child: &Arc<Mutex<Option<std::process::Child>>>,
    cancel: &AtomicBool,
) -> DecodedFrames {
    match format {
        StickerFormat::Tgs => {
            let decoded = quill::sticker_playback::decode_tgs_sized(
                path,
                edge as usize,
                max_frames,
                quill::sticker_playback::MAX_PLAYBACK_FPS,
                cancel,
            )?;
            let frames = decoded
                .frames
                .into_iter()
                .map(|bytes| {
                    image::RgbaImage::from_raw(edge, edge, bytes)
                        .ok_or_else(|| tr(keys::Q_AV_INVALID_STICKER_FRAME).to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((frames, decoded.fps))
        }
        StickerFormat::Webm => {
            let decoded = quill::sticker_playback::decode_webm_sized(
                path,
                dir,
                edge as usize,
                max_frames,
                child,
                cancel,
            )
            .map_err(|error| {
                if cancel.load(Ordering::SeqCst) {
                    tr(keys::Q_AV_STICKER_PLAYBACK_CANCELLED).to_string()
                } else {
                    error
                }
            })?;
            let fps = decoded.fps;
            // Already BGRA (GPUI's byte order), centered on the transparent
            // square.
            let frames = decoded
                .into_squares(edge)
                .into_iter()
                .map(|bgra| {
                    image::RgbaImage::from_raw(edge, edge, bgra)
                        .ok_or_else(|| tr(keys::Q_AV_INVALID_STICKER_FRAME).to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((frames, fps))
        }
        _ => Err(tr(keys::Q_AV_UNSUPPORTED_ANIMATED_STICKER).into()),
    }
}

/// Decoded frames as a clip: tinted when asked, one image per frame, with
/// their size in bytes and the clip's length.
fn finish_clip(
    mut frames: Vec<image::RgbaImage>,
    fps: f64,
    tint: Option<Tint>,
) -> Result<(Arc<[Arc<RenderImage>]>, usize, f64, Duration), String> {
    if frames.is_empty() {
        return Err(tr(keys::Q_AV_STICKER_HAS_NO_FRAMES).to_string());
    }
    if let Some(tint) = tint {
        for frame in &mut frames {
            tint.apply(frame);
        }
    }
    let duration = Duration::from_secs_f64(frames.len() as f64 / fps);
    let bytes = frames.iter().map(|frame| frame.as_raw().len()).sum();
    let frames: Arc<[Arc<RenderImage>]> = frames
        .into_iter()
        .map(|rgba| {
            Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
                rgba,
            )])))
        })
        .collect::<Vec<_>>()
        .into();
    Ok((frames, bytes, fps, duration))
}

impl QuillApp {
    pub(super) fn sticker_image(
        &self,
        id: FileId,
        format: StickerFormat,
        cx: &mut Context<QuillApp>,
    ) -> Option<Arc<RenderImage>> {
        // Battery and animations: the sticker panel shows stills.
        if power_saving::on(power_saving::Flag::StickersPanel) {
            return None;
        }
        self.animated_image(
            id,
            format,
            PlaybackSize::Sticker,
            Looping::Setting,
            true,
            cx,
        )
    }

    /// A history row's animated sticker: for the conversation's animation
    /// layer when the row is built inside the conversation slice, else the
    /// current frame. `None` while it decodes (the row shows the still).
    pub(super) fn history_sticker(
        &self,
        id: FileId,
        format: StickerFormat,
        cx: &mut Context<QuillApp>,
    ) -> Option<AnimatedVisual> {
        self.history_animated(id, format, PlaybackSize::Sticker, None, cx)
    }

    /// One sticker of a dice: the roll loops and the landing plays once
    /// and rests on its last frame (the rolled value), whatever the loop
    /// setting says.
    pub(super) fn history_dice_sticker(
        &self,
        id: FileId,
        format: StickerFormat,
        phase: DicePhase,
        cx: &mut Context<QuillApp>,
    ) -> Option<AnimatedVisual> {
        self.history_animated(id, format, PlaybackSize::Dice, Some(phase), cx)
    }

    fn history_animated(
        &self,
        id: FileId,
        format: StickerFormat,
        size: PlaybackSize,
        dice: Option<DicePhase>,
        cx: &mut Context<QuillApp>,
    ) -> Option<AnimatedVisual> {
        // Battery and animations: stickers and emoji in messages stay
        // still (the row shows the preview image). A dice is not a sticker
        // autoplay in tdesktop and always plays.
        let looping = match dice {
            None if power_saving::on(size.chat_flag()) => return None,
            None => Looping::Setting,
            Some(DicePhase::Rolling) => Looping::Always,
            Some(DicePhase::Landing) => Looping::Never,
        };
        if self.slices.in_conversation() && super::anim_layer::current().is_some() {
            self.layered_clip(id, format, size, looping.loops(self), cx)
                .map(AnimatedVisual::Layered)
        } else {
            self.animated_image(id, format, size, looping, true, cx)
                .map(AnimatedVisual::Image)
        }
    }

    /// Decoded animations for the custom emoji in `message`'s text, by
    /// custom emoji id. Emoji still decoding (or static ones) are absent;
    /// the text shows their still image meanwhile.
    pub(super) fn message_custom_emoji_frames(
        &self,
        message: &quill::state::HistoryMessage,
        cx: &mut Context<QuillApp>,
    ) -> HashMap<i64, AnimatedVisual> {
        use quill::telegram::envelope::MessageContent;
        use quill::text::TextEntityKind;
        let mut out = HashMap::new();
        let MessageContent::Text(text) = &message.content else {
            return out;
        };
        let Some(session) = self.session() else {
            return out;
        };
        let wanted: Vec<_> = text
            .entities
            .iter()
            .filter_map(|entity| match entity.kind {
                TextEntityKind::CustomEmoji { custom_emoji_id } => Some(custom_emoji_id),
                _ => None,
            })
            .filter_map(|id| {
                session
                    .stickers
                    .emoji
                    .custom_emoji_stickers
                    .iter()
                    .find(|item| item.custom_emoji_id == Some(id))
                    .map(|item| (id, item.file_id, item.format))
            })
            .collect();
        for (id, file_id, format) in wanted {
            if out.contains_key(&id) {
                continue;
            }
            if let Some(visual) =
                self.history_animated(file_id, format, PlaybackSize::Emoji, None, cx)
            {
                out.insert(id, visual);
            }
        }
        out
    }

    /// Images for the custom emoji in a chat-list preview: animated when
    /// decoded, the still meanwhile. Built inside the chat list slice, the
    /// animated ones are painted by its animation layer (`anim_layer`), so
    /// their ticks don't rebuild the list.
    pub(super) fn preview_emoji_images(
        &self,
        chat: &quill::state::ChatSummary,
        cx: &mut Context<QuillApp>,
    ) -> PreviewEmoji {
        self.sidebar_emoji_images(&chat.last_preview_style.entities, cx)
    }

    /// [`Self::preview_emoji_images`] for the custom emoji a forum row shows
    /// as topic icons.
    pub(super) fn topic_icon_images(
        &self,
        icons: &[i64],
        cx: &mut Context<QuillApp>,
    ) -> PreviewEmoji {
        let entities: Vec<quill::text::TextEntity> = icons
            .iter()
            .map(|id| quill::text::TextEntity {
                utf8_start: 0,
                utf8_end: 0,
                kind: quill::text::TextEntityKind::CustomEmoji {
                    custom_emoji_id: *id,
                },
            })
            .collect();
        self.sidebar_emoji_images(&entities, cx)
    }

    fn sidebar_emoji_images(
        &self,
        entities: &[quill::text::TextEntity],
        cx: &mut Context<QuillApp>,
    ) -> PreviewEmoji {
        if !self.slices.in_sidebar()
            || super::anim_layer::current().is_none()
            || power_saving::on(power_saving::Flag::EmojiChat)
        {
            return PreviewEmoji {
                still: self.custom_emoji_images(entities, cx),
                layered: HashMap::new(),
            };
        }
        use quill::text::TextEntityKind;
        let mut out = PreviewEmoji::default();
        let Some(session) = self.session() else {
            return out;
        };
        let roots = self.media_display_roots();
        let looping = session.settings.media_prefs.loop_animated_stickers;
        for entity in entities {
            let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
                continue;
            };
            let Some(item) = session
                .stickers
                .emoji
                .custom_emoji_stickers
                .iter()
                .find(|item| item.custom_emoji_id == Some(custom_emoji_id))
            else {
                continue;
            };
            if let Some(clip) =
                self.layered_clip(item.file_id, item.format, PlaybackSize::Emoji, looping, cx)
            {
                out.layered.insert(custom_emoji_id, clip);
                continue;
            }
            let still = item
                .display_file_id()
                .and_then(|file| session.media.files.get(&file.0))
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, &roots))
                .map(ImageSource::from);
            if let Some(still) = still {
                out.still.insert(custom_emoji_id, still);
            }
        }
        out
    }

    /// The decoded clip of an animated sticker or custom emoji, for an
    /// animation layer (which keeps it playing); starts decoding it when
    /// missing.
    pub(super) fn layered_clip(
        &self,
        id: FileId,
        format: StickerFormat,
        size: PlaybackSize,
        looping: bool,
        cx: &mut Context<QuillApp>,
    ) -> Option<LayeredClip> {
        if !matches!(format, StickerFormat::Tgs | StickerFormat::Webm) || id.0 == 0 {
            return None;
        }
        let key = ClipKey::plain(id);
        let clip = self.decoded_clip(key, size, looping);
        self.request_clip(key, format, size, cx);
        clip
    }

    /// Starts decoding `key` after this frame unless it is cached, being
    /// decoded or failed. A cached clip counts as shown just now.
    pub(super) fn request_clip(
        &self,
        key: ClipKey,
        format: StickerFormat,
        size: PlaybackSize,
        cx: &mut Context<QuillApp>,
    ) {
        if !matches!(format, StickerFormat::Tgs | StickerFormat::Webm) || key.file.0 == 0 {
            return;
        }
        let weak = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = weak.update(cx, |this, cx| {
                this.ensure_sticker_playback(key, format, size, cx)
            });
        });
    }

    /// The clip of `key` when it is decoded, without asking for it; its
    /// `started` is when the decode finished.
    pub(super) fn decoded_clip(
        &self,
        key: ClipKey,
        size: PlaybackSize,
        looping: bool,
    ) -> Option<LayeredClip> {
        self.playback_cache(size)
            .clips
            .get(&key)
            .map(|clip| LayeredClip {
                frames: clip.frames.clone(),
                fps: clip.fps,
                started: clip.started,
                duration: clip.duration,
                looping,
                loops: None,
                shown: Some(clip.used.clone()),
            })
    }

    /// Whether `key` is decoded or failed to decode.
    #[cfg(feature = "demo-capture")]
    pub(super) fn clip_settled(&self, key: ClipKey, size: PlaybackSize) -> bool {
        let cache = self.playback_cache(size);
        cache.clips.contains_key(&key) || cache.failed.contains(&key)
    }

    /// Images for the custom emoji among `entities`: animated when
    /// decoded, the still meanwhile; unresolved ones are absent.
    pub(super) fn custom_emoji_images(
        &self,
        entities: &[quill::text::TextEntity],
        cx: &mut Context<QuillApp>,
    ) -> HashMap<i64, ImageSource> {
        use quill::text::TextEntityKind;
        let mut out = HashMap::new();
        let Some(session) = self.session() else {
            return out;
        };
        let roots = self.media_display_roots();
        for entity in entities {
            let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
                continue;
            };
            let Some(item) = session
                .stickers
                .emoji
                .custom_emoji_stickers
                .iter()
                .find(|item| item.custom_emoji_id == Some(custom_emoji_id))
            else {
                continue;
            };
            let source = (!power_saving::on(power_saving::Flag::EmojiChat))
                .then(|| self.custom_emoji_image(item.file_id, item.format, cx))
                .flatten()
                .map(ImageSource::from)
                .or_else(|| {
                    item.display_file_id()
                        .and_then(|file| session.media.files.get(&file.0))
                        .and_then(|file| file.usable_path())
                        .and_then(|path| sandboxed_display_path(path, &roots))
                        .map(ImageSource::from)
                });
            if let Some(source) = source {
                out.insert(custom_emoji_id, source);
            }
        }
        out
    }

    /// The animated frames of a custom emoji (small, many on screen).
    pub(super) fn custom_emoji_image(
        &self,
        id: FileId,
        format: StickerFormat,
        cx: &mut Context<QuillApp>,
    ) -> Option<Arc<RenderImage>> {
        self.animated_image(id, format, PlaybackSize::Emoji, Looping::Setting, true, cx)
    }

    /// [`Self::custom_emoji_image`] for a cell that is built but not on
    /// screen (list overdraw): the first frame, no frame-clock request.
    pub(super) fn custom_emoji_image_parked(
        &self,
        id: FileId,
        format: StickerFormat,
        cx: &mut Context<QuillApp>,
    ) -> Option<Arc<RenderImage>> {
        self.animated_image(id, format, PlaybackSize::Emoji, Looping::Setting, false, cx)
    }

    fn animated_image(
        &self,
        id: FileId,
        format: StickerFormat,
        size: PlaybackSize,
        looping: Looping,
        play: bool,
        cx: &mut Context<QuillApp>,
    ) -> Option<Arc<RenderImage>> {
        if !matches!(format, StickerFormat::Tgs | StickerFormat::Webm) || id.0 == 0 {
            return None;
        }
        let app = self;
        let mut animating = false;
        let key = ClipKey::plain(id);
        let image = app.playback_cache(size).clips.get(&key).and_then(|clip| {
            let elapsed = clip.started.elapsed();
            let looping = looping.loops(app);
            let count = clip.frames.len();
            let index = if !play {
                0
            } else if !looping && elapsed >= clip.duration {
                count.saturating_sub(1)
            } else {
                animating = count > 1;
                (elapsed.as_secs_f64() * clip.fps) as usize % count.max(1)
            };
            clip.frames.get(index).cloned()
        });
        if animating && app.frame.window_active.get() {
            // Clips play at their decoded rate, at most 30 fps
            // (`MAX_PLAYBACK_FPS`; Lottie files are usually 60).
            let fps = match size {
                PlaybackSize::Sticker | PlaybackSize::Dice => 60,
                PlaybackSize::Emoji => 30,
            };
            let fps = image
                .as_ref()
                .and(app.playback_cache(size).clips.get(&key))
                .map_or(fps, |clip| fps.min(clip.fps.ceil() as u32));
            app.request_animation_tick(fps, cx);
        }
        app.request_clip(key, format, size, cx);
        image
    }
}

/// Whether a clip starts over at its end.
#[derive(Clone, Copy)]
enum Looping {
    /// As Settings → Chat → "Loop animated stickers" says.
    Setting,
    Always,
    Never,
}

impl Looping {
    fn loops(self, app: &QuillApp) -> bool {
        match self {
            Self::Setting => app
                .session()
                .is_none_or(|s| s.settings.media_prefs.loop_animated_stickers),
            Self::Always => true,
            Self::Never => false,
        }
    }
}

/// Whether `clips` clips holding `held` bytes leave no room for one more
/// of `adding` bytes (at least one clip always fits an empty cache).
fn over_budget(clips: usize, held: usize, adding: usize, capacity: usize, budget: usize) -> bool {
    clips > 0 && (clips >= capacity || held + adding > budget)
}

#[cfg(test)]
mod tests {
    use super::{finish_clip, over_budget};
    use quill::sticker_playback::Tint;

    #[test]
    fn a_tinted_clip_is_tinted_and_counted_in_the_budget() {
        let white = image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]));
        let (frames, bytes, _, duration) =
            finish_clip(vec![white; 3], 30., Some(Tint([0, 184, 255]))).unwrap();
        assert_eq!(bytes, 3 * 2 * 2 * 4);
        assert_eq!(duration, std::time::Duration::from_millis(100));
        assert_eq!(
            frames[2].as_bytes(0).unwrap().get(..4),
            Some(&[0, 184, 255, 255][..])
        );
        assert!(finish_clip(Vec::new(), 30., None).is_err());
    }

    #[test]
    fn clip_cache_is_bounded_by_count_and_bytes() {
        const MB: usize = 1024 * 1024;
        assert!(
            !over_budget(0, 0, 100 * MB, 16, 48 * MB),
            "one clip always fits"
        );
        assert!(!over_budget(3, 20 * MB, 6 * MB, 16, 48 * MB));
        assert!(over_budget(8, 44 * MB, 6 * MB, 16, 48 * MB), "bytes");
        assert!(over_budget(16, MB, 0, 16, 48 * MB), "count");
        // Checking room before a decode (size unknown yet).
        assert!(over_budget(7, 48 * MB + 1, 0, 16, 48 * MB));
    }
}
