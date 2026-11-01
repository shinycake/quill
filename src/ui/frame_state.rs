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
    pub(super) window_views: std::cell::RefCell<std::collections::HashSet<EntityId>>,
    /// The next tick serves media playing with sound (allowed while the
    /// window is inactive).
    pub(super) animation_sound: std::cell::Cell<bool>,
    /// A window view asked while its own window was active (the voice chat
    /// window while no main or chat window is): the next tick moves the
    /// window views alone.
    pub(super) own_window_tick: std::cell::Cell<bool>,
    /// Whether a Quill window is active, the main one or a chat window
    /// (`QuillApp::follow_windows`): like tdesktop (`isGifPausedAtLeastFor`
    /// → `!widget()->isActive()`), animated stickers and emoji hold still
    /// while none is.
    pub(super) window_active: std::cell::Cell<bool>,
    /// Whether the main window itself is active, as of its last activation
    /// change or render.
    pub(super) main_active: std::cell::Cell<bool>,
    /// The active chat window hid the main window whole at its last
    /// render: the main window's own content asks for no ticks, as behind
    /// another app (`chat_window::cover`).
    pub(super) main_covered: std::cell::Cell<bool>,
    /// The open viewer's fade: what it hides gets no ticks once it has ended.
    pub(super) viewer_cover: std::cell::Cell<super::media_viewer::ViewerCover>,
    /// The title last handed to the platform window (`window_chrome`).
    pub(super) window_title_shown: std::cell::RefCell<String>,
    /// Unread badge memo for the title (`tray::badge_cache`).
    pub(super) title_badge: std::cell::RefCell<quill::tray::badge_cache::BadgeCache>,
    /// `media_display_roots`, computed once per frame (rows ask for it
    /// one by one, and it touches the file system).
    pub(super) media_roots_frame: std::cell::RefCell<Option<std::rc::Rc<[PathBuf]>>>,
    /// The main window's last frame `QuillApp::main_frame_starts` ran for.
    pub(super) frame_started: std::cell::Cell<u64>,
    /// The chat or call window's view being drawn: animated content asks
    /// ticks for it, not for the app. Not `windows.scope`, which input in a
    /// chat window leaves in place for any window to draw under.
    pub(super) tick_view: std::cell::Cell<Option<EntityId>>,
    pub(super) frame_clock_running: std::cell::Cell<bool>,
    /// The main window's display and frame lead, for the frame clock's
    /// content grid (`frame_clock::ClockTiming`).
    pub(super) clock_timing: std::cell::Cell<Option<super::frame_clock::ClockTiming>>,
    /// The main window's refresh period as its display reports it, as of
    /// its last frame (`QuillApp::note_frame_timing`).
    pub(super) main_refresh: std::cell::Cell<Option<std::time::Duration>>,
    /// What the main window's animation layers drew since their ticks
    /// (`QuillApp::note_layer_drawn`).
    pub(super) layers_drawn: std::cell::RefCell<super::layer_draws::LayerDraws>,
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
            window_views: Default::default(),
            animation_sound: Default::default(),
            own_window_tick: Default::default(),
            window_active: std::cell::Cell::new(true),
            main_active: std::cell::Cell::new(true),
            main_covered: Default::default(),
            viewer_cover: Default::default(),
            window_title_shown: Default::default(),
            title_badge: Default::default(),
            media_roots_frame: Default::default(),
            frame_started: Default::default(),
            tick_view: Default::default(),
            frame_clock_running: Default::default(),
            clock_timing: Default::default(),
            main_refresh: Default::default(),
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
