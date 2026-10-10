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
            match (as_files, media_kind_for(&attachment.path)) {
                (true, Some(_)) => {
                    attachment.kind = AttachmentKind::Document;
                    attachment.spoiler = false;
                }
                (false, Some(kind)) if attachment.kind == AttachmentKind::Document => {
                    attachment.kind = kind;
                }
                _ => {}
            }
        }
    }

    /// Whether the tray can switch between files and compressed media:
    /// some attachment is a photo or video file.
    pub fn can_send_as_files(list: &[Self]) -> bool {
        list.iter()
            .any(|attachment| media_kind_for(&attachment.path).is_some())
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
        if paths.len() > crate::album::ALBUM_MAX_ITEMS.saturating_sub(list.len()) {
            return Err("Attach at most 10 files at a time.");
        }
        let media: Option<Vec<AttachmentKind>> = paths.iter().map(|p| media_kind_for(p)).collect();
        let media = media.filter(|_| {
            list.iter()
                .all(|att| matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video))
        });
        let planned: Vec<(PathBuf, AttachmentKind)> = paths
            .iter()
            .enumerate()
            .map(|(ix, path)| {
                let kind = media
                    .as_ref()
                    .map_or(AttachmentKind::Document, |kinds| kinds[ix]);
                (path.clone(), kind)
            })
            .collect();
        Self::append_planned(list, &planned)
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
        if planned.len() > crate::album::ALBUM_MAX_ITEMS.saturating_sub(list.len()) {
            return Err("Attach at most 10 files at a time.");
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
        })
    }

    /// Add a picked file. A document replaces the list (documents are not mixed
    /// into a photo/video album). Photos and videos accumulate up to 10.
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
                if list.len() >= crate::album::ALBUM_MAX_ITEMS {
                    return;
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
    let path = std::env::temp_dir().join(format!(
        "quill-paste-{stamp}-{}.{}",
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
    /// files (timestamp + pid in the name).
    #[test]
    fn clipboard_image_attachment_names_are_unique() {
        let bytes: &[u8] = &[0x89, b'P', b'N', b'G'];
        let first = clipboard_image_attachment(bytes, "png").expect("first");
        let second = clipboard_image_attachment(bytes, "png").expect("second");
        assert_ne!(first.path, second.path);
        fs::remove_file(&first.path).ok();
        fs::remove_file(&second.path).ok();
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
        assert!(
            ComposerAttachment::append_dropped_files(&mut list, &vec![file.clone(); 10]).is_err()
        );
        assert_eq!(list, before);
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
