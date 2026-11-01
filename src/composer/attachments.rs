//! Composer attachments: picked, dropped and pasted files.

use super::*;

/// How the user chose to send a local file.
/// Video is `inputMessageVideo`, not a document (tdesktop Photo/Video vs File).
/// A video note is `inputMessageVideoNote` and is not mixed into an album.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Photo,
    Document,
    Video,
    VideoNote,
}

/// A local file the user explicitly attached. Path is canonical at pick time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerAttachment {
    pub path: PathBuf,
    pub kind: AttachmentKind,
    pub file_name: String,
    /// Send hidden behind a spoiler (`has_spoiler`; photos and videos).
    pub spoiler: bool,
    /// Cover, GIF and video-editor state (`media_options.rs`).
    pub extras: AttachmentExtras,
}

/// Whether the video file has no sound track (ffprobe). Blocks; a file it
/// can't read counts as having sound, so it stays a video.
pub fn without_sound_track(path: &Path) -> bool {
    crate::video_edit::probe_source(path).is_some_and(|source| !source.has_audio)
}

/// The media kind a file is sent as when dropped or pasted, by extension:
/// formats Telegram sends as photos or videos. Anything else is a file.
pub fn media_kind_for(path: &Path) -> Option<AttachmentKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" => Some(AttachmentKind::Photo),
        "mp4" | "mov" | "m4v" => Some(AttachmentKind::Video),
        _ => None,
    }
}

impl ComposerAttachment {
    /// Telegram Desktop's "Send without compression": every attachment
    /// goes as a file (`as_files`), or back to photo/video where the file
    /// is one. All at once, since an album can't mix files and media.
    pub fn set_send_as_files(list: &mut [Self], as_files: bool) {
        for attachment in list {
            let media = media_kind_for(&attachment.path);
            let gif = is_gif_path(&attachment.path);
            match (as_files, media) {
                (true, Some(_)) => {
                    attachment.kind = AttachmentKind::Document;
                    attachment.spoiler = false;
                    attachment.extras.gif = false;
                }
                (false, Some(kind)) if attachment.kind == AttachmentKind::Document => {
                    attachment.kind = kind;
                }
                (true, None) if gif && attachment.extras.gif => {
                    attachment.kind = AttachmentKind::Document;
                    attachment.extras.gif = false;
                    attachment.spoiler = false;
                }
                (false, None) if gif && attachment.kind == AttachmentKind::Document => {
                    attachment.kind = AttachmentKind::Video;
                    attachment.extras.gif = true;
                }
                _ => {}
            }
        }
    }

    /// Whether the tray can switch between files and compressed media:
    /// some attachment is a photo, video or GIF file.
    pub fn can_send_as_files(list: &[Self]) -> bool {
        list.iter().any(|attachment| {
            media_kind_for(&attachment.path).is_some() || is_gif_path(&attachment.path)
        })
    }

    /// tdesktop `modifications.gif = !hasAudio`: an `.mp4` video may be a
    /// GIF when it has no sound track. This says whether the file is worth
    /// checking with [`without_sound_track`]; the check runs ffprobe, so the
    /// UI does it in the background and then sets `extras.gif`.
    pub fn needs_sound_check(&self) -> bool {
        self.kind == AttachmentKind::Video
            && !self.extras.gif
            && self
                .path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
    }

    /// Whether this attachment goes out as a GIF animation when the tray is
    /// sent: it is a GIF and goes alone (one file, separate messages, or a
    /// video whose sound was taken out, which leaves the album).
    pub fn sends_as_animation(&self, tray_len: usize, grouped: bool) -> bool {
        self.extras.gif && (tray_len == 1 || !grouped || self.extras.sound_removed)
    }

    /// Whether the tray can go as albums: two or more photos and videos,
    /// or two or more files (tdesktop `AlbumType::PhotoVideo` and
    /// `AlbumType::File`); the two never share an album.
    pub fn albumable(list: &[Self]) -> bool {
        let media = |att: &Self| matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video);
        list.len() >= 2
            && (list.iter().all(media)
                || list.iter().all(|att| att.kind == AttachmentKind::Document))
    }

    /// The messages the tray is sent as: albums of at most
    /// [`ALBUM_MAX_ITEMS`](crate::album::ALBUM_MAX_ITEMS) in order (tdesktop
    /// `DivideByGroups`); when `grouped` is false every file is its own
    /// message, and a video turned into a GIF leaves the album and goes
    /// alone (tdesktop `GroupRanges`). Each inner list of two or more is an
    /// album.
    pub fn split_for_send(list: &[Self], grouped: bool) -> Vec<Vec<Self>> {
        if !grouped {
            return list.iter().map(|att| vec![att.clone()]).collect();
        }
        let (alone, together): (Vec<&Self>, Vec<&Self>) = list
            .iter()
            .partition(|att| att.extras.gif && att.extras.sound_removed);
        let mut groups = Vec::new();
        groups.extend(
            together
                .chunks(crate::album::ALBUM_MAX_ITEMS)
                .map(|chunk| chunk.iter().map(|att| (*att).clone()).collect()),
        );
        groups.extend(alone.into_iter().map(|att| vec![att.clone()]));
        groups
    }

    /// Append an explicit OS file drop (or pasted files), preserving the
    /// draft on failure. Like Telegram Desktop, a drop of only photos and
    /// videos becomes media (an album); anything else is sent as files,
    /// since albums can't mix documents with media.
    pub fn append_dropped_files(
        list: &mut Vec<Self>,
        paths: &[PathBuf],
    ) -> Result<usize, &'static str> {
        if paths.is_empty() {
            return Ok(0);
        }
        let media: Option<Vec<AttachmentKind>> = paths.iter().map(|p| media_kind_for(p)).collect();
        let media = media.filter(|_| {
            list.iter().all(|att| {
                matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video) && !att.extras.gif
            })
        });
        // A lone `.gif` goes out as a GIF (an animation, with a caption
        // like any media); next to other files it stays a file.
        let gif_alone =
            list.is_empty() && paths.len() == 1 && paths.first().is_some_and(|p| is_gif_path(p));
        let planned: Vec<(PathBuf, AttachmentKind)> = paths
            .iter()
            .enumerate()
            .map(|(ix, path)| {
                let kind = if gif_alone {
                    AttachmentKind::Video
                } else {
                    media
                        .as_ref()
                        .and_then(|kinds| kinds.get(ix).copied())
                        .unwrap_or(AttachmentKind::Document)
                };
                (path.clone(), kind)
            })
            .collect();
        let had_gif = list
            .iter()
            .any(|att| is_gif_path(&att.path) && att.extras.gif);
        let added = Self::append_planned(list, &planned)?;
        if gif_alone && let Some(first) = list.first_mut() {
            first.extras.gif = true;
        }
        if had_gif {
            // An animated `.gif` file is only sent alone: next to other files it is a file.
            for att in list
                .iter_mut()
                .filter(|att| is_gif_path(&att.path) && att.extras.gif)
            {
                att.kind = AttachmentKind::Document;
                att.extras.gif = false;
            }
        }
        Ok(added)
    }

    /// Append files whose kind was already chosen, for example by the drop
    /// zone they landed on. All or nothing: on failure the list is unchanged.
    pub fn append_planned(
        list: &mut Vec<Self>,
        planned: &[(PathBuf, AttachmentKind)],
    ) -> Result<usize, &'static str> {
        if planned.is_empty() {
            return Ok(0);
        }
        let picked = planned
            .iter()
            .map(|(path, kind)| Self::pick(path, *kind))
            .collect::<Option<Vec<_>>>()
            .ok_or("Could not attach files. Drop existing files, not folders.")?;

        list.extend(picked);
        Ok(planned.len())
    }

    /// Validate `candidate` as a user-picked send path. Never call with paths
    /// taken from untrusted TDLib JSON.
    pub fn pick(candidate: &Path, kind: AttachmentKind) -> Option<Self> {
        let path = pick_send_path(candidate)?;
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        Some(Self {
            path,
            kind,
            file_name,
            spoiler: false,
            extras: AttachmentExtras::default(),
        })
    }

    /// Add a picked file. A document replaces the list (documents are not mixed
    /// into a photo/video album). Photos and videos accumulate.
    pub fn push_attachment(list: &mut Vec<Self>, next: Self) {
        match next.kind {
            AttachmentKind::Document | AttachmentKind::VideoNote => {
                list.clear();
                list.push(next);
            }
            AttachmentKind::Photo | AttachmentKind::Video => {
                if list
                    .iter()
                    .any(|att| !matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video))
                {
                    list.clear();
                }
                list.push(next);
            }
        }
    }

    /// Path string safe to embed in `inputFileLocal` (matches the pick).
    pub fn send_path_str(&self) -> Option<String> {
        if !is_explicit_send_path(&self.path, &self.path) {
            return None;
        }
        Some(self.path.to_string_lossy().into_owned())
    }
}

/// Parity slice (platform-paste-image): persist clipboard image bytes as a
/// temp file and pick it as a photo attachment. The temp file lives until the
/// OS reclaims it; the send path is canonicalized at pick time like any
/// user-picked file. Returns `None` when the bytes can't be persisted.
pub fn clipboard_image_attachment(bytes: &[u8], extension: &str) -> Option<ComposerAttachment> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // A process-wide counter keeps two pastes in the same clock tick apart.
    static PASTE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = PASTE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "quill-paste-{stamp}-{seq}-{}.{}",
        std::process::id(),
        extension
    ));
    std::fs::write(&path, bytes).ok()?;
    ComposerAttachment::pick(&path, AttachmentKind::Photo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn attachment(name: &str, kind: AttachmentKind) -> ComposerAttachment {
        ComposerAttachment {
            path: PathBuf::from(format!("/tmp/{name}")),
            kind,
            file_name: name.to_string(),
            spoiler: true,
            extras: AttachmentExtras::default(),
        }
    }

    #[test]
    fn send_as_files_switches_media_both_ways_and_drops_spoilers() {
        let mut list = vec![
            attachment("a.jpg", AttachmentKind::Photo),
            attachment("b.mp4", AttachmentKind::Video),
            attachment("notes.txt", AttachmentKind::Document),
        ];
        assert!(ComposerAttachment::can_send_as_files(&list));
        ComposerAttachment::set_send_as_files(&mut list, true);
        assert!(list.iter().all(|a| a.kind == AttachmentKind::Document));
        assert!(!list[0].spoiler);
        ComposerAttachment::set_send_as_files(&mut list, false);
        assert_eq!(list[0].kind, AttachmentKind::Photo);
        assert_eq!(list[1].kind, AttachmentKind::Video);
        assert_eq!(list[2].kind, AttachmentKind::Document);
        assert!(!ComposerAttachment::can_send_as_files(&list[2..]));
    }
    /// Parity slice (platform-paste-image): pasted bytes persist to a temp
    /// file and come back as a Photo attachment with a png suffix.
    #[test]
    fn clipboard_image_attachment_persists_png_bytes() {
        // Minimal 1x1 PNG.
        let bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let att = clipboard_image_attachment(bytes, "png").expect("persisted");
        assert_eq!(att.kind, AttachmentKind::Photo);
        assert!(att.path.extension().and_then(|e| e.to_str()) == Some("png"));
        assert!(att.path.is_file());
        assert_eq!(fs::read(&att.path).expect("read back"), bytes);
        fs::remove_file(&att.path).ok();
    }

    /// Parity slice (platform-paste-image): distinct pastes get distinct temp
    /// files (timestamp + sequence + pid in the name), even in the same clock tick.
    #[test]
    fn clipboard_image_attachment_names_are_unique() {
        let bytes: &[u8] = &[0x89, b'P', b'N', b'G'];
        let first = clipboard_image_attachment(bytes, "png").expect("first");
        let second = clipboard_image_attachment(bytes, "png").expect("second");
        assert_ne!(first.path, second.path);
        fs::remove_file(&first.path).ok();
        fs::remove_file(&second.path).ok();
    }

    #[test]
    fn a_lone_gif_is_a_gif_and_loses_that_next_to_other_files() {
        let root = scratch("gif-drop");
        let (gif, photo) = (root.join("fun.gif"), root.join("a.png"));
        for path in [&gif, &photo] {
            fs::write(path, [1]).unwrap();
        }
        let mut list = Vec::new();
        ComposerAttachment::append_dropped_files(&mut list, std::slice::from_ref(&gif)).unwrap();
        assert_eq!(list[0].kind, AttachmentKind::Video);
        assert!(list[0].extras.gif);
        // As a file and back.
        assert!(ComposerAttachment::can_send_as_files(&list));
        ComposerAttachment::set_send_as_files(&mut list, true);
        assert_eq!(list[0].kind, AttachmentKind::Document);
        assert!(!list[0].extras.gif);
        ComposerAttachment::set_send_as_files(&mut list, false);
        assert_eq!(list[0].kind, AttachmentKind::Video);
        assert!(list[0].extras.gif);
        // Another file turns the GIF into a file.
        ComposerAttachment::append_dropped_files(&mut list, std::slice::from_ref(&photo)).unwrap();
        assert!(list.iter().all(|att| att.kind == AttachmentKind::Document));
        assert!(list.iter().all(|att| !att.extras.gif));
        // Dropped together with a photo, it is a file too.
        let mut both = Vec::new();
        ComposerAttachment::append_dropped_files(&mut both, &[gif, photo]).unwrap();
        assert!(both.iter().all(|att| att.kind == AttachmentKind::Document));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn gif_videos_leave_the_album_only_when_their_sound_was_taken_out() {
        let mut silent = attachment("a.mp4", AttachmentKind::Video);
        silent.extras.gif = true;
        let mut muted = attachment("b.mp4", AttachmentKind::Video);
        muted.extras.gif = true;
        muted.extras.sound_removed = true;
        let photo = attachment("c.jpg", AttachmentKind::Photo);
        let list = [silent.clone(), muted.clone(), photo.clone()];
        let groups = ComposerAttachment::split_for_send(&list, true);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 2, "a soundless video stays in the album");
        assert_eq!(groups[1], [muted.clone()]);
        // Ungrouped: one message each.
        assert_eq!(ComposerAttachment::split_for_send(&list, false).len(), 3);
        assert!(silent.sends_as_animation(1, true));
        assert!(!silent.sends_as_animation(3, true));
        assert!(silent.sends_as_animation(3, false));
        assert!(muted.sends_as_animation(3, true));
        assert!(!photo.sends_as_animation(1, true));
    }

    #[test]
    fn a_drop_of_23_media_files_goes_out_as_albums_of_10_10_and_3() {
        let root = scratch("many");
        let paths: Vec<PathBuf> = (0..23)
            .map(|ix| {
                let path = root.join(if ix % 3 == 0 {
                    format!("{ix:02}.mp4")
                } else {
                    format!("{ix:02}.jpg")
                });
                fs::write(&path, b"x").unwrap();
                path
            })
            .collect();
        let mut list = Vec::new();
        assert_eq!(
            ComposerAttachment::append_dropped_files(&mut list, &paths),
            Ok(23)
        );
        let groups = ComposerAttachment::split_for_send(&list, true);
        let sizes: Vec<usize> = groups.iter().map(Vec::len).collect();
        assert_eq!(sizes, [10, 10, 3]);
        let order: Vec<&str> = groups
            .iter()
            .flatten()
            .map(|att| att.file_name.as_str())
            .collect();
        let dropped: Vec<&str> = list.iter().map(|att| att.file_name.as_str()).collect();
        assert_eq!(order, dropped);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn twelve_files_go_as_albums_of_10_and_2_and_never_with_media() {
        let files: Vec<ComposerAttachment> = (0..12)
            .map(|ix| attachment(&format!("{ix:02}.txt"), AttachmentKind::Document))
            .collect();
        assert!(ComposerAttachment::albumable(&files));
        let sizes: Vec<usize> = ComposerAttachment::split_for_send(&files, true)
            .iter()
            .map(Vec::len)
            .collect();
        assert_eq!(sizes, [10, 2]);
        let mut mixed = files[..2].to_vec();
        mixed.push(attachment("c.jpg", AttachmentKind::Photo));
        assert!(!ComposerAttachment::albumable(&mixed));
        assert!(!ComposerAttachment::albumable(&files[..1]));
    }

    #[test]
    fn an_mp4_without_a_sound_track_is_marked_as_a_gif() {
        let root = scratch("silent-gif");
        let make = |name: &str, with_sound: bool| {
            let path = root.join(name);
            let mut args = vec![
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=64x64:rate=5",
            ];
            if with_sound {
                args.extend(["-f", "lavfi", "-i", "sine=duration=1", "-c:a", "aac"]);
            }
            args.extend(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-shortest"]);
            let mut args: Vec<String> = args.into_iter().map(String::from).collect();
            args.push(path.to_string_lossy().into_owned());
            let made =
                crate::video_encoder::h264_job("The test clip", &args, 64 * 64).is_ok_and(|job| {
                    let mut command = crate::video_encoder::quiet_command_for(&job.program);
                    command.args(&job.args);
                    crate::child_timeout::output_within(
                        &mut command,
                        crate::child_timeout::transcode_limit(1.0),
                    )
                    .is_ok_and(|out| out.status.success())
                });
            made.then_some(path)
        };
        // Skipped where ffmpeg isn't installed.
        let (Some(silent), Some(loud)) = (make("silent.mp4", false), make("loud.mp4", true)) else {
            return;
        };
        assert!(without_sound_track(&silent), "no sound track: a GIF");
        assert!(!without_sound_track(&loud), "with sound: a video");
        assert!(!without_sound_track(&root.join("missing.mp4")));
        let mut att = ComposerAttachment::pick(&silent, AttachmentKind::Video).unwrap();
        assert!(att.needs_sound_check());
        att.extras.gif = true;
        assert!(!att.needs_sound_check());
        let photo = ComposerAttachment::pick(&loud, AttachmentKind::Photo).unwrap();
        assert!(!photo.needs_sound_check());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_gif_video_stays_gif_when_more_files_are_added() {
        let root = scratch("gif-keeps");
        let (clip, photo) = (root.join("a.mp4"), root.join("b.png"));
        for path in [&clip, &photo] {
            fs::write(path, [1]).unwrap();
        }
        let mut list = Vec::new();
        ComposerAttachment::append_dropped_files(&mut list, std::slice::from_ref(&clip)).unwrap();
        list[0].extras.gif = true;
        list[0].extras.sound_removed = true;
        ComposerAttachment::append_dropped_files(&mut list, std::slice::from_ref(&photo)).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].extras.gif && list[0].extras.sound_removed);
        assert_eq!(list[0].kind, AttachmentKind::Video);
        let _ = fs::remove_dir_all(&root);
    }

    fn scratch(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "quill-composer-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn attachment_pick_and_caption_only_send() {
        let root = scratch("attach");
        let photo = root.join("shot.png");
        fs::write(&photo, [9, 9]).unwrap();
        let att = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
        assert_eq!(att.file_name, "shot.png");
        assert!(att.send_path_str().is_some());
        let empty_text = ComposerSnapshot::capture_with_attachment(
            ChatId(1),
            ViewGeneration(1),
            "   ",
            Some(att.clone()),
        );
        assert!(!empty_text.is_empty());
        assert_eq!(empty_text.caption(), "");
        assert!(
            ComposerAttachment::pick(&root.join("nope.png"), AttachmentKind::Document).is_none()
        );
        let note = root.join("round.mp4");
        fs::write(&note, [1]).unwrap();
        let video_note = ComposerAttachment::pick(&note, AttachmentKind::VideoNote).unwrap();
        let mut list = vec![att];
        ComposerAttachment::push_attachment(&mut list, video_note);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, AttachmentKind::VideoNote);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dropped_photos_and_videos_become_media_and_mixed_drops_files() {
        let root = scratch("media-drop");
        let (photo, clip, doc) = (root.join("a.JPG"), root.join("b.mov"), root.join("c.pdf"));
        for path in [&photo, &clip, &doc] {
            fs::write(path, [1]).unwrap();
        }
        let mut list = Vec::new();
        ComposerAttachment::append_dropped_files(&mut list, &[photo.clone(), clip.clone()])
            .unwrap();
        let kinds: Vec<_> = list.iter().map(|att| att.kind).collect();
        assert_eq!(kinds, [AttachmentKind::Photo, AttachmentKind::Video]);
        let mut mixed = Vec::new();
        ComposerAttachment::append_dropped_files(&mut mixed, &[photo, doc]).unwrap();
        assert!(mixed.iter().all(|att| att.kind == AttachmentKind::Document));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dropped_files_append_atomically_and_keep_every_file() {
        let root = scratch("file-drop");
        let photo = root.join("draft.png");
        let file = root.join("notes.txt");
        fs::write(&photo, [1]).unwrap();
        fs::write(&file, b"notes").unwrap();
        let mut list = vec![ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap()];
        assert_eq!(
            ComposerAttachment::append_dropped_files(&mut list, &[]),
            Ok(0)
        );
        let before = list.clone();
        for invalid in [root.clone(), root.join("missing.txt")] {
            assert!(
                ComposerAttachment::append_dropped_files(&mut list, &[file.clone(), invalid])
                    .is_err()
            );
            assert_eq!(list, before);
        }
        assert_eq!(
            ComposerAttachment::append_dropped_files(&mut list, &[file.clone(), photo.clone()]),
            Ok(2)
        );
        assert_eq!(list.len(), 3);
        assert_eq!(list[0], before[0]);
        assert!(
            list[1..]
                .iter()
                .all(|att| att.kind == AttachmentKind::Document && att.send_path_str().is_some())
        );
        assert_eq!(list[1].path, fs::canonicalize(file).unwrap());
        assert_eq!(list[2].path, fs::canonicalize(photo).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
