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
    AttachmentKind, ComposerAttachment, PHOTO_SIDE_LARGE, PhotoPrep, fitted_photo_size,
    media_dimensions_valid, photo_needs_jpeg,
};
use quill::media_viewer::READ_BYTES_LIMIT;
use quill::scratch::TempFile;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// What the worker that read a tray file found. A JPEG it made is a
/// `TempFile`, so an answer nobody takes (the give-up won, the read was
/// restarted, the file left the tray: the stage dropped the task) removes
/// its own file when it is dropped.
#[derive(Debug)]
pub(super) enum MediaCheck {
    /// It goes as it is.
    AsIs,
    /// It goes as this JPEG.
    Jpeg(TempFile),
    /// It goes as a file.
    FileOnly,
}

/// What a settled read of a tray file found. Kept by path
/// (`TrayReads::media`), so the same file in a second tray gets the same
/// answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MediaFound {
    /// It goes as it is.
    AsIs,
    /// It goes as this JPEG.
    Jpeg(PathBuf),
    /// It goes as a file.
    FileOnly,
}

impl MediaCheck {
    /// The answer as the tray keeps it; the JPEG is the tray's to keep now.
    pub(super) fn claim(self) -> MediaFound {
        match self {
            Self::AsIs => MediaFound::AsIs,
            Self::Jpeg(file) => MediaFound::Jpeg(file.claim()),
            Self::FileOnly => MediaFound::FileOnly,
        }
    }
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
            if let Some(found) = self.composer_sends.reads.media.done(&path).cloned() {
                self.apply_media_check(&path, &found);
                continue;
            }
            if self.composer_sends.reads.media.is_reading(&path) {
                continue;
            }
            let send_box = &mut self.composer_sends.reads;
            send_box.media_check_jobs += 1;
            let job = send_box.media_check_jobs;
            let source = path.clone();
            let worker = cx.background_spawn(async move {
                match kind {
                    AttachmentKind::Video => check_video(&source),
                    _ => check_photo(&source),
                }
            });
            let timer = cx.background_executor().timer(GIVE_UP_AFTER);
            let waited = path.clone();
            // The reads belong to the app (`TrayReads`), as the sound reads
            // do: a closed window or a switched account must not drop the
            // answer and leave the read running for good.
            // screen-scope: the tray reads are the app's; the answer lands for
            // every window and account whichever screen is on screen.
            let give_up = Self::spawn_app(cx, async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |this, cx| {
                    this.finish_media_check(&waited, job, MediaCheck::FileOnly, cx);
                });
            });
            let key = path.clone();
            // screen-scope: as the give-up above.
            let answer = Self::spawn_app(cx, async move |this, cx| {
                let check = worker.await;
                let _ = this.update(cx, |this, cx| {
                    this.finish_media_check(&path, job, check, cx);
                });
            });
            self.composer_sends
                .reads
                .media
                .start(key, job, vec![give_up, answer]);
        }
    }

    /// Files of the open slot's tray are still being read.
    pub(super) fn tray_media_checks_pending(&self) -> bool {
        let reading = &self.composer_sends.reads.media;
        self.pending_attachments()
            .iter()
            .any(|attachment| reading.is_reading(&attachment.path))
    }

    fn finish_media_check(
        &mut self,
        path: &Path,
        job: u64,
        check: MediaCheck,
        cx: &mut Context<Self>,
    ) {
        let reads = &mut self.composer_sends.reads;
        // A late answer (the read was given up on, or restarted) is dropped
        // here, and with it the JPEG it made.
        if !reads.media.is_current(path, job) {
            return;
        }
        let found = check.claim();
        reads.media.finish(path, job, found.clone());
        self.apply_media_check(path, &found);
        // A file now needs its thumbnail, and a photo that became a JPEG
        // its size (the JPEG is the picture the offer is about).
        self.prepare_tray_file_thumbnails(cx);
        self.check_tray_photo_sizes(cx);
        cx.notify();
    }

    fn apply_media_check(&mut self, path: &Path, found: &MediaFound) {
        for attachment in self.composer_sends.attachments_at_mut(path) {
            match found {
                MediaFound::AsIs => {}
                MediaFound::Jpeg(jpeg) => {
                    attachment.extras.set_photo(PhotoPrep::Jpeg(jpeg.clone()))
                }
                MediaFound::FileOnly => attachment.keep_as_file(),
            }
        }
    }
}

fn needs_media_check(attachment: &ComposerAttachment) -> bool {
    match attachment.kind {
        AttachmentKind::Photo => attachment.extras.photo_jpeg().is_none(),
        AttachmentKind::Video => !attachment.extras.is_gif(),
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
    write_jpeg(&fitted, &dir).map_or(MediaCheck::FileOnly, |jpeg| {
        MediaCheck::Jpeg(TempFile::new(jpeg))
    })
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
    use super::{MediaCheck, MediaFound, check_photo};
    use crate::ui::send_box_state::Reads;
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
        let jpeg = jpeg.claim();
        let decoded = image::open(&jpeg).expect("a JPEG");
        assert_eq!((decoded.width(), decoded.height()), (32, 16));
        let _ = std::fs::remove_file(jpeg);
    }

    #[test]
    fn a_jpeg_whose_read_was_given_up_on_removes_itself() {
        let mut reads: Reads<MediaFound> = Reads::default();
        let path = fixture("halves.jxl");
        reads.start(path.clone(), 1, Vec::new());
        // The worker is still decoding when the give-up settles the read.
        let late = check_photo(&path);
        let MediaCheck::Jpeg(file) = &late else {
            panic!("JPEG XL decodes everywhere");
        };
        let made = file.path().to_path_buf();
        assert!(made.exists());
        assert!(reads.finish(&path, 1, MediaFound::FileOnly));
        assert!(!reads.is_current(&path, 1), "the give-up settled it");
        drop(late);
        assert!(!made.exists(), "an answer nobody takes removes its file");
    }

    #[test]
    fn a_picture_is_kept_as_it_is_or_refused_by_what_it_holds() {
        let dir = quill::scratch::temp_work_dir("quill-send-photos");
        std::fs::create_dir_all(&dir).expect("a folder");
        let plain = dir.join("plain.png");
        image::RgbaImage::new(40, 30).save(&plain).expect("a PNG");
        assert!(matches!(check_photo(&plain), MediaCheck::AsIs));

        let narrow = dir.join("narrow.png");
        image::RgbaImage::new(210, 10).save(&narrow).expect("a PNG");
        assert!(matches!(check_photo(&narrow), MediaCheck::FileOnly));

        let broken = dir.join("broken.jpg");
        std::fs::write(&broken, b"not a picture").expect("a file");
        assert!(matches!(check_photo(&broken), MediaCheck::FileOnly));
    }
}
