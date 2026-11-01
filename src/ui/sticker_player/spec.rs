//! What a view plays (file, format, skin tone, tint, mirroring) and the
//! identity of its play head.
#![deny(clippy::indexing_slicing)]

use quill::ids::FileId;
use quill::sticker_playback::WebmStep;
use quill::sticker_playback::player::{ClipKey, Origin};
use quill::sticker_playback::{FitzType, MAX_STICKER_FRAMES, Tint};
use quill::telegram::envelope::StickerFormat;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// The part of a window a view shows in, which says when its head is
/// forgotten and its clip closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::ui) enum Part {
    /// The window's open chat: its history, its header and profile, and
    /// the effects over them. They go when the window lets go of the chat,
    /// so the chat plays from the start when it shows again (tdesktop
    /// clears the played set when the chat is left).
    Chat,
    /// The rest of the window: the chat list, the composer, panels and
    /// popups. Opening another chat leaves them as they are (tdesktop's
    /// chat list rows keep their emoji); their clips close through the
    /// closer once nothing shows them.
    Chrome,
}

/// One view's play head: views with the same id share a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::ui) struct HeadId {
    id: u64,
    part: Part,
}

impl HeadId {
    /// The head in `part` named by `name` (a place and what identifies the
    /// view there, such as `("history", chat, message)`).
    pub(in crate::ui) fn of(part: Part, name: impl Hash) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut hasher);
        Self {
            id: hasher.finish(),
            part,
        }
    }

    pub(super) fn part(self) -> Part {
        self.part
    }
}

/// A local animated sticker file and how to draw it.
#[derive(Clone, Debug)]
pub(in crate::ui) struct ClipSpec {
    pub(in crate::ui) file: FileId,
    pub(in crate::ui) format: StickerFormat,
    pub(in crate::ui) path: PathBuf,
    pub(in crate::ui) fitz: FitzType,
    pub(in crate::ui) tint: Option<Tint>,
    pub(in crate::ui) mirror: bool,
    /// The display rate its frame grid is made for (`ClipKey::hz`).
    pub(in crate::ui) hz: u32,
}

impl ClipSpec {
    pub(in crate::ui) fn key(&self, edge: u32) -> ClipKey {
        ClipKey {
            file: self.file.0,
            edge,
            fitz: self.fitz,
            tint: self.tint,
            mirror: self.mirror,
            hz: self.hz,
        }
    }

    /// Whether `key` is this file drawn this way, at any size.
    pub(in crate::ui) fn same_file(&self, key: &ClipKey) -> bool {
        key.file == self.file.0
            && key.fitz == self.fitz
            && key.tint == self.tint
            && key.mirror == self.mirror
    }

    /// Where the engine gets the frames of this file at `edge` px.
    pub(in crate::ui) fn origin(&self, edge: u32) -> Origin {
        match self.format {
            StickerFormat::Webm => {
                let path = self.path.clone();
                let file = self.file.0;
                Origin::Decoded(Box::new(move |cancel, frames| {
                    let nonce = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_nanos());
                    let dir = quill::animation::gif_frame_cache_dir(file)
                        .join(format!("sticker-{edge}-{nonce}"));
                    let fps = quill::sticker_playback::stream_webm_sized(
                        &path,
                        &dir,
                        edge as usize,
                        MAX_STICKER_FRAMES,
                        &cancel.child,
                        &cancel.flag,
                        &mut |step| match step {
                            WebmStep::Frame(frame) => frames.frame(&mut frame.into_square(edge)),
                            WebmStep::Repeat(times) => frames.repeat(times),
                        },
                    );
                    let _ = std::fs::remove_dir_all(&dir);
                    fps
                }))
            }
            _ => Origin::Lottie(self.path.clone()),
        }
    }
}
