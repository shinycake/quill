//! message media attachments: photo/video/voice/sticker/location/dice rendering.

use super::app::QuillApp;
use super::nested_click::SwallowPress;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::progress::ProgressCircle;
use gpui_kit::component::slider::Slider;
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
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

mod audio;
mod cards;
mod document;
mod fixtures;
mod layout;
mod location;
mod sticker;
mod surfaces;
mod video_note;
mod visual;

pub(super) use audio::{audio_row, transcription_row, voice_note_row, waveform_row};
pub(super) use cards::{ContactCardState, contact_row, dice_row, paid_media_card};
pub(super) use document::{
    action_disc, bubble_accent, document_chip, download_display_name, format_bytes, inline_link,
};
pub(super) use layout::{
    MediaCorners, MediaDisc, MediaFrameKind, bubble_outer_width, media_content_width, media_disc,
    media_frame, media_frame_for, single_media_frame, single_media_width,
};
pub(super) use location::{location_row, venue_row};
pub(super) use sticker::sticker_attachment;
pub(super) use surfaces::spoiler_cover;
pub(super) use video_note::{VIDEO_NOTE_DIAMETER, video_note_attachment};
pub(super) use visual::{animation_attachment, photo_attachment, video_attachment};

use layout::{MEDIA_VISUAL_GROUP, download_fraction};
use surfaces::{
    gif_badge, inline_surface, live_tile, note_duration_badge, round_inline_surface, unseen_ring,
    video_badge,
};

pub(super) fn photo_display_path(
    photo: &quill::telegram::envelope::PhotoContent,
    files: &HashMap<i32, ParsedFile>,
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
    files: &HashMap<i32, ParsedFile>,
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
    files: &HashMap<i32, ParsedFile>,
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
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
) -> bool {
    downloading.contains(&file_id.0)
        || files
            .get(&file_id.0)
            .is_some_and(|f| f.local.is_downloading_active)
}
