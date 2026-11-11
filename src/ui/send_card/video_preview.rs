//! The first frame and length of each video in a send tray (tdesktop
//! `PrepareDetails` reads a video's cover and duration for the send box).
//! macOS decodes with AVFoundation, which every Mac has; Linux and Windows
//! cut the frame with the bundled ffmpeg. The frame is a JPEG on disk, shown
//! like a photo, and removed when its file leaves every tray.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use gpui_kit::*;
use quill::composer::AttachmentKind;
use std::path::{Path, PathBuf};

/// What reading a tray video found; either part may be missing (a file the
/// decoder can't read still sends, it just shows its kind).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct VideoPreview {
    /// The first frame, upright, at most 640 px (`FRAME_SIDE`, macOS) on its long side.
    pub(crate) frame: Option<Frame>,
    /// Whole seconds.
    pub(crate) duration: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) path: PathBuf,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Big enough for the largest tile on a Retina screen.
#[cfg(target_os = "macos")]
const FRAME_SIDE: u32 = 640;

impl QuillApp {
    /// Start reading every tray video (and round video) that has not been
    /// read. Like the other tray reads, the answer is kept by path for
    /// every tray and window.
    pub(in crate::ui) fn read_tray_video_previews(&mut self, cx: &mut Context<Self>) {
        let unread: Vec<PathBuf> = self
            .composer_sends
            .attachments()
            .filter(|attachment| {
                matches!(
                    attachment.kind,
                    AttachmentKind::Video | AttachmentKind::VideoNote
                )
            })
            .map(|attachment| attachment.path.clone())
            .filter(|path| self.composer_sends.reads.previews.is_unread(path))
            .collect();
        for path in unread {
            let reads = &mut self.composer_sends.reads;
            reads.preview_jobs += 1;
            let job = reads.preview_jobs;
            reads.previews.start(path.clone(), job);
            let source = path.clone();
            let worker = cx.background_spawn(async move { read_preview(&source) });
            self.spawn_scoped(cx, async move |this, cx| {
                let preview = worker.await;
                let _ = this.update(cx, |this, cx| {
                    let stale = preview.frame.as_ref().map(|frame| frame.path.clone());
                    if this
                        .composer_sends
                        .reads
                        .previews
                        .finish(&path, job, preview)
                    {
                        cx.notify();
                    } else if let Some(stale) = stale {
                        // The file left its tray meanwhile.
                        let _ = std::fs::remove_file(stale);
                    }
                });
            })
            .detach();
        }
    }
}

/// Remove the frame a forgotten read made.
pub(in crate::ui) fn discard(preview: &VideoPreview) {
    if let Some(frame) = &preview.frame {
        let _ = std::fs::remove_file(&frame.path);
    }
}

fn read_preview(path: &Path) -> VideoPreview {
    #[cfg(target_os = "macos")]
    if let Some(preview) = macos::read(path) {
        return preview;
    }
    read_with_ffmpeg(path)
}

fn read_with_ffmpeg(path: &Path) -> VideoPreview {
    let duration = quill::video_edit::probe_source(path)
        .map(|source| i32::try_from((source.duration_ms + 500) / 1000).unwrap_or(i32::MAX));
    let frame = quill::video_edit::render_frame(path, 0, 0, false)
        .ok()
        .and_then(frame_at);
    VideoPreview { frame, duration }
}

fn frame_at(path: PathBuf) -> Option<Frame> {
    match image::image_dimensions(&path) {
        Ok((width, height)) if width > 0 && height > 0 => Some(Frame {
            path,
            width,
            height,
        }),
        _ => {
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{FRAME_SIDE, VideoPreview};
    use objc2_av_foundation::{AVAssetImageGenerator, AVURLAsset};
    use objc2_core_media::CMTime;
    use objc2_foundation::{NSString, NSURL};
    use std::path::Path;

    /// The first frame and length through AVFoundation; `None` when it
    /// can't open the file (a format only ffmpeg knows).
    pub(super) fn read(path: &Path) -> Option<VideoPreview> {
        let path_str = path.to_str()?;
        // SAFETY: an asset and a synchronous image request; AVFoundation
        // allows both off the main thread, and `actual_time` may be null.
        #[allow(deprecated)]
        let (image, seconds) = unsafe {
            let url = NSURL::fileURLWithPath(&NSString::from_str(path_str));
            let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
            let generator = AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset);
            generator.setAppliesPreferredTrackTransform(true);
            let image = generator
                .copyCGImageAtTime_actualTime_error(
                    CMTime::with_seconds(0.0, 600),
                    std::ptr::null_mut(),
                )
                .ok()?;
            (image, asset.duration().seconds())
        };
        let picture = crate::ui::image_decode::draw_srgb(&image).ok()?;
        let (width, height) = picture.dimensions();
        let scale = (f64::from(FRAME_SIDE) / f64::from(width.max(height).max(1))).min(1.0);
        let picture = image::imageops::thumbnail(
            &picture,
            ((f64::from(width) * scale).round() as u32).max(1),
            ((f64::from(height) * scale).round() as u32).max(1),
        );
        let dir = quill::scratch::temp_work_dir("quill-send-previews");
        let jpeg = crate::ui::send_box_thumbnails::write_jpeg(&picture, &dir)?;
        let duration = (seconds.is_finite() && seconds >= 0.0).then(|| seconds.round() as i32);
        Some(VideoPreview {
            frame: super::frame_at(jpeg),
            duration,
        })
    }
}
