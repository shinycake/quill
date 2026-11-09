//! Message widgets on screen: one entry per widget message seen, with its
//! lifecycle, its runtime while it has one, its last frame and what it
//! asked for (`ui/widgets_ui.rs` drives them, `ui/widget_bubble.rs` draws
//! them). docs/decisions/message-widgets.md.
#![deny(clippy::indexing_slicing)]

use super::owned_image::OwnedImage;
use super::widget_runtime::Runtime;
use gpui_kit::{EntityId, Task};
use quill::ids::ChatId;
use quill::message_widget::storage::{Area, Stored};
use quill::message_widget::{Lifecycle, WidgetKey};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// What one runtime was started for: a new value restarts it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct Identity {
    pub html: String,
    pub is_copy: bool,
    pub can_send_queries: bool,
    pub can_send_background_queries: bool,
}

/// When to poll the public data of a widget that asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Poll {
    /// It did not ask (or has no `widget_id`).
    Off,
    /// A poll is due at this time (on screen) or when it comes back.
    Due(Instant),
    /// A poll is with TDLib.
    Asking,
}

pub(super) struct WidgetEntry {
    pub lifecycle: Lifecycle,
    pub runtime: Option<Runtime>,
    pub identity: Option<Identity>,
    /// The `WidgetData` JSON the runtime has.
    pub data: String,
    /// Public data from the last poll, newer than the message's own.
    pub polled: Option<String>,
    pub frame: Option<(OwnedImage, f32, f32)>,
    /// The height the bubble gives it, CSS px.
    pub height: f32,
    pub width: f32,
    pub visible: bool,
    pub hidden_at: u64,
    /// The window (`None`: the main one) and view it was last drawn in.
    pub window: Option<ChatId>,
    pub view: Option<EntityId>,
    /// Stops in a row ([`quill::message_widget::MAX_CRASHES`]).
    pub crashes: u8,
    pub wants_frames: bool,
    pub cursor: String,
    pub poll: Poll,
    pub stored: Stored,
    /// The stored copy changed since the last write.
    pub unsaved: bool,
    /// The widget claims long presses of the current press (a context menu of its own).
    pub hold: bool,
    pub pressed: bool,
    /// The mouse is over it (it was told so).
    pub hovered: bool,
    /// The selected text, read after a press.
    pub selection: String,
}

impl WidgetEntry {
    pub(super) fn new(stored: Stored, height: f32) -> Self {
        Self {
            lifecycle: Lifecycle::Idle,
            runtime: None,
            identity: None,
            data: String::new(),
            polled: None,
            frame: None,
            height,
            width: 0.0,
            visible: false,
            hidden_at: 0,
            window: None,
            view: None,
            crashes: 0,
            wants_frames: false,
            cursor: String::new(),
            poll: Poll::Off,
            stored,
            unsaved: false,
            hold: false,
            pressed: false,
            hovered: false,
            selection: String::new(),
        }
    }
}

/// What the history passes of this frame drew, per window.
#[derive(Default)]
pub(super) struct Passes {
    /// The widgets each window's pass drew, with the width it gave them.
    pub seen: HashMap<Option<ChatId>, HashMap<WidgetKey, f32>>,
    /// Windows whose pass waits for its reconcile.
    pub queued: HashSet<Option<ChatId>>,
    /// The view each window's history draws in, to redraw it alone.
    pub views: HashMap<Option<ChatId>, Option<EntityId>>,
}

#[derive(Default)]
pub(crate) struct WidgetsUi {
    pub(super) entries: HashMap<WidgetKey, WidgetEntry>,
    /// Counts going off screen, for the oldest-hidden release order.
    pub(super) clock: u64,
    /// Rows note themselves here while they render (`&self`).
    pub(super) passes: RefCell<Passes>,
    /// `sessionStorage`: lives while the app runs, never written.
    pub(super) session: HashMap<WidgetKey, Area>,
    /// The loop taking the runtimes' frames and events is running.
    pub(super) landing: bool,
    /// The next public data poll's timer.
    pub(super) poll_timer: Option<(Instant, Task<()>)>,
    /// The next storage write's timer.
    pub(super) save_timer: Option<Task<()>>,
}

