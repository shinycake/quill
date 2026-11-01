//! media_viewer.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;

/// Height of the viewer's top bar (title + actions).
const VIEWER_TOP_BAR: f32 = 56.0;
/// Width kept free on each side of the media for the prev/next arrows.
const VIEWER_SIDE_LANE: f32 = 80.0;
use super::message_media::{file_is_downloading, format_bytes, viewer_display_path};
use super::message_text::rich_text_line;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::media_viewer::{
    MediaViewer, MediaViewerItem, MediaViewerKind, SwipeAction, SwipeDir, SwipeInput,
    VIEWER_SHOW_MS, VIEWER_TRACKPAD_GESTURE_GAP, VIEWER_WAIT_HIDE_MS, VIEWER_WHEEL_NOTCH_PX,
    ViewerKeyAction, ViewerKeyMods, ViewerOrientation, ViewerSource, ViewerVideoStart, ViewerZoom,
    WheelAction, WheelInput, WheelSource, ZoomLevels, collect_media_items, controls_fade_ms,
    controls_hide_wait_ms, controls_should_hide, decide_viewer_video_start, orient_rgba,
    pinch_zoom_factor, save_media_to_downloads, seek_target_secs, viewer_delete_gate,
    viewer_key_action, wheel_zoom_factor,
};
use quill::settings::MediaPrefs;
use quill::state::HistoryMessage;
use quill::state::SharedMediaTab;
use quill::telegram::envelope::ParsedFile;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

mod actions;
mod caption;
mod chrome;
mod controller;
mod files;
mod files_demo;
mod fullscreen;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(in crate::ui) use fullscreen::VideoFullscreen;
mod geometry;
mod group_thumbs_ui;
mod layer;
mod navigation;
mod overlay;
mod overlay_controls;
mod pip;
mod playback;
mod profile_photos;
mod swipe_indicator;
mod ttl_ui;
mod video;
mod video_extras;

pub(in crate::ui) use layer::ViewerVideoLayer;
pub(in crate::ui) use pip::PipLink;

/// Viewer state that is not part of the item list: Shared Media paging,
/// video full screen, and the inactive-window pause of looping clips.
#[derive(Default)]
pub(super) struct ViewerExtra {
    /// The Shared Media tab the viewer pages over.
    pub shared_tab: SharedMediaTab,
    /// Items of that tab the viewer has already merged in.
    pub shared_seen: usize,
    /// Video full screen, kept in step with the window (`fullscreen`).
    pub(in crate::ui) video_fullscreen: fullscreen::VideoFullscreen,
    /// A looping clip paused because the window lost focus.
    pub inactive_paused: bool,
    /// B10: whose photos the viewer shows (`ViewerSource::Profile`).
    pub profile_owner: Option<quill::state::PhotoOwner>,
    /// The "saved to your Downloads folder" toast and its generation (a
    /// newer save restarts the hide timer).
    pub saved_toast: Option<quill::viewer_extras::SavedToast>,
    pub saved_toast_gen: u64,
    /// "Share at": the message and second the forward picker was opened at.
    pub share_at: Option<(ChatId, MessageId, i32)>,
    /// Turns of videos rotated in this run, by `(chat, message)`.
    pub rotations: HashMap<(i64, i64), u8>,
    /// The width the last frame laid the thumbnail strip out in, which
    /// says which profile photo thumbs to fetch.
    strip_lane: Option<f32>,
    #[cfg(test)]
    strip_asked: Vec<quill::ids::FileId>,
    /// The caption's collapsed / expanded state.
    pub(in crate::ui) caption: caption::ViewerCaption,
}

impl ViewerExtra {
    fn set_strip_lane(&mut self, lane: f32, wake: &quill::wake::Signal) {
        if self.strip_lane != Some(lane) {
            self.strip_lane = Some(lane);
            wake.poke();
        }
    }
}

/// Which part of the viewer a render draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum ViewerPart {
    /// Chat windows: the whole viewer inline, its backdrop included.
    Inline,
    /// The main window's slice under the playing picture: the media box.
    Media,
    /// The main window's slice over the playing picture: bars, arrows,
    /// caption and controls.
    Chrome,
}

impl ViewerPart {
    fn draws_media(self) -> bool {
        self != Self::Chrome
    }

    fn draws_chrome(self) -> bool {
        self != Self::Media
    }
}

/// Screenshot-capture runs render a single frame: skip fades there so the
/// shot shows the settled viewer, not frame zero of an animation.
fn still_frame() -> bool {
    std::env::var_os("QUILL_DEMO_CAPTURE").is_some()
}

/// Captures keep the controls up, since they render once after the idle
/// wait; `QUILL_DEMO_IDLE_HIDE=1` lets them fade for a capture of that.
fn controls_stay_up() -> bool {
    still_frame() && std::env::var_os("QUILL_DEMO_IDLE_HIDE").is_none()
}

/// Fade a group of viewer controls in over 200 ms or out over 600 ms
/// (tdesktop `mediaviewShowDuration` / `mediaviewHideDuration`, linear).
/// The animation id carries the flip generation, so each show/hide
/// restarts it.
fn fade_controls<E: Styled + IntoElement + 'static>(
    element: E,
    id: &'static str,
    generation: u64,
    hidden: bool,
) -> AnimationElement<E> {
    element.with_animation(
        (id, generation as usize),
        Animation::new(Duration::from_millis(controls_fade_ms(hidden))),
        move |element, t| {
            let t = if still_frame() { 1.0 } else { t };
            element.opacity(if hidden { 1.0 - t } else { t })
        },
    )
}
