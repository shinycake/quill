//! Videos picked for an owned set: a WEBM sticker adapted into an emoji
//! ("Adapt from a Sticker", tdesktop `AdaptStickerToEmoji`) and a saved GIF
//! chosen in the existing-sticker chooser (`handleChosenGifs` →
//! `AdaptGifToSet`). Both open the video editor on the file, which encodes
//! the sticker and hands it to the "New Sticker" box.
use super::state::{PendingVideo, PickExisting};
use super::*;
use quill::ids::FileId;
use quill::telegram::envelope::AnimationItem;
use std::path::PathBuf;

impl QuillApp {
    /// A downloaded file's path, when the sandbox lets the UI show it.
    pub(super) fn local_media_path(&self, file_id: FileId) -> Option<String> {
        let roots = self.media_display_roots();
        let path = self
            .session()?
            .media
            .files
            .get(&file_id.0)?
            .usable_path()
            .map(str::to_string)?;
        quill::local_path::sandboxed_display_path(&path, &roots)
            .map(|path| path.to_string_lossy().into_owned())
    }

    /// The saved GIFs the chooser offers next to the stickers (tdesktop's
    /// panel has a GIFs tab there); none while adapting a sticker, since
    /// that panel is stickers only.
    pub(super) fn pick_gif_candidates(&self, pick: PickExisting) -> Vec<AnimationItem> {
        if pick.adapt {
            return Vec::new();
        }
        self.session()
            .map(|session| session.stickers.gifs.animations.clone())
            .unwrap_or_default()
    }

    /// The still the chooser shows for a GIF.
    pub(super) fn gif_still(&self, animation: &AnimationItem) -> Option<PathBuf> {
        animation
            .thumb_file_id
            .filter(|id| id.0 != 0)
            .and_then(|id| self.local_media_path(id))
            .map(PathBuf::from)
    }

    /// A GIF was chosen in the chooser: into the video editor for the set.
    pub(super) fn pick_gif_for_set(
        &mut self,
        animation: &AnimationItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pick) = self.sticker_box.pick else {
            return;
        };
        self.pick_video_for_set(pick, animation.file_id, None, window, cx);
    }

    /// Open the video editor on the chosen file for the set, with `emoji`
    /// already picked (a sticker's own). A file not downloaded yet is
    /// fetched and the editor opens when it arrives
    /// (`sticker_box_frame_upkeep`); the chooser closes either way, as
    /// tdesktop's panel hides on the pick.
    pub(super) fn pick_video_for_set(
        &mut self,
        pick: PickExisting,
        file_id: FileId,
        emoji: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sticker_box.pick = None;
        self.sticker_box.pending_still = None;
        let Some(path) = self.local_media_path(file_id) else {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.download_file(file_id, 16);
            }
            self.sticker_box.pending_video = Some(PendingVideo {
                pick,
                file_id,
                emoji,
            });
            cx.notify();
            return;
        };
        self.open_set_video_editor(
            pick.set_id,
            pick.kind,
            PathBuf::from(path),
            emoji,
            window,
            cx,
        );
    }
}
