//! The viewer's "saved" toast: shown with the timer that hides it, or not
//! there. Replacing or dropping the stage drops the timer, so a newer save
//! restarts it and a close ends it.
use gpui_kit::Task;
use quill::viewer_extras::SavedToast;

#[derive(Default)]
pub(in crate::ui) enum SavedToastStage {
    #[default]
    None,
    Shown {
        toast: SavedToast,
        _hide: Task<()>,
    },
}

impl SavedToastStage {
    pub(in crate::ui) fn toast(&self) -> Option<&SavedToast> {
        match self {
            Self::None => None,
            Self::Shown { toast, .. } => Some(toast),
        }
    }
}
