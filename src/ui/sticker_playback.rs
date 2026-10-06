//! Shared bounded animated sticker images for history and the picker.
use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::FileId;
use quill::local_path::sandboxed_display_path;
use quill::telegram::envelope::StickerFormat;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub(super) struct StickerClip {
    image: Arc<RenderImage>,
    final_image: Arc<RenderImage>,
    started: Instant,
    duration: Duration,
    used: Instant,
}
pub(super) struct StickerJob {
    cancel: Arc<AtomicBool>,
    child: Arc<Mutex<Option<std::process::Child>>>,
}
/// Which playback cache. Stickers decode at 128 px (16 clips). Custom emoji
/// are small and many — every visible one animates, as in Telegram
/// Desktop — so 56 px, at most 36 frames, 160 clips (~70 MB at worst).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PlaybackSize {
    Sticker,
    Emoji,
}

impl PlaybackSize {
    fn edge(self) -> u32 {
        match self {
            Self::Sticker => 128,
            Self::Emoji => 56,
        }
    }

    fn capacity(self) -> usize {
        match self {
            Self::Sticker => 16,
            Self::Emoji => 160,
        }
    }

    fn max_frames(self) -> usize {
        match self {
            Self::Sticker => quill::sticker_playback::MAX_STICKER_FRAMES,
            Self::Emoji => 36,
        }
    }

    /// Concurrent decodes.
    fn decoders(self) -> usize {
        match self {
            Self::Sticker => 2,
            Self::Emoji => 3,
        }
    }
}

#[derive(Default)]
pub(super) struct StickerPlayback {
    clips: HashMap<i32, StickerClip>,
    jobs: HashMap<i32, StickerJob>,
    failed: std::collections::HashSet<i32>,
    epoch: u64,
}
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
        for size in [PlaybackSize::Sticker, PlaybackSize::Emoji] {
            let cache = self.playback_cache_mut(size);
            cache.clips.clear();
            cache.jobs.clear();
            cache.failed.clear();
            cache.epoch = cache.epoch.wrapping_add(1);
        }
    }

    fn playback_cache(&self, size: PlaybackSize) -> &StickerPlayback {
        match size {
            PlaybackSize::Sticker => &self.sticker_playback,
            PlaybackSize::Emoji => &self.emoji_playback,
        }
    }

    fn playback_cache_mut(&mut self, size: PlaybackSize) -> &mut StickerPlayback {
        match size {
            PlaybackSize::Sticker => &mut self.sticker_playback,
            PlaybackSize::Emoji => &mut self.emoji_playback,
        }
    }

    fn ensure_sticker_playback(
        &mut self,
        id: FileId,
        format: StickerFormat,
        size: PlaybackSize,
        cx: &mut Context<Self>,
    ) {
        // ponytail: sixteen resident clips and two decoders; larger visible grids may re-decode evicted images.
        if let Some(clip) = self.playback_cache_mut(size).clips.get_mut(&id.0) {
            clip.used = Instant::now();
            return;
        }
        let cache = self.playback_cache(size);
        if cache.clips.contains_key(&id.0)
            || cache.jobs.contains_key(&id.0)
            || cache.failed.contains(&id.0)
            || cache.jobs.len() >= size.decoders()
        {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        let path = session
            .files
            .get(&id.0)
            .and_then(|f| f.usable_path())
            .and_then(|p| sandboxed_display_path(p, &self.media_display_roots()));
        let Some(path) = path else {
            if !session.media_prefs.data_saver && session.should_download(id) {
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.download_file(id, 1);
                }
            }
            return;
        };
        let epoch = cache.epoch;
        if self.playback_cache(size).clips.len() >= size.capacity() {
            if let Some(old) = self
                .playback_cache(size)
                .clips
                .iter()
                .min_by_key(|(_, clip)| clip.used)
                .map(|(id, _)| *id)
            {
                self.playback_cache_mut(size).clips.remove(&old);
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(None));
        self.playback_cache_mut(size).jobs.insert(
            id.0,
            StickerJob {
                cancel: cancel.clone(),
                child: child.clone(),
            },
        );
        let edge = size.edge();
        let max_frames = size.max_frames();
        let dir =
            quill::animation::gif_frame_cache_dir(id.0).join(format!("sticker-{edge}-{epoch}"));
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = (|| {
                        let (frames, fps) = match format {
                            StickerFormat::Tgs => {
                                let decoded = quill::sticker_playback::decode_tgs_sized(
                                    &path,
                                    edge as usize,
                                    max_frames,
                                    &cancel,
                                )?;
                                let frames = decoded
                                    .frames
                                    .into_iter()
                                    .map(|bytes| {
                                        let mut rgba =
                                            image::RgbaImage::from_raw(edge, edge, bytes)
                                                .ok_or_else(|| {
                                                    "Invalid sticker frame".to_string()
                                                })?;
                                        // rlottie is premultiplied; GPUI expects straight BGRA.
                                        for pixel in rgba.chunks_exact_mut(4) {
                                            let alpha = u16::from(pixel[3]);
                                            if alpha > 0 {
                                                for channel in &mut pixel[..3] {
                                                    *channel = (u16::from(*channel) * 255 / alpha)
                                                        .min(255)
                                                        as u8;
                                                }
                                            }
                                        }
                                        Ok::<_, String>(rgba)
                                    })
                                    .collect::<Result<Vec<_>, _>>()?;
                                (frames, decoded.fps)
                            }
                            StickerFormat::Webm => {
                                let decoded = quill::sticker_playback::decode_webm_sized(
                                    &path,
                                    &dir,
                                    edge as usize,
                                    max_frames,
                                    &child,
                                    &cancel,
                                )?;
                                let mut frames = Vec::with_capacity(decoded.frames.len());
                                for path in decoded.frames {
                                    if cancel.load(Ordering::SeqCst) {
                                        return Err("Sticker playback cancelled".into());
                                    }
                                    let rgba =
                                        image::open(path).map_err(|e| e.to_string())?.into_rgba8();
                                    let mut square = image::RgbaImage::new(edge, edge);
                                    image::imageops::overlay(
                                        &mut square,
                                        &rgba,
                                        i64::from((edge - rgba.width().min(edge)) / 2),
                                        i64::from((edge - rgba.height().min(edge)) / 2),
                                    );
                                    for pixel in square.chunks_exact_mut(4) {
                                        pixel.swap(0, 2);
                                    }
                                    frames.push(square);
                                }
                                (frames, decoded.fps)
                            }
                            _ => return Err("Unsupported animated sticker".into()),
                        };
                        let duration = Duration::from_secs_f64(frames.len() as f64 / fps);
                        let last = frames
                            .last()
                            .ok_or_else(|| "Sticker has no frames".to_string())?
                            .clone();
                        let delay =
                            image::Delay::from_numer_denom_ms((1000.0 / fps).round() as u32, 1);
                        let animated = frames
                            .into_iter()
                            .map(|rgba| image::Frame::from_parts(rgba, 0, 0, delay))
                            .collect::<SmallVec<[_; 1]>>();
                        Ok(StickerClip {
                            image: Arc::new(RenderImage::new(animated)),
                            final_image: Arc::new(RenderImage::new(SmallVec::from_buf([
                                image::Frame::new(last),
                            ]))),
                            used: Instant::now(),
                            started: Instant::now(),
                            duration,
                        })
                    })();
                    let _ = std::fs::remove_dir_all(dir);
                    result
                })
                .await;
            let duration = this
                .update(cx, |this, cx| {
                    if this.playback_cache(size).epoch != epoch {
                        return None;
                    }
                    this.playback_cache_mut(size).jobs.remove(&id.0);
                    let duration = match result {
                        Ok(clip) => {
                            let duration = clip.duration;
                            if this.playback_cache(size).clips.len() >= size.capacity() {
                                if let Some(old) = this
                                    .playback_cache(size)
                                    .clips
                                    .iter()
                                    .min_by_key(|(_, clip)| clip.used)
                                    .map(|(id, _)| *id)
                                {
                                    this.playback_cache_mut(size).clips.remove(&old);
                                }
                            }
                            this.playback_cache_mut(size).clips.insert(id.0, clip);
                            Some(duration)
                        }
                        Err(error) => {
                            this.playback_cache_mut(size).failed.insert(id.0);
                            this.status_note = error;
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

impl QuillApp {
    pub(super) fn sticker_image(
        &self,
        id: FileId,
        format: StickerFormat,
        cx: &mut Context<QuillApp>,
    ) -> Option<Arc<RenderImage>> {
        self.animated_image(id, format, PlaybackSize::Sticker, cx)
    }

    /// Decoded animations for the custom emoji in `message`'s text, by
    /// custom emoji id. Emoji still decoding (or static ones) are absent;
    /// the text shows their still image meanwhile.
    pub(super) fn message_custom_emoji_frames(
        &self,
        message: &quill::state::HistoryMessage,
        cx: &mut Context<QuillApp>,
    ) -> HashMap<i64, Arc<RenderImage>> {
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
                    .emoji
                    .custom_emoji_stickers
                    .iter()
                    .find(|item| item.custom_emoji_id == Some(id))
                    .map(|item| (id, item.file_id, item.format))
            })
            .collect();
        for (id, file_id, format) in wanted {
            if let Some(frames) = self.custom_emoji_image(file_id, format, cx) {
                out.insert(id, frames);
            }
        }
        out
    }

    /// Images for the custom emoji in a chat-list preview: animated when
    /// decoded, the still meanwhile.
    pub(super) fn preview_emoji_images(
        &self,
        chat: &quill::state::ChatSummary,
        cx: &mut Context<QuillApp>,
    ) -> HashMap<i64, ImageSource> {
        use quill::text::TextEntityKind;
        let mut out = HashMap::new();
        let Some(session) = self.session() else {
            return out;
        };
        let roots = self.media_display_roots();
        for entity in &chat.last_preview_style.entities {
            let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
                continue;
            };
            let Some(item) = session
                .emoji
                .custom_emoji_stickers
                .iter()
                .find(|item| item.custom_emoji_id == Some(custom_emoji_id))
            else {
                continue;
            };
            let source = self
                .custom_emoji_image(item.file_id, item.format, cx)
                .map(ImageSource::from)
                .or_else(|| {
                    item.display_file_id()
                        .and_then(|file| session.files.get(&file.0))
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
        self.animated_image(id, format, PlaybackSize::Emoji, cx)
    }

    fn animated_image(
        &self,
        id: FileId,
        format: StickerFormat,
        size: PlaybackSize,
        cx: &mut Context<QuillApp>,
    ) -> Option<Arc<RenderImage>> {
        if !matches!(format, StickerFormat::Tgs | StickerFormat::Webm) || id.0 == 0 {
            return None;
        }
        let entity = cx.entity();
        let app = self;
        let image = app.playback_cache(size).clips.get(&id.0).map(|clip| {
            if app
                .session()
                .is_none_or(|s| s.media_prefs.loop_animated_stickers)
                || clip.started.elapsed() < clip.duration
            {
                clip.image.clone()
            } else {
                clip.final_image.clone()
            }
        });
        {
            let weak = entity.downgrade();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.ensure_sticker_playback(id, format, size, cx)
                });
            });
        }
        image
    }
}
