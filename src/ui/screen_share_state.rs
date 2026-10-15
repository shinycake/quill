//! Screen sharing UI state: the source chooser, the screen recording
//! privacy box (`screen_chooser.rs`) and paused tiles (`group_call_paused.rs`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::calls::capture::screen::{ScreenBackend, ScreenSource, SourceId};
use std::collections::HashMap;
use std::sync::Arc;

/// The call a chooser or privacy box belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenFor {
    /// The voice / video chat.
    Group,
    /// A 1:1 call, by id.
    Private(i32),
}

/// One source in the chooser grid.
pub(crate) struct ChooserSource {
    pub(super) source: ScreenSource,
    pub(super) label: String,
    /// Captured once, as in tdesktop; `None` until it arrives.
    pub(super) thumbnail: Option<Arc<RenderImage>>,
}

#[derive(Default)]
pub(crate) struct ScreenShareUi {
    /// The chooser's own window (tdesktop opens it as a separate window).
    pub(super) window: Option<AnyWindowHandle>,
    pub(super) opening: bool,
    /// The call the open chooser shares into.
    pub(super) for_call: Option<ScreenFor>,
    pub(super) sources: Vec<ChooserSource>,
    pub(super) selected: Option<SourceId>,
    /// Bumped on every open, so a stale listing or thumbnail is dropped.
    pub(super) generation: u64,
    /// The privacy box shown over this call's window.
    pub(super) privacy_for: Option<ScreenFor>,
    /// A backend used instead of the OS one (demos, tests).
    pub(super) backend: Option<Arc<dyn ScreenBackend>>,
    /// The demo's "currently shared" source (live calls ask the driver).
    pub(super) demo_active: Option<SourceId>,
    /// Paused tiles: the last frame, small and blurred (tdesktop
    /// `BlurLargeImage`), by `(call, user, screen)` and frame seq.
    pub(super) paused_images: HashMap<(i32, i64, bool), (u64, Arc<RenderImage>)>,
}
