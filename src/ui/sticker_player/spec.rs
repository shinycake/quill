//! What a view plays (file, format, skin tone, tint, mirroring) and the
//! identity of its play head.
#![deny(clippy::indexing_slicing)]

use quill::ids::FileId;
use quill::sticker_playback::player::{ClipKey, DecodedFrames, Origin};
use quill::sticker_playback::{FitzType, MAX_STICKER_FRAMES, Tint};
use quill::telegram::envelope::StickerFormat;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// One view's play head: views with the same id share a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::ui) struct HeadId(u64);

impl HeadId {
    /// The head named by `parts` (a place and what identifies the view
    /// there, such as `("history", chat, message)`).
    pub(in crate::ui) fn of(parts: impl Hash) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        parts.hash(&mut hasher);
        Self(hasher.finish())
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
}

impl ClipSpec {
    pub(in crate::ui) fn key(&self, edge: u32) -> ClipKey {
        ClipKey {
            file: self.file.0,
            edge,
            fitz: self.fitz,
            tint: self.tint,
            mirror: self.mirror,
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
                Origin::Decoded(Box::new(move |cancel| {
                    let nonce = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_nanos());
                    let dir = quill::animation::gif_frame_cache_dir(file)
                        .join(format!("sticker-{edge}-{nonce}"));
                    let decoded = quill::sticker_playback::decode_webm_sized(
                        &path,
                        &dir,
                        edge as usize,
                        MAX_STICKER_FRAMES,
                        &cancel.child,
                        &cancel.flag,
                    );
                    let _ = std::fs::remove_dir_all(&dir);
                    let decoded = decoded?;
                    let fps = decoded.fps;
                    Ok(DecodedFrames {
                        // Already BGRA, centred on the transparent square.
                        frames: decoded.into_squares(edge),
                        fps,
                    })
                }))
            }
            _ => Origin::Lottie(self.path.clone()),
        }
    }
}
