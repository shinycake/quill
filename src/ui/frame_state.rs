//! Window and per-frame bookkeeping: animation demand, focus restore.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::*;
use std::path::PathBuf;

pub(crate) struct FrameUi {
    /// Highest frame rate animated content asked for since the last
    /// clock tick (0: nothing animated rendered); see `frame_clock`.
    pub(super) animation_demand: std::cell::Cell<u32>,
    /// Chat-row online-dot and unread-badge animation state.
    pub(super) row_fx: std::cell::RefCell<quill::row_fx::RowFxMap>,
    /// What asked for the next tick: cached slices by entity id, `None`
    /// for `QuillApp` itself (see `frame_clock`).
    pub(super) animation_targets: std::cell::RefCell<std::collections::HashSet<Option<EntityId>>>,
    /// The next tick serves media playing with sound (allowed while the
    /// window is inactive).
    pub(super) animation_sound: std::cell::Cell<bool>,
    /// Whether the main window is active this frame: like tdesktop
    /// (`isGifPausedAtLeastFor` → `!widget()->isActive()`), animated
    /// stickers and emoji hold still while it isn't.
    pub(super) window_active: std::cell::Cell<bool>,
    /// The active chat window hid the main window whole at its last
    /// render: the main window's own content asks for no ticks, as behind
    /// another app (`chat_window::cover`).
    pub(super) main_covered: std::cell::Cell<bool>,
    /// The title last handed to the platform window (`window_chrome`).
    pub(super) window_title_shown: std::cell::RefCell<String>,
    /// Unread badge memo for the title (`tray::badge_cache`).
    pub(super) title_badge: std::cell::RefCell<quill::tray::badge_cache::BadgeCache>,
    /// `media_display_roots`, computed once per frame (rows ask for it
    /// one by one, and it touches the file system).
    pub(super) media_roots_frame: std::cell::RefCell<Option<std::rc::Rc<[PathBuf]>>>,
    pub(super) frame_clock_running: std::cell::Cell<bool>,
    /// The main window's display and frame lead, for the frame clock's
    /// content grid (`frame_clock::ClockTiming`).
    pub(super) clock_timing: std::cell::Cell<Option<super::frame_clock::ClockTiming>>,
    /// What the main window's animation layers drew since their ticks
    /// (`QuillApp::note_layer_drawn`).
    pub(super) layers_drawn: std::cell::RefCell<super::frame_clock::LayerDraws>,
    /// History motion: new-message reveal and selection-mode fades.
    pub(super) motion: super::motion::MotionState,
    /// Chat list column width (drag its right edge; double-click resets).
    pub(super) sidebar_width: Pixels,
    /// Whether the chat list column is hidden, and its slide and peek.
    pub(super) sidebar: super::sidebar_reveal::SidebarReveal,
    /// The main window's content width as last drawn (`shell_layout`).
    pub(super) main_width: Pixels,
    /// A debounced `window_state.json` save is scheduled.
    pub(super) window_state_save_pending: bool,
    /// Whether the main window is currently excluded from screen capture.
    pub(super) capture_blocked: bool,
    /// The "screenshots can't be blocked here" note was dismissed (Linux).
    pub(super) capture_notice_dismissed: bool,
    pub(super) context_menu_focus: FocusHandle,
    /// Focus handles of the open context menu's items.
    pub(super) context_menu_items: super::context_menu_keys::ContextMenuItems,
    pub(super) context_menu_was_open: bool,
    pub(super) context_menu_previous_focus: Option<FocusHandle>,
    /// Focus return for hand-drawn layers (see `esc_stack::track_layer_focus`).
    pub(super) layer_focus: super::esc_stack::LayerFocus,
    /// The one timer behind the debounced draft save (`draft_timer.rs`).
    /// App-wide like the driver's pending draft it commits: a chat
    /// window's state swapped in when it fires must not hide it.
    pub(super) draft_timer: super::draft_timer::DraftTimer,
}

impl FrameUi {
    pub(super) fn new(cx: &mut Context<QuillApp>) -> Self {
        let stored = quill::settings::load_window_state();
        Self {
            animation_demand: Default::default(),
            row_fx: Default::default(),
            animation_targets: Default::default(),
            animation_sound: Default::default(),
            window_active: std::cell::Cell::new(true),
            main_covered: Default::default(),
            window_title_shown: Default::default(),
            title_badge: Default::default(),
            media_roots_frame: Default::default(),
            frame_clock_running: Default::default(),
            clock_timing: Default::default(),
            layers_drawn: Default::default(),
            motion: Default::default(),
            sidebar_width: px(
                stored.map_or(quill::settings::DEFAULT_SIDEBAR_WIDTH, |state| {
                    state.sidebar_width
                }),
            ),
            sidebar: super::sidebar_reveal::SidebarReveal::new(
                stored.is_some_and(|state| state.sidebar_hidden),
                std::time::Instant::now(),
            ),
            main_width: px(0.),
            window_state_save_pending: false,
            capture_blocked: false,
            capture_notice_dismissed: false,
            context_menu_focus: cx.focus_handle(),
            context_menu_items: Default::default(),
            context_menu_was_open: false,
            context_menu_previous_focus: None,
            layer_focus: Default::default(),
            draft_timer: Default::default(),
        }
    }
}
