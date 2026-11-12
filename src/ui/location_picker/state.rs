//! What the Location picker keeps on the UI side: the search field, the
//! pointer gesture on the map, the list's scroll, and the timers that turn
//! input pauses into picker events. Everything the picker shows comes from
//! `Session::location` (`quill::location_picker`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::input::InputState;
use gpui_kit::*;

#[derive(Clone, Copy)]
pub(super) struct Drag {
    pub(super) last: Point<Pixels>,
}

/// The search field: closed, or open with its input and the debounce that
/// turns a pause in typing into a search (tdesktop `kSearchDebounceDelay`).
#[derive(Default)]
pub(super) enum Search {
    #[default]
    Closed,
    Open {
        input: Entity<InputState>,
        debounce: Wait,
    },
}

/// A change to the search field and the picker's query, together.
pub(super) enum SearchEdit {
    /// The field opens with this input.
    Open(Entity<InputState>),
    /// The text changed.
    Text(String),
    /// The field closes.
    Close,
}

impl Search {
    pub(super) fn input(&self) -> Option<&Entity<InputState>> {
        match self {
            Self::Open { input, .. } => Some(input),
            Self::Closed => None,
        }
    }
}

/// A timer the picker waits for; dropping it cancels the wait.
#[derive(Default)]
pub(super) enum Wait {
    #[default]
    Idle,
    Waiting {
        _task: Task<()>,
    },
}

/// Where the map is between a move and its address: `Settling` is
/// tdesktop's `move_end` half a second after the map stops, `Resolving` is
/// `kResolveAddressDelay` (the address waits for the pin to rest). A new
/// move starts over; each timer's own firing leaves its stage.
#[derive(Default)]
pub(super) enum MapPhase {
    #[default]
    Idle,
    Settling {
        _task: Task<()>,
    },
    Resolving {
        _task: Task<()>,
    },
}

/// The "where am I" lookup, from asking to its answer.
#[derive(Default)]
pub(super) enum Locate {
    #[default]
    Idle,
    Locating {
        _task: Task<()>,
    },
}

/// The reverse-geocode of the pin, from asking to its answer.
#[derive(Default)]
pub(super) enum Geocode {
    #[default]
    Idle,
    Resolving {
        _task: Task<()>,
    },
}

#[derive(Default)]
pub(crate) struct LocationPickerUi {
    pub(super) search: Search,
    /// The pointer gesture on the map. It is the user's hand, not a stage
    /// of the map: a drag runs across several `Settling` restarts.
    pub(super) drag: Option<Drag>,
    pub(super) zoom_carry: f32,
    pub(super) list_scroll: ScrollHandle,
    pub(super) map_phase: MapPhase,
    pub(super) locate: Locate,
    pub(super) geocode: Geocode,
    pub(super) map_focus: Option<FocusHandle>,
}

/// Its tasks (settle, resolve, search, locate, geocode) cancel on drop.
/// The picker's open flag is the session's (`location.picker`), so it
/// closes here, or it would come back on switch-back, still Locating.
impl crate::ui::on_screen::Farewell for LocationPickerUi {
    fn farewell(
        app: &mut crate::ui::app::QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<crate::ui::app::QuillApp>,
    ) {
        if app.location_picker_open() {
            app.close_location_picker(cx);
        }
    }
}

impl LocationPickerUi {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

impl Wait {
    pub(super) fn waiting(task: Task<()>) -> Self {
        Self::Waiting { _task: task }
    }
}

impl MapPhase {
    pub(super) fn settling(task: Task<()>) -> Self {
        Self::Settling { _task: task }
    }

    pub(super) fn resolving(task: Task<()>) -> Self {
        Self::Resolving { _task: task }
    }
}

impl Locate {
    pub(super) fn locating(task: Task<()>) -> Self {
        Self::Locating { _task: task }
    }
}

impl Geocode {
    pub(super) fn resolving(task: Task<()>) -> Self {
        Self::Resolving { _task: task }
    }
}
