//! Drop zones for files dragged onto a chat, after Telegram Desktop's
//! `DragArea` (history/history_drag_area.cpp) and `ComputeMimeDataState`
//! (storage/storage_media_prepare.cpp).
//!
//! While files hover over the chat tdesktop shows one or two drop zones
//! whose labels depend on what is being dragged: a lone document has one
//! zone; images, mixed media, several files or a folder get a top and a
//! bottom zone to choose between sending quickly (compressed media),
//! sending as documents, and sending a folder or several files as one
//! archive. This module is the pure part: classifying the dragged paths,
//! choosing the zones and their wording, and resolving the zone that
//! received the drop into what to attach. No GPUI and no I/O except
//! [`DragEntry::probe`] and [`folder_files_for_sending`];
//! `src/ui/drop_zones.rs` draws the zones.

use std::path::{Path, PathBuf};

use crate::composer::{AttachmentKind, media_kind_for};

/// What is being dragged (tdesktop `Storage::MimeDataState`, minus the
/// states only edit boxes use).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragState {
    /// One document, or several that cannot be archived separately.
    Files,
    /// Several documents: sending them as one archive is offered.
    FilesArchive,
    /// Only photos.
    PhotoFiles,
    /// Photos and videos.
    MediaFiles,
    /// A single folder.
    Folder,
}

/// One dragged path, reduced to what classification needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragEntry {
    pub is_dir: bool,
    /// How the file is sent when it goes as media; `None` for documents.
    pub media: Option<AttachmentKind>,
}

impl DragEntry {
    /// Look at the file system once for a dragged path.
    pub fn probe(path: &Path) -> Self {
        Self {
            is_dir: path.is_dir(),
            media: media_kind_for(path),
        }
    }
}

/// Classify a drag. `None` means nothing here can be sent: no paths, or
/// several paths with a folder among them (tdesktop refuses those too).
pub fn classify(entries: &[DragEntry]) -> Option<DragState> {
    if entries.is_empty() {
        return None;
    }
    if entries.iter().any(|entry| entry.is_dir) {
        return (entries.len() == 1).then_some(DragState::Folder);
    }
    let all_photos = entries
        .iter()
        .all(|entry| entry.media == Some(AttachmentKind::Photo));
    let all_media = entries.iter().all(|entry| entry.media.is_some());
    Some(if all_photos {
        DragState::PhotoFiles
    } else if all_media {
        DragState::MediaFiles
    } else if entries.len() > 1 {
        DragState::FilesArchive
    } else {
        DragState::Files
    })
}

/// What a zone does with the dropped paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropAction {
    /// Every file goes as a document, uncompressed.
    AsDocuments,
    /// Photos and videos go as compressed media (an album).
    AsMedia,
    /// A folder's files go separately (its top level and one level of
    /// subfolders, as tdesktop's `FolderFilesForSending`).
    FolderFiles,
    /// One zip archive of the folder or the files.
    Archive,
}

/// One drop zone: a title and the line under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zone {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub action: DropAction,
}

/// The zones for a drag: the top one, and a bottom one when the drag
/// offers a second way to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zones {
    pub top: Zone,
    pub bottom: Option<Zone>,
}

/// The wording and actions of tdesktop's `lng_drag_*` strings.
pub fn zones(state: DragState) -> Zones {
    const fn zone(title: &'static str, subtitle: &'static str, action: DropAction) -> Zone {
        Zone {
            title,
            subtitle,
            action,
        }
    }
    let documents = zone(
        "Drop files here",
        "to send them as documents",
        DropAction::AsDocuments,
    );
    match state {
        DragState::Files => Zones {
            top: documents,
            bottom: None,
        },
        DragState::FilesArchive => Zones {
            top: documents,
            bottom: Some(zone(
                "Drop files here",
                "to send them as one archive",
                DropAction::Archive,
            )),
        },
        DragState::PhotoFiles => Zones {
            top: zone(
                "Drop images here",
                "to send them without compression",
                DropAction::AsDocuments,
            ),
            bottom: Some(zone(
                "Drop photos here",
                "to send them in a quick way",
                DropAction::AsMedia,
            )),
        },
        DragState::MediaFiles => Zones {
            top: documents,
            bottom: Some(zone(
                "Drop photos and videos",
                "to send them as media files",
                DropAction::AsMedia,
            )),
        },
        DragState::Folder => Zones {
            top: zone(
                "Drop folder here",
                "to send all its files",
                DropAction::FolderFiles,
            ),
            bottom: Some(zone(
                "Drop folder here",
                "to send it as an archive",
                DropAction::Archive,
            )),
        },
    }
}

/// How the dropped files are attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropPlan {
    /// Attach these paths, each as the given kind.
    Attach(Vec<(PathBuf, AttachmentKind)>),
    /// Build a zip of these sources, then attach it as a document.
    Archive(ArchiveSource),
    /// Nothing can be sent from this drop; the note says why.
    Reject(&'static str),
}

/// What an archive is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveSource {
    /// A folder and everything in it, under a root of the same name.
    Folder(PathBuf),
    /// Loose files, at the top level of the archive.
    Files(Vec<PathBuf>),
}

/// Resolve a drop on a zone. `paths` are the dropped paths and `folder_files`
/// the result of [`folder_files_for_sending`] when the drop was a folder
/// sent as separate files (the caller reads the folder, this stays pure).
pub fn plan_drop(action: DropAction, paths: &[PathBuf], folder_files: &[PathBuf]) -> DropPlan {
    match action {
        DropAction::AsDocuments => attach_all(paths, |_| AttachmentKind::Document),
        DropAction::AsMedia => {
            if paths.iter().all(|path| media_kind_for(path).is_some()) {
                attach_all(paths, |path| {
                    media_kind_for(path).unwrap_or(AttachmentKind::Document)
                })
            } else {
                DropPlan::Reject("Only photos and videos can be sent this way.")
            }
        }
        DropAction::FolderFiles => {
            if folder_files.is_empty() {
                DropPlan::Reject("That folder has no files to send.")
            } else {
                // Like a plain drop: all-media goes as an album, anything else
                // as documents (an album cannot mix them).
                let all_media = folder_files
                    .iter()
                    .all(|path| media_kind_for(path).is_some());
                attach_all(folder_files, |path| {
                    if all_media {
                        media_kind_for(path).unwrap_or(AttachmentKind::Document)
                    } else {
                        AttachmentKind::Document
                    }
                })
            }
        }
        DropAction::Archive => match paths {
            [only] if only.is_dir() => DropPlan::Archive(ArchiveSource::Folder(only.clone())),
            files if !files.is_empty() && files.iter().all(|path| !path.is_dir()) => {
                DropPlan::Archive(ArchiveSource::Files(files.to_vec()))
            }
            _ => DropPlan::Reject("Drop one folder, or files, to send an archive."),
        },
    }
}

fn attach_all(paths: &[PathBuf], kind: impl Fn(&Path) -> AttachmentKind) -> DropPlan {
    if paths.is_empty() {
        return DropPlan::Reject("Nothing to attach.");
    }
    DropPlan::Attach(
        paths
            .iter()
            .map(|path| (path.clone(), kind(path)))
            .collect(),
    )
}

/// tdesktop `FolderFilesForSending`: the files directly in `folder`, then the
/// files directly in each subfolder, each group sorted by name. Hidden files
/// are included; symbolic links to directories are not followed.
pub fn folder_files_for_sending(folder: &Path) -> Vec<PathBuf> {
    fn files_in(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        let Ok(read) = std::fs::read_dir(dir) else {
            return (files, dirs);
        };
        for entry in read.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_file() {
                files.push(entry.path());
            } else if kind.is_dir() {
                dirs.push(entry.path());
            }
        }
        files.sort();
        dirs.sort();
        (files, dirs)
    }
    let (mut result, subdirs) = files_in(folder);
    for subdir in subdirs {
        result.extend(files_in(&subdir).0);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo() -> DragEntry {
        DragEntry {
            is_dir: false,
            media: Some(AttachmentKind::Photo),
        }
    }

    fn video() -> DragEntry {
        DragEntry {
            is_dir: false,
            media: Some(AttachmentKind::Video),
        }
    }

    fn doc() -> DragEntry {
        DragEntry {
            is_dir: false,
            media: None,
        }
    }

    fn dir() -> DragEntry {
        DragEntry {
            is_dir: true,
            media: None,
        }
    }

    #[test]
    fn classification_follows_the_drag_state_table() {
        assert_eq!(classify(&[]), None);
        assert_eq!(classify(&[doc()]), Some(DragState::Files));
        assert_eq!(classify(&[doc(), doc()]), Some(DragState::FilesArchive));
        assert_eq!(classify(&[doc(), photo()]), Some(DragState::FilesArchive));
        assert_eq!(classify(&[photo()]), Some(DragState::PhotoFiles));
        assert_eq!(classify(&[photo(), photo()]), Some(DragState::PhotoFiles));
        assert_eq!(classify(&[photo(), video()]), Some(DragState::MediaFiles));
        assert_eq!(classify(&[video()]), Some(DragState::MediaFiles));
        assert_eq!(classify(&[dir()]), Some(DragState::Folder));
        // A folder next to anything else cannot be sent.
        assert_eq!(classify(&[dir(), doc()]), None);
        assert_eq!(classify(&[dir(), dir()]), None);
    }

    #[test]
    fn zones_have_the_tdesktop_wording() {
        let files = zones(DragState::Files);
        assert_eq!(files.top.title, "Drop files here");
        assert_eq!(files.top.subtitle, "to send them as documents");
        assert!(files.bottom.is_none());

        let photos = zones(DragState::PhotoFiles);
        assert_eq!(photos.top.subtitle, "to send them without compression");
        assert_eq!(photos.top.action, DropAction::AsDocuments);
        let quick = photos.bottom.unwrap();
        assert_eq!(quick.title, "Drop photos here");
        assert_eq!(quick.subtitle, "to send them in a quick way");
        assert_eq!(quick.action, DropAction::AsMedia);

        let media = zones(DragState::MediaFiles).bottom.unwrap();
        assert_eq!(media.title, "Drop photos and videos");
        assert_eq!(media.action, DropAction::AsMedia);

        let several = zones(DragState::FilesArchive).bottom.unwrap();
        assert_eq!(several.subtitle, "to send them as one archive");
        assert_eq!(several.action, DropAction::Archive);

        let folder = zones(DragState::Folder);
        assert_eq!(folder.top.subtitle, "to send all its files");
        assert_eq!(folder.top.action, DropAction::FolderFiles);
        assert_eq!(folder.bottom.unwrap().action, DropAction::Archive);
    }

    #[test]
    fn documents_zone_sends_images_uncompressed() {
        let paths = vec![PathBuf::from("/a/one.png"), PathBuf::from("/a/two.mp4")];
        assert_eq!(
            plan_drop(DropAction::AsDocuments, &paths, &[]),
            DropPlan::Attach(vec![
                (PathBuf::from("/a/one.png"), AttachmentKind::Document),
                (PathBuf::from("/a/two.mp4"), AttachmentKind::Document),
            ])
        );
    }

    #[test]
    fn quick_zone_sends_media_and_refuses_documents() {
        let paths = vec![PathBuf::from("/a/one.PNG"), PathBuf::from("/a/two.mp4")];
        assert_eq!(
            plan_drop(DropAction::AsMedia, &paths, &[]),
            DropPlan::Attach(vec![
                (PathBuf::from("/a/one.PNG"), AttachmentKind::Photo),
                (PathBuf::from("/a/two.mp4"), AttachmentKind::Video),
            ])
        );
        let mixed = vec![PathBuf::from("/a/one.png"), PathBuf::from("/a/notes.txt")];
        assert!(matches!(
            plan_drop(DropAction::AsMedia, &mixed, &[]),
            DropPlan::Reject(_)
        ));
    }

    #[test]
    fn folder_files_zone_attaches_what_the_caller_listed() {
        let listed = vec![PathBuf::from("/f/a.png"), PathBuf::from("/f/b.png")];
        assert_eq!(
            plan_drop(DropAction::FolderFiles, &[PathBuf::from("/f")], &listed),
            DropPlan::Attach(vec![
                (PathBuf::from("/f/a.png"), AttachmentKind::Photo),
                (PathBuf::from("/f/b.png"), AttachmentKind::Photo),
            ])
        );
        let mixed = vec![PathBuf::from("/f/a.png"), PathBuf::from("/f/b.txt")];
        let DropPlan::Attach(attached) =
            plan_drop(DropAction::FolderFiles, &[PathBuf::from("/f")], &mixed)
        else {
            panic!("expected an attach plan");
        };
        assert!(
            attached
                .iter()
                .all(|(_, kind)| *kind == AttachmentKind::Document)
        );
        assert!(matches!(
            plan_drop(DropAction::FolderFiles, &[PathBuf::from("/f")], &[]),
            DropPlan::Reject(_)
        ));
    }

    #[test]
    fn archive_zone_builds_from_a_folder_or_loose_files() {
        let root = tempfile_dir("archive-plan");
        let file = root.join("note.txt");
        std::fs::write(&file, b"hi").unwrap();
        assert_eq!(
            plan_drop(DropAction::Archive, std::slice::from_ref(&root), &[]),
            DropPlan::Archive(ArchiveSource::Folder(root.clone()))
        );
        assert_eq!(
            plan_drop(DropAction::Archive, std::slice::from_ref(&file), &[]),
            DropPlan::Archive(ArchiveSource::Files(vec![file.clone()]))
        );
        assert!(matches!(
            plan_drop(DropAction::Archive, &[root.clone(), file], &[]),
            DropPlan::Reject(_)
        ));
        assert!(matches!(
            plan_drop(DropAction::Archive, &[], &[]),
            DropPlan::Reject(_)
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn folder_listing_is_sorted_and_one_level_deep() {
        let root = tempfile_dir("folder-listing");
        std::fs::write(root.join("b.txt"), b"b").unwrap();
        std::fs::write(root.join("a.txt"), b"a").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("c.txt"), b"c").unwrap();
        std::fs::create_dir(root.join("sub").join("deep")).unwrap();
        std::fs::write(root.join("sub").join("deep").join("d.txt"), b"d").unwrap();
        let names: Vec<String> = folder_files_for_sending(&root)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.txt", "b.txt", "c.txt"]);
        let _ = std::fs::remove_dir_all(root);
    }

    fn tempfile_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quill-drop-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
