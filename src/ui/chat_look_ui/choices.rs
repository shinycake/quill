//! The chat look dialog's small stage values: where the dimming comes from
//! and the wallpaper file picker.
use super::{ChatLookDialog, LookStage};
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use std::rc::Rc;

/// Where the dialog's dimming comes from: the slider, which is the one
/// value (nothing mirrors it), or a fixed percent where there is no slider
/// (tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) enum Dimming {
    Slider(Entity<SliderState>),
    #[cfg(test)]
    Fixed(i32),
}

impl Dimming {
    /// The dimming now, 0-100.
    pub(in crate::ui) fn percent(&self, cx: &App) -> i32 {
        match self {
            Self::Slider(slider) => (slider.read(cx).value().end().round() as i32).clamp(0, 100),
            #[cfg(test)]
            Self::Fixed(percent) => (*percent).clamp(0, 100),
        }
    }
}

/// The wallpaper file picker's stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ui) enum WallpaperPick {
    #[default]
    Idle,
    /// The native picker is open; dropping the task closes the wait.
    Choosing(PickTask),
}

/// The picker's task, shared by the clones of the dialog the renderer takes.
#[derive(Clone)]
pub(in crate::ui) struct PickTask(Rc<Task<()>>);

impl PickTask {
    pub(in crate::ui) fn new(task: Task<()>) -> Self {
        Self(Rc::new(task))
    }
}

impl std::fmt::Debug for PickTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PickTask")
    }
}

impl PartialEq for PickTask {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for PickTask {}

impl ChatLookDialog {
    /// A choice (theme, wallpaper, dimming) changed: a refusal of the
    /// earlier choices no longer describes them.
    pub(in crate::ui) fn choices_changed(&mut self) {
        if matches!(self.stage, LookStage::Blocked(_)) {
            self.stage = LookStage::Editing;
        }
    }
}
