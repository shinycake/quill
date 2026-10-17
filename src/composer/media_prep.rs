//! Everything a media send reads from its files before the request can be
//! built: the scaled copy of a large photo, a video's duration and size, a
//! round video's probe and thumbnail, a GIF's size. These run `ffprobe` and
//! `ffmpeg`, so they belong on a background thread: the composer prepares a
//! snapshot there ([`PreparedMedia::prepare`]), then the driver builds the
//! request from the result without starting a process.
//!
//! A snapshot that carries no [`PreparedMedia`] still works: the driver
//! reads the files itself (tests, share and link sends), each child bounded
//! by a timeout (`child_timeout`).
#![deny(clippy::indexing_slicing)]

use super::{AttachmentKind, ComposerAttachment, ComposerSnapshot, MediaSendOptions};
use crate::video::{VideoNoteProbe, VideoNoteThumbnail, VideoProbe};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// All the files of one send together may take this long to prepare, over
/// each child's own limit. A stalled network share would otherwise hold a
/// ten photo album for minutes.
pub const PREP_BUDGET: Duration = Duration::from_secs(180);

/// Why a send could not be prepared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareError {
    /// A video's duration or size could not be read.
    Video,
    /// A file picked as a round video is not one (not square, too long).
    VideoNote,
    /// Preparing took longer than [`PREP_BUDGET`].
    TimedOut,
    /// A file cannot be sent as picked.
    Unreadable,
}

/// A round video's probe and the first frame made for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedNote {
    pub probe: VideoNoteProbe,
    pub thumbnail: Option<VideoNoteThumbnail>,
}

/// Duration and size of a GIF; zeros when the file could not be read (TDLib
/// reads the real values from the file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimationSize {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
}

/// What was read from one file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreparedFile {
    /// The path a photo goes out with: itself, or the scaled copy.
    pub send_path: Option<String>,
    /// A video sent as a video (alone, in an album or as paid media).
    pub video: Option<VideoProbe>,
    pub note: Option<PreparedNote>,
    /// A video sent alone as a GIF.
    pub animation: Option<AnimationSize>,
}

/// The prepared files of one snapshot, by the path the user picked.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreparedMedia {
    files: HashMap<PathBuf, PreparedFile>,
}

/// The path a file goes out with. A photo is scaled to the limit of the
/// "High Quality" choice first (`photo_for_send`: blocks on ffmpeg), anything
/// else goes as picked. `None` when the picked path is not one to send.
pub fn send_path_for(attachment: &ComposerAttachment, media: MediaSendOptions) -> Option<String> {
    let path = attachment.send_path_str()?;
    if attachment.kind != AttachmentKind::Photo {
        return Some(path);
    }
    let prepared = super::photo_for_send(&attachment.path, media.hd_photos);
    if prepared == attachment.path {
        Some(path)
    } else {
        Some(prepared.to_string_lossy().into_owned())
    }
}

impl PreparedMedia {
    /// Read the files of `snapshot`. Blocks (ffprobe, ffmpeg): run it off
    /// the UI thread.
    pub fn prepare(snapshot: &ComposerSnapshot) -> Result<Self, PrepareError> {
        let started = Instant::now();
        let alone = snapshot.album.is_empty();
        let mut files = HashMap::new();
        for attachment in snapshot.attachment.iter().chain(snapshot.album.iter()) {
            if started.elapsed() > PREP_BUDGET {
                return Err(PrepareError::TimedOut);
            }
            if files.contains_key(&attachment.path) {
                continue;
            }
            let file = prepare_file(attachment, snapshot.media, alone)?;
            files.insert(attachment.path.clone(), file);
        }
        Ok(Self { files })
    }

    /// `self` with `file` read from `path` (tests and callers that read a
    /// file some other way).
    pub fn with_file(mut self, path: impl Into<PathBuf>, file: PreparedFile) -> Self {
        self.files.insert(path.into(), file);
        self
    }

    pub fn file(&self, path: &Path) -> Option<&PreparedFile> {
        self.files.get(path)
    }
}

fn prepare_file(
    attachment: &ComposerAttachment,
    media: MediaSendOptions,
    alone: bool,
) -> Result<PreparedFile, PrepareError> {
    let mut file = PreparedFile::default();
    match attachment.kind {
        AttachmentKind::Photo => {
            file.send_path =
                Some(send_path_for(attachment, media).ok_or(PrepareError::Unreadable)?);
        }
        AttachmentKind::Video if alone && attachment.extras.gif => {
            file.animation = Some(
                crate::video::probe_with_ffprobe(&attachment.path, false)
                    .map(|probe| AnimationSize {
                        duration: probe.duration,
                        width: probe.width,
                        height: probe.height,
                    })
                    .unwrap_or_default(),
            );
        }
        AttachmentKind::Video => {
            file.video = Some(
                crate::video::probe_local_video(&attachment.path)
                    .map_err(|_| PrepareError::Video)?,
            );
        }
        AttachmentKind::VideoNote => {
            let probe = crate::video::probe_local_video_note(&attachment.path)
                .map_err(|_| PrepareError::VideoNote)?;
            file.note = Some(PreparedNote {
                probe,
                thumbnail: crate::video::write_video_note_thumbnail(&attachment.path),
            });
        }
        AttachmentKind::Document => {}
    }
    Ok(file)
}

impl ComposerSnapshot {
    /// Some file needs ffprobe or ffmpeg before it can be sent.
    pub fn needs_media_prep(&self) -> bool {
        self.attachment
            .iter()
            .chain(self.album.iter())
            .any(|attachment| {
                matches!(
                    attachment.kind,
                    AttachmentKind::Photo | AttachmentKind::Video | AttachmentKind::VideoNote
                )
            })
    }

    /// Attach the files read in the background.
    pub fn with_prepared(mut self, prepared: PreparedMedia) -> Self {
        self.prepared = Some(std::sync::Arc::new(prepared));
        self
    }

    /// What was read from `path` ahead of the send, if it was.
    pub fn prepared_file(&self, path: &Path) -> Option<&PreparedFile> {
        self.prepared.as_deref()?.file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ChatId, ViewGeneration};

    fn attachment(name: &str, kind: AttachmentKind) -> ComposerAttachment {
        ComposerAttachment {
            path: PathBuf::from(name),
            kind,
            file_name: name.into(),
            spoiler: false,
            extras: Default::default(),
        }
    }

    fn snapshot(attachment: ComposerAttachment) -> ComposerSnapshot {
        ComposerSnapshot::capture_with_attachment(
            ChatId(1),
            ViewGeneration(0),
            "",
            Some(attachment),
        )
    }

    #[test]
    fn only_photos_videos_and_round_videos_need_preparing() {
        for (kind, needs) in [
            (AttachmentKind::Photo, true),
            (AttachmentKind::Video, true),
            (AttachmentKind::VideoNote, true),
            (AttachmentKind::Document, false),
        ] {
            assert_eq!(snapshot(attachment("a", kind)).needs_media_prep(), needs);
        }
        assert!(!ComposerSnapshot::capture(ChatId(1), ViewGeneration(0), "hi").needs_media_prep());
    }

    #[test]
    fn an_unreadable_video_fails_the_prepare_quickly() {
        let snap = snapshot(attachment("/no/such/dir/clip.mp4", AttachmentKind::Video));
        assert_eq!(PreparedMedia::prepare(&snap), Err(PrepareError::Video));
    }

    #[test]
    fn an_unreadable_round_video_fails_the_prepare_quickly() {
        let snap = snapshot(attachment(
            "/no/such/dir/note.mp4",
            AttachmentKind::VideoNote,
        ));
        assert_eq!(PreparedMedia::prepare(&snap), Err(PrepareError::VideoNote));
    }

    #[test]
    fn documents_prepare_to_nothing() {
        let snap = snapshot(attachment("/x/a.txt", AttachmentKind::Document));
        let prepared = PreparedMedia::prepare(&snap).unwrap();
        assert_eq!(
            prepared.file(Path::new("/x/a.txt")),
            Some(&PreparedFile::default())
        );
    }
}
