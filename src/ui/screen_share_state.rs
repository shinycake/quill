//! Screen sharing UI state: the source chooser, the screen recording
//! privacy box (`screen_chooser.rs`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::calls::capture::screen::{ScreenBackend, ScreenSource, SourceId};
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

/// The call a chooser or the privacy box was opened for, and the video
/// chat it was (`ScreenFor::Group`): another video chat, or none, closes
/// them (`sync_screen_share_call`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChooserTarget {
    pub(super) for_call: ScreenFor,
    pub(super) group_call: Option<i32>,
}

/// The chooser's content: its sources and what is picked. The window it
/// shows in is the stage ([`Chooser::Opening`], [`Chooser::Open`]).
pub(crate) struct OpenChooser {
    pub(super) target: ChooserTarget,
    /// Captures the thumbnails one by one. The chooser owns the task, so a
    /// chooser that closed or was replaced stops it and no picture lands in
    /// a later one.
    pub(super) _thumbnails: Task<()>,
    pub(super) sources: Vec<ChooserSource>,
    pub(super) selected: Option<SourceId>,
    /// The chooser's "Share System Audio" box (Windows only).
    pub(super) with_audio: bool,
}

/// Where the screen chooser stands: one value for the listing, the open
/// chooser and the screen access box.
#[derive(Default)]
pub(crate) enum Chooser {
    #[default]
    Closed,
    /// The sources are being listed by `_list`, which the stage owns: a
    /// closed or replaced chooser drops it, so only the live listing's
    /// answer arrives. Tagged with the screen it started on: the task is
    /// dropped when the screen changes, so a listing from an earlier screen
    /// is no listing now.
    Listing {
        target: ChooserTarget,
        epoch: super::on_screen::ScreenEpoch,
        _list: Task<()>,
    },
    /// The sources are in and the chooser's window is being made (tdesktop
    /// opens it as a window of its own). The task opens it and moves the
    /// stage to `Open`; leaving the stage drops the task, so no window is
    /// made for a chooser that is gone.
    Opening { sheet: OpenChooser, _open: Task<()> },
    Open {
        sheet: OpenChooser,
        window: AnyWindowHandle,
    },
    /// The screen access box shown over this call's window.
    PrivacyBox(ChooserTarget),
}

impl Chooser {
    /// The call a listing or an open chooser shares into.
    pub(super) fn sharing_into(&self) -> Option<ChooserTarget> {
        match self {
            Self::Listing { target, .. } => Some(*target),
            Self::Opening { sheet, .. } | Self::Open { sheet, .. } => Some(sheet.target),
            Self::Closed | Self::PrivacyBox(_) => None,
        }
    }

    /// The call the access box was opened for.
    pub(super) fn privacy_for(&self) -> Option<ChooserTarget> {
        match self {
            Self::PrivacyBox(target) => Some(*target),
            _ => None,
        }
    }

    /// The chooser's content, whether or not its window exists yet.
    pub(super) fn open(&self) -> Option<&OpenChooser> {
        match self {
            Self::Opening { sheet, .. } | Self::Open { sheet, .. } => Some(sheet),
            _ => None,
        }
    }

    pub(super) fn open_mut(&mut self) -> Option<&mut OpenChooser> {
        match self {
            Self::Opening { sheet, .. } | Self::Open { sheet, .. } => Some(sheet),
            _ => None,
        }
    }

    /// The chooser's window, once it exists.
    pub(super) fn window(&self) -> Option<AnyWindowHandle> {
        match self {
            Self::Open { window, .. } => Some(*window),
            _ => None,
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
impl ScreenShareUi {
    pub(super) fn window(&self) -> Option<AnyWindowHandle> {
        self.chooser.window()
    }

    pub(super) fn demo_source(&self) -> Option<SourceId> {
        self.demo_active.as_ref().map(|share| share.source.clone())
    }

    pub(super) fn demo_audio(&self) -> bool {
        self.demo_active
            .as_ref()
            .is_some_and(|share| share.with_audio)
    }

    pub(super) fn selected(&self) -> Option<SourceId> {
        self.chooser.open().and_then(|open| open.selected.clone())
    }

    pub(super) fn with_audio(&self) -> bool {
        self.chooser.open().is_some_and(|open| open.with_audio)
    }

    /// The video chat the open chooser was made for.
    pub(super) fn group_call(&self) -> Option<i32> {
        self.chooser
            .sharing_into()
            .and_then(|target| target.group_call)
    }

    /// Pretend the chooser was made for another video chat.
    pub(super) fn retarget_group_call(&mut self, call: i32) {
        if let Some(open) = self.chooser.open_mut() {
            open.target.group_call = Some(call);
        }
    }
}

impl ScreenShareUi {
    /// Close the access box (nothing else).
    pub(super) fn close_privacy(&mut self) {
        if self.chooser.privacy_for().is_some() {
            self.chooser = Chooser::Closed;
        }
    }
}

/// The demo's running share: its source, and whether it carries system
/// audio (only meaningful while a share runs).
pub(crate) struct DemoShare {
    pub(super) source: SourceId,
    pub(super) with_audio: bool,
}

#[derive(Default)]
pub(crate) struct ScreenShareUi {
    pub(super) chooser: Chooser,
    /// A backend used instead of the OS one (demos, tests).
    pub(super) backend: Option<Arc<dyn ScreenBackend>>,
    /// The demo's running share (live calls ask the driver).
    pub(super) demo_active: Option<DemoShare>,
    /// Show the system audio box in demos and tests, as on Windows.
    pub(super) demo_system_audio: bool,
}
