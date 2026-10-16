//! Thumbnails for pictures sent as files (tdesktop `PrepareFileThumbnail` /
//! `FinalizeFileThumbnail`). Telegram makes the thumbnail of a small JPEG,
//! PNG, GIF or WebP file itself; for a HEIC, AVIF, JPEG XL or large picture
//! the sender uploads one, or the chat shows a plain file row. Quill makes
//! it in the background when the file joins the tray: the picture decoded
//! at 320 px on its long side, JPEG quality 87. The file itself is sent as
//! it is.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::image_decode::{self, DecodeSize};
use gpui_kit::*;
use quill::composer::{
    AttachmentKind, FILE_THUMBNAIL_SIDE, FileThumbnail, file_thumbnail_upload_required,
};
use quill::media_viewer::is_image_file;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// tdesktop `kThumbnailQuality`.
const QUALITY: u8 = 87;

/// Sending waits at most this long for a thumbnail; a decoder stuck on a
/// file must not hold the batch.
const GIVE_UP_AFTER: Duration = Duration::from_secs(15);

impl QuillApp {
    /// Start a thumbnail for each picture in the tray that goes as a file
    /// and needs one. Sending waits for them, like it waits for the video
    /// sound checks.
    pub(super) fn prepare_tray_file_thumbnails(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<PathBuf> = self
            .composer_sends
            .attachments()
            .filter(|attachment| attachment.kind == AttachmentKind::Document)
            .filter(|attachment| attachment.extras.file_thumbnail.is_none())
            .filter(|attachment| is_image_file(&attachment.file_name, ""))
            .map(|attachment| attachment.path.clone())
            .filter(|path| {
                let send_box = &self.composer_ui.send_box;
                !send_box.thumbnailing.contains_key(path) && !send_box.thumbnailed.contains(path)
            })
            .collect();
        for path in paths {
            let send_box = &mut self.composer_ui.send_box;
            send_box.thumbnail_jobs += 1;
            let job = send_box.thumbnail_jobs;
            send_box.thumbnailing.insert(path.clone(), job);
            let source = path.clone();
            let worker = cx.background_spawn(async move { write_file_thumbnail(&source) });
            let timer = cx.background_executor().timer(GIVE_UP_AFTER);
            let waited = path.clone();
            self.spawn_scoped(cx, async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |this, cx| {
                    let send_box = &mut this.composer_ui.send_box;
                    if send_box.settle_thumbnail(&waited, job) {
                        cx.notify();
                    }
                });
            })
            .detach();
            self.spawn_scoped(cx, async move |this, cx| {
                let thumbnail = worker.await;
                let _ = this.update(cx, |this, cx| {
                    let send_box = &mut this.composer_ui.send_box;
                    // The batch may have been sent or cleared, or the job
                    // given up on, meanwhile.
                    if !send_box.settle_thumbnail(&path, job) {
                        return;
                    }
                    if let Some(thumbnail) = thumbnail {
                        for attachment in this.composer_sends.attachments_at_mut(&path) {
                            attachment.extras.file_thumbnail = Some(thumbnail.clone());
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Thumbnails of the open slot's tray files are still being made.
    pub(super) fn tray_thumbnails_pending(&self) -> bool {
        let making = &self.composer_ui.send_box.thumbnailing;
        self.pending_attachments()
            .iter()
            .any(|attachment| making.contains_key(&attachment.path))
    }
}

impl super::send_box_state::SendBoxUi {
    /// End thumbnail job `job` for `path`: `false` when it is no longer
    /// the current one (given up on, superseded, or the batch cleared).
    fn settle_thumbnail(&mut self, path: &Path, job: u64) -> bool {
        if self.thumbnailing.get(path) != Some(&job) {
            return false;
        }
        self.thumbnailing.remove(path);
        self.thumbnailed.insert(path.to_path_buf());
        true
    }
}

/// The thumbnail JPEG in `{media cache}/file-thumbs`, or `None` when the
/// server makes its own or the picture does not decode.
fn write_file_thumbnail(source: &Path) -> Option<FileThumbnail> {
    let size = std::fs::metadata(source).ok()?.len();
    if !file_thumbnail_upload_required(source, size) {
        return None;
    }
    // Decoded with its short side at 320 px (no bigger than needed), then
    // fitted to 320 px on the long side.
    let decode = DecodeSize {
        short_edge: FILE_THUMBNAIL_SIDE,
    };
    let picture = image_decode::decode_file(source, decode).ok()?;
    let (width, height) = picture.dimensions();
    let long = width.max(height).max(1);
    let scale = (FILE_THUMBNAIL_SIDE as f32 / long as f32).min(1.0);
    let thumb_w = ((width as f32 * scale).round() as u32).max(1);
    let thumb_h = ((height as f32 * scale).round() as u32).max(1);
    let thumb = image::imageops::thumbnail(&picture, thumb_w, thumb_h);
    // JPEG has no alpha: a transparent picture goes over white.
    let rgb = image::RgbImage::from_fn(thumb_w, thumb_h, |x, y| {
        let [r, g, b, a] = thumb.get_pixel(x, y).0;
        let over = |c: u8| ((u16::from(c) * u16::from(a) + 255 * (255 - u16::from(a))) / 255) as u8;
        image::Rgb([over(r), over(g), over(b)])
    });
    let dir = quill::local_path::media_cache_base().join("file-thumbs");
    quill::local_path::secure_create_dir(&dir).ok()?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let dest = dir.join(format!("{}-{nanos}.jpg", std::process::id()));
    let file = std::fs::File::create(&dest).ok()?;
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), QUALITY);
    if encoder.encode_image(&rgb).is_err() {
        let _ = std::fs::remove_file(&dest);
        return None;
    }
    let _ = quill::local_path::restrict_file(&dest);
    Some(FileThumbnail {
        path: dest,
        width: i32::try_from(thumb_w).ok()?,
        height: i32::try_from(thumb_h).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::write_file_thumbnail;
    use std::path::Path;

    #[test]
    fn pictures_telegram_cannot_thumbnail_get_a_jpeg() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/halves.jxl");
        let thumb = write_file_thumbnail(&source).expect("a thumbnail");
        // The 32×16 picture is not enlarged.
        assert_eq!((thumb.width, thumb.height), (32, 16));
        let decoded = image::open(&thumb.path).expect("a JPEG");
        assert_eq!(decoded.width(), 32);
        let _ = std::fs::remove_file(&thumb.path);
        // A small PNG gets its thumbnail from the server.
        let png = std::env::temp_dir().join(format!("quill-thumb-{}.png", std::process::id()));
        image::RgbaImage::new(4, 4).save(&png).expect("a PNG");
        assert!(write_file_thumbnail(&png).is_none());
        let _ = std::fs::remove_file(&png);
    }
}
