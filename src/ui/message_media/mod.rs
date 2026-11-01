//! message media attachments: photo/video/voice/sticker/location/dice rendering.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::progress::ProgressCircle;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::media_viewer::MediaViewerItem;
use quill::state::Session;
use quill::story_viewer::StoryViewerItem;
use quill::telegram::envelope::{ParsedFile, SpeechRecognition};
use quill::voice::{self, format_voice_duration};
use std::path::PathBuf;
use std::sync::Arc;

mod action_card;
mod audio;
mod call;
mod cards;
mod dice;
mod document;
mod document_thumbed;
mod fixtures;
mod layout;
mod location;
mod paid_media;
mod picture_file;
mod scrubber;
mod sticker;
mod story_card;
mod surfaces;
mod video_note;
mod visual;

pub(super) use action_card::{action_card, is_bubble_card};
pub(super) use audio::{audio_row, transcription_row, voice_note_row, waveform_row};
pub(super) use call::call_card;
pub(super) use cards::{ContactCardState, contact_row};
pub(super) use dice::dice_row;
pub(super) use document::{
    DocOrigin, action_disc, bubble_accent, document_chip, download_display_name, format_bytes,
    inline_link,
};
pub(super) use layout::{
    MediaCorners, MediaDisc, MediaFrameKind, bubble_outer_width, download_fraction,
    leads_with_media, media_content_width, media_disc, media_frame_for, single_media_frame,
    single_media_width,
};
pub(super) use location::{location_row, venue_row};
pub(super) use picture_file::picture_file_attachment;
pub(super) use scrubber::{AudioScrubber, HoverHandler, ScrubberFace};
pub(super) use sticker::{sticker_attachment, sticker_box};
pub(super) use story_card::{expired_mention_text, story_mention_extras};
pub(super) use surfaces::{fading_spoiler, spoiler_cover, spoiler_surface, video_badge};
pub(super) use video_note::{VIDEO_NOTE_DIAMETER, video_note_attachment};
pub(super) use visual::{animation_attachment, photo_attachment, video_attachment};

use layout::MEDIA_VISUAL_GROUP;
use surfaces::{
    gif_badge, inline_surface, live_tile, note_duration_badge, round_inline_surface, unseen_ring,
};

pub(super) fn photo_display_path(
    photo: &quill::telegram::envelope::PhotoContent,
    files: &quill::state::FileMap,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    // Sharpest downloaded size first: the bubble frame is up to 360pt
    // wide (720px on Retina), well past the ≤320px "m" thumbnail.
    let mut ids = Vec::new();
    if let Some(size) = photo.largest_size() {
        ids.push(size.file_id);
    }
    if let Some(size) = photo.thumb_size() {
        ids.push(size.file_id);
    }
    for id in ids {
        if let Some(path) = files.get(&id.0).and_then(|f| f.usable_path())
            && let Some(safe) = sandboxed_display_path(path, roots)
        {
            return Some(safe);
        }
    }
    for size in &photo.sizes {
        if let Some(path) = files.get(&size.file_id.0).and_then(|f| f.usable_path())
            && let Some(safe) = sandboxed_display_path(path, roots)
        {
            return Some(safe);
        }
    }
    None
}

/// Phase 4.5: resolve the current viewer item's visual — first local
/// display candidate inside the media allowlist roots, same sandbox rule as
/// history rows (`sandboxed_display_path`).
pub(super) fn viewer_display_path(
    item: &MediaViewerItem,
    files: &quill::state::FileMap,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    item.display_file_ids.iter().find_map(|id| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, roots))
    })
}

pub(super) fn story_viewer_display_path(
    item: &StoryViewerItem,
    files: &quill::state::FileMap,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    item.display_file_ids.iter().find_map(|id| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, roots))
    })
}

pub(super) fn file_is_downloading(
    file_id: FileId,
    files: &quill::state::FileMap,
    downloads: &quill::state::DownloadIntents,
) -> bool {
    downloads.is_downloading(file_id.0, files)
}
