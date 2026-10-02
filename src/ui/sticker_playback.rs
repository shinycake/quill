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
        self.sticker_playback.clips.clear();
        self.sticker_playback.jobs.clear();
        self.sticker_playback.failed.clear();
        self.sticker_playback.epoch = self.sticker_playback.epoch.wrapping_add(1);
    }
    fn ensure_sticker_playback(
        &mut self,
        id: FileId,
        format: StickerFormat,
        cx: &mut Context<Self>,
    ) {
        // ponytail: sixteen resident clips and two decoders; larger visible grids may re-decode evicted images.
        if let Some(clip) = self.sticker_playback.clips.get_mut(&id.0) {
            clip.used = Instant::now();
            return;
        }
        let cache = &self.sticker_playback;
        if cache.clips.contains_key(&id.0)
            || cache.jobs.contains_key(&id.0)
            || cache.failed.contains(&id.0)
            || cache.jobs.len() >= 2
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
        if self.sticker_playback.clips.len() >= 16 {
            if let Some(old) = self
                .sticker_playback
                .clips
                .iter()
                .min_by_key(|(_, clip)| clip.used)
                .map(|(id, _)| *id)
            {
                self.sticker_playback.clips.remove(&old);
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(None));
        self.sticker_playback.jobs.insert(
            id.0,
            StickerJob {
                cancel: cancel.clone(),
                child: child.clone(),
            },
        );
        let dir = quill::animation::gif_frame_cache_dir(id.0).join(format!("sticker-{epoch}"));
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = (|| {
                        let (frames, fps) = match format {
                            StickerFormat::Tgs => {
                                let decoded = quill::sticker_playback::decode_tgs(&path, &cancel)?;
                                let frames = decoded
                                    .frames
                                    .into_iter()
                                    .map(|bytes| {
                                        let mut rgba = image::RgbaImage::from_raw(128, 128, bytes)
                                            .ok_or_else(|| "Invalid sticker frame".to_string())?;
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
                                let decoded = quill::sticker_playback::decode_webm(
                                    &path, &dir, &child, &cancel,
                                )?;
                                let mut frames = Vec::with_capacity(decoded.frames.len());
                                for path in decoded.frames {
                                    if cancel.load(Ordering::SeqCst) {
                                        return Err("Sticker playback cancelled".into());
                                    }
                                    let rgba =
                                        image::open(path).map_err(|e| e.to_string())?.into_rgba8();
                                    let mut square = image::RgbaImage::new(128, 128);
                                    image::imageops::overlay(
                                        &mut square,
                                        &rgba,
                                        i64::from((128 - rgba.width().min(128)) / 2),
                                        i64::from((128 - rgba.height().min(128)) / 2),
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
                    if this.sticker_playback.epoch != epoch {
                        return None;
                    }
                    this.sticker_playback.jobs.remove(&id.0);
                    let duration = match result {
                        Ok(clip) => {
                            let duration = clip.duration;
                            if this.sticker_playback.clips.len() >= 16 {
                                if let Some(old) = this
                                    .sticker_playback
                                    .clips
                                    .iter()
                                    .min_by_key(|(_, clip)| clip.used)
                                    .map(|(id, _)| *id)
                                {
                                    this.sticker_playback.clips.remove(&old);
                                }
                            }
                            this.sticker_playback.clips.insert(id.0, clip);
                            Some(duration)
                        }
                        Err(error) => {
                            this.sticker_playback.failed.insert(id.0);
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
                    if this.sticker_playback.epoch == epoch {
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
        if !matches!(format, StickerFormat::Tgs | StickerFormat::Webm) || id.0 == 0 {
            return None;
        }
        let entity = cx.entity();
        let app = self;
        let image = app.sticker_playback.clips.get(&id.0).map(|clip| {
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
                let _ = weak.update(cx, |this, cx| this.ensure_sticker_playback(id, format, cx));
            });
        }
        image
    }
}
