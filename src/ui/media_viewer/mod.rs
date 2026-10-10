//! media_viewer.

use super::app::QuillApp;

/// Height of the viewer's top bar (title + actions).
const VIEWER_TOP_BAR: f32 = 56.0;
/// Width kept free on each side of the media for the prev/next arrows.
const VIEWER_SIDE_LANE: f32 = 80.0;
use super::message_media::{file_is_downloading, viewer_display_path};
use super::message_text::{custom_emoji_paths, rich_text_line};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState, SliderValue};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::media_viewer::{
    MediaViewer, MediaViewerItem, MediaViewerKind, VIEWER_FADE_MS, VIEWER_SHOW_MS,
    VIEWER_WAIT_HIDE_MS, VIEWER_WHEEL_NOTCH_PX, ViewerKeyAction, ViewerKeyMods, ViewerOrientation,
    ViewerSource, ViewerVideoStart, ViewerZoom, collect_media_items, controls_hide_wait_ms,
    controls_should_hide, decide_viewer_video_start, orient_rgba, save_media_to_downloads,
    seek_target_secs, viewer_delete_gate, viewer_key_action, wheel_zoom_factor,
};
use quill::playback::PlaybackClock;
use quill::settings::MediaPrefs;
use quill::state::HistoryMessage;
use quill::state::SharedMediaTab;
use quill::telegram::envelope::ParsedFile;
use quill::voice::format_voice_duration;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Child;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

mod actions;
mod navigation;
mod overlay;
mod overlay_controls;
mod playback;
mod video;

/// Viewer state that is not part of the item list: Shared Media paging,
/// video full screen, and the inactive-window pause of looping clips.
#[derive(Default)]
pub(super) struct ViewerExtra {
    /// The Shared Media tab the viewer pages over.
    pub shared_tab: SharedMediaTab,
    /// Items of that tab the viewer has already merged in.
    pub shared_seen: usize,
    /// Video full screen (tdesktop `_fullScreenVideo`): arrows seek, digits
    /// jump, Escape leaves it.
    pub video_fullscreen: bool,
    /// The window was already full screen when the mode was entered.
    pub window_was_fullscreen: bool,
    /// Give the window back to windowed mode on the next frame.
    pub restore_fullscreen: bool,
    /// A looping clip paused because the window lost focus.
    pub inactive_paused: bool,
    /// B10: whose profile photos the viewer shows (`ViewerSource::Profile`).
    pub profile_user: Option<i64>,
    /// The `chatPhoto.id` of the photo you set for that contact.
    pub profile_personal: Option<i64>,
    /// The "saved to your Downloads folder" toast and its generation (a
    /// newer save restarts the hide timer).
    pub saved_toast: Option<quill::viewer_extras::SavedToast>,
    pub saved_toast_gen: u64,
    /// The speed dial's slider and the speed shown while it is dragged.
    pub speed_slider: Option<Entity<SliderState>>,
    pub speed_preview: Option<f64>,
    /// Screenshot demo: open the speed dial on the first frame.
    pub demo_speed_dial_open: bool,
}

/// Screenshot-capture runs render a single frame: skip fades there so the
/// shot shows the settled viewer, not frame zero of an animation.
fn still_frame() -> bool {
    std::env::var_os("QUILL_DEMO_CAPTURE").is_some()
}

/// Fade a group of viewer controls in or out over `VIEWER_FADE_MS`
/// (tdesktop `mediaviewFadeDuration`). The animation id carries the flip
/// generation, so each show/hide restarts it.
fn fade_controls<E: Styled + IntoElement + 'static>(
    element: E,
    id: &'static str,
    generation: u64,
    hidden: bool,
) -> AnimationElement<E> {
    element.with_animation(
        (id, generation as usize),
        Animation::new(Duration::from_millis(VIEWER_FADE_MS)),
        move |element, t| {
            let t = if still_frame() { 1.0 } else { t };
            element.opacity(if hidden { 1.0 - t } else { t })
        },
    )
}
