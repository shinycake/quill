//! Window and per-frame bookkeeping: animation demand, focus restore, quit guard.

use super::*;
use gpui_kit::*;
use std::path::PathBuf;
use std::time::Instant;

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
    /// The title last handed to the platform window (`window_chrome`).
    pub(super) window_title_shown: std::cell::RefCell<String>,
    /// `media_display_roots`, computed once per frame (rows ask for it
    /// one by one, and it touches the file system).
    pub(super) media_roots_frame: std::cell::RefCell<Option<Vec<PathBuf>>>,
    pub(super) frame_clock_running: std::cell::Cell<bool>,
    /// History motion: new-message reveal and selection-mode fades.
    pub(super) motion: super::motion::MotionState,
    /// Chat list column width (drag its right edge; double-click resets).
    pub(super) sidebar_width: Pixels,
    /// A debounced `window_state.json` save is scheduled.
    pub(super) window_state_save_pending: bool,
    /// Cmd+Q hold detection (`macWarnBeforeQuit`) and its clock origin.
    pub(super) quit_guard: quill::quit_guard::QuitGuard,
    pub(super) quit_clock: Instant,
    /// Whether the main window is currently excluded from screen capture.
    pub(super) capture_blocked: bool,
    /// The "screenshots can't be blocked here" note was dismissed (Linux).
    pub(super) capture_notice_dismissed: bool,
    pub(super) context_menu_focus: FocusHandle,
    pub(super) context_menu_was_open: bool,
    pub(super) context_menu_previous_focus: Option<FocusHandle>,
}

impl FrameUi {
    pub(super) fn new(cx: &mut Context<QuillApp>) -> Self {
        Self {
            animation_demand: Default::default(),
            row_fx: Default::default(),
            animation_targets: Default::default(),
            animation_sound: Default::default(),
            window_active: std::cell::Cell::new(true),
            window_title_shown: Default::default(),
            media_roots_frame: Default::default(),
            frame_clock_running: Default::default(),
            motion: Default::default(),
            sidebar_width: px(quill::settings::load_window_state()
                .map_or(quill::settings::DEFAULT_SIDEBAR_WIDTH, |state| {
                    state.sidebar_width
                })),
            window_state_save_pending: false,
            quit_guard: Default::default(),
            quit_clock: std::time::Instant::now(),
            capture_blocked: false,
            capture_notice_dismissed: false,
            context_menu_focus: cx.focus_handle(),
            context_menu_was_open: false,
            context_menu_previous_focus: None,
        }
    }
}
