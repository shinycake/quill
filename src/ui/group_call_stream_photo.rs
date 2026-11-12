//! The chat photo behind the stream window (`group_call_stream_layout.rs`).

use gpui_kit::*;
use std::sync::Arc;

/// The chat photo behind the stream, blurred once per path off the UI
/// thread. The task lives in the stage it works for: dropping the stage
/// cancels it.
#[derive(Default)]
pub(crate) enum StreamPhoto {
    /// The chat has no photo (or none was asked for yet).
    #[default]
    None,
    /// `path` is blurring.
    Blurring { path: String, _task: Task<()> },
    /// `path` blurred into `image`.
    Blurred {
        path: String,
        image: Arc<RenderImage>,
    },
    /// `path` could not be read: not blurred again until the chat's photo
    /// path changes or the window reopens (a frame never retries it).
    Failed { path: String },
}

impl StreamPhoto {
    pub(in crate::ui) fn image(&self) -> Option<Arc<RenderImage>> {
        match self {
            Self::Blurred { image, .. } => Some(image.clone()),
            _ => None,
        }
    }

    pub(super) fn path(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Blurring { path, .. } | Self::Blurred { path, .. } | Self::Failed { path } => {
                Some(path)
            }
        }
    }

    /// Leaves the stage, giving back the image to drop from the window.
    pub(super) fn take_image(&mut self) -> Option<Arc<RenderImage>> {
        match std::mem::take(self) {
            Self::Blurred { image, .. } => Some(image),
            _ => None,
        }
    }
}
