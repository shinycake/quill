//! Reading photos and videos as they join a tray (tdesktop `PrepareDetails`).
//! A picture Telegram only takes as a JPEG gets one here, at the "High
//! Quality" limit; sending scales it down further if it has to. A file that
//! turns out not to be a photo or video stays in the tray as a file. Sending
//! waits for these reads.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::image_decode::{self, DecodeSize};
use super::send_box_thumbnails::{GIVE_UP_AFTER, write_jpeg};
use gpui_kit::*;
use quill::composer::{
    AttachmentKind, ComposerAttachment, PHOTO_SIDE_LARGE, fitted_photo_size,
    media_dimensions_valid, photo_needs_jpeg,
};
use quill::media_viewer::READ_BYTES_LIMIT;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// What reading a tray file found. Kept by path (`SendBoxUi::media_checked`),
/// so the same file in a second tray gets the same answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MediaCheck {
    /// It goes as it is.
    AsIs,
    /// It goes as this JPEG.
    Jpeg(PathBuf),
    /// It goes as a file.
    FileOnly,
}

impl QuillApp {
    /// Start reading each photo and video in any tray that has not been read.
    pub(super) fn check_tray_media(&mut self, cx: &mut Context<Self>) {
        let unread: Vec<(PathBuf, AttachmentKind)> = self
            .composer_sends
            .attachments()
            .filter(|attachment| needs_media_check(attachment))
            .map(|attachment| (attachment.path.clone(), attachment.kind))
            .collect();
        for (path, kind) in unread {
            if let Some(check) = self.composer_sends.reads.media_checked.get(&path).cloned() {
                self.apply_media_check(&path, &check);
                continue;
            }
            if self.composer_sends.reads.media_checking.contains_key(&path) {
                continue;
            }
            let send_box = &mut self.composer_sends.reads;
            send_box.media_check_jobs += 1;
            let job = send_box.media_check_jobs;
            send_box.media_checking.insert(path.clone(), job);
            let source = path.clone();
            let worker = cx.background_spawn(async move {
                match kind {
                    AttachmentKind::Video => check_video(&source),
                    _ => check_photo(&source),
                }
            });
            let timer = cx.background_executor().timer(GIVE_UP_AFTER);
            let waited = path.clone();
            self.spawn_scoped(cx, async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |this, cx| {
                    this.finish_media_check(&waited, job, MediaCheck::FileOnly, cx);
                });
            })
            .detach();
            self.spawn_scoped(cx, async move |this, cx| {
                let check = worker.await;
                let _ = this.update(cx, |this, cx| {
                    this.finish_media_check(&path, job, check, cx);
                });
            })
            .detach();
        }
    }

    /// Files of the open slot's tray are still being read.
    pub(super) fn tray_media_checks_pending(&self) -> bool {
        let reading = &self.composer_sends.reads.media_checking;
        self.pending_attachments()
            .iter()
            .any(|attachment| reading.contains_key(&attachment.path))
    }

    fn finish_media_check(
        &mut self,
        path: &Path,
        job: u64,
        check: MediaCheck,
        cx: &mut Context<Self>,
    ) {
        let send_box = &mut self.composer_sends.reads;
        if send_box.media_checking.get(path) != Some(&job) {
            return;
        }
        send_box.media_checking.remove(path);
        send_box
            .media_checked
            .insert(path.to_path_buf(), check.clone());
        self.apply_media_check(path, &check);
        // A file now needs its thumbnail.
        self.prepare_tray_file_thumbnails(cx);
        cx.notify();
    }

    fn apply_media_check(&mut self, path: &Path, check: &MediaCheck) {
        for attachment in self.composer_sends.attachments_at_mut(path) {
            match check {
                MediaCheck::AsIs => {}
                MediaCheck::Jpeg(jpeg) => attachment.extras.photo_jpeg = Some(jpeg.clone()),
                MediaCheck::FileOnly => attachment.keep_as_file(),
            }
        }
    }
}

fn needs_media_check(attachment: &ComposerAttachment) -> bool {
    match attachment.kind {
        AttachmentKind::Photo => attachment.extras.photo_jpeg.is_none(),
        AttachmentKind::Video => !attachment.extras.gif,
        AttachmentKind::Document | AttachmentKind::VideoNote => false,
    }
}

/// tdesktop `CheckForVideo` and `ValidVideoForAlbum`.
fn check_video(path: &Path) -> MediaCheck {
    match quill::video::probe_local_video(path) {
        Ok(probe)
            if media_dimensions_valid(
                u32::try_from(probe.width).unwrap_or(0),
                u32::try_from(probe.height).unwrap_or(0),
            ) =>
        {
            MediaCheck::AsIs
        }
        _ => MediaCheck::FileOnly,
    }
}

/// tdesktop `CheckForImage` and `ValidPhotoForAlbum`: a picture that decodes,
/// is at most 64 MiB, is not animated, and is not too narrow.
pub(super) fn check_photo(path: &Path) -> MediaCheck {
    let too_big = std::fs::metadata(path).map_or(true, |meta| meta.len() > READ_BYTES_LIMIT as u64);
    if too_big || is_animated_webp(path) {
        return MediaCheck::FileOnly;
    }
    if !photo_needs_jpeg(path)
        && let Ok(size) = image::image_dimensions(path)
    {
        return if media_dimensions_valid(size.0, size.1)
            && image_decode::area_allowed(size.0, size.1).is_ok()
        {
            MediaCheck::AsIs
        } else {
            MediaCheck::FileOnly
        };
    }
    // Not a format the header reader knows, whatever its name says.
    let decode = DecodeSize {
        short_edge: PHOTO_SIDE_LARGE,
    };
    let Ok(picture) = image_decode::decode_file(path, decode) else {
        return MediaCheck::FileOnly;
    };
    let (width, height) = picture.dimensions();
    if !media_dimensions_valid(width, height) {
        return MediaCheck::FileOnly;
    }
    let (fit_w, fit_h) = fitted_photo_size(width, height, PHOTO_SIDE_LARGE);
    let fitted = image::imageops::thumbnail(&picture, fit_w, fit_h);
    let dir = quill::scratch::temp_work_dir("quill-send-photos");
    write_jpeg(&fitted, &dir).map_or(MediaCheck::FileOnly, MediaCheck::Jpeg)
}

fn is_animated_webp(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    image::codecs::webp::WebPDecoder::new(BufReader::new(file))
        .is_ok_and(|decoder| decoder.has_animation())
}

#[cfg(test)]
mod tests {
    use super::{MediaCheck, check_photo};
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/images")
            .join(name)
    }

    #[test]
    fn a_picture_telegram_cannot_take_goes_as_a_jpeg_of_the_same_size() {
        let MediaCheck::Jpeg(jpeg) = check_photo(&fixture("halves.jxl")) else {
            panic!("JPEG XL decodes everywhere");
        };
        let decoded = image::open(&jpeg).expect("a JPEG");
        assert_eq!((decoded.width(), decoded.height()), (32, 16));
        let _ = std::fs::remove_file(jpeg);
    }

    #[test]
    fn a_picture_is_kept_as_it_is_or_refused_by_what_it_holds() {
        let dir = quill::scratch::temp_work_dir("quill-send-photos");
        std::fs::create_dir_all(&dir).expect("a folder");
        let plain = dir.join("plain.png");
        image::RgbaImage::new(40, 30).save(&plain).expect("a PNG");
        assert_eq!(check_photo(&plain), MediaCheck::AsIs);

        let narrow = dir.join("narrow.png");
        image::RgbaImage::new(210, 10).save(&narrow).expect("a PNG");
        assert_eq!(check_photo(&narrow), MediaCheck::FileOnly);

        let broken = dir.join("broken.jpg");
        std::fs::write(&broken, b"not a picture").expect("a file");
        assert_eq!(check_photo(&broken), MediaCheck::FileOnly);
    }
}
