//! What the Location picker keeps on the UI side: the search field, the
//! pointer gesture on the map, the list's scroll, and the timers that turn
//! input pauses into picker events. Everything the picker shows comes from
//! `Session::location` (`quill::location_picker`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::input::InputState;
use gpui_kit::*;

/// A pan in progress: where the pointer was last.
#[derive(Clone, Copy)]
pub(super) struct Drag {
    pub(super) last: Point<Pixels>,
}

#[derive(Default)]
pub(crate) struct LocationPickerUi {
    /// The venue search field while it is open (tdesktop's title-bar search).
    pub(super) search: Option<Entity<InputState>>,
    pub(super) drag: Option<Drag>,
    /// Scroll and pinch not yet worth a whole zoom step.
    pub(super) zoom_carry: f32,
    pub(super) list_scroll: ScrollHandle,
    /// tdesktop's `move_end`, half a second after the map stops.
    pub(super) settle: Option<Task<()>>,
    /// tdesktop `kResolveAddressDelay`: the address waits for the pin to rest.
    pub(super) resolve: Option<Task<()>>,
    /// tdesktop `kSearchDebounceDelay`.
    pub(super) search_timer: Option<Task<()>>,
    pub(super) locate: Option<Task<()>>,
    pub(super) geocode: Option<Task<()>>,
    /// The map takes arrow keys and +/- while focused.
    pub(super) map_focus: Option<FocusHandle>,
}

impl LocationPickerUi {
    /// Forget the gesture, the field and every timer (the picker closed).
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}
