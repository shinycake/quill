//! Message widgets on screen: one entry per widget message seen, with its
//! lifecycle (the live stages hold its runtime), where it is drawn, its last
//! frame and what it asked for (`ui/widgets_ui.rs` drives them,
//! `ui/widget_bubble.rs` draws them). docs/decisions/message-widgets.md.
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

/// One window's card of a widget: the width it gives it (CSS px) and the
/// view its history draws in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Showing {
    pub width: f32,
    pub view: Option<EntityId>,
}

/// Where a widget is drawn. A widget can be drawn in several windows (the
/// same chat in main and in a chat window), but it has one runtime with
/// one width: the owner's. The owner lays it out and ticks it; every other
/// window draws the owner's frame scaled into its own card
/// (`widget_bubble.rs`). Only the owner's pass moves the runtime's width,
/// so two windows never flip it between them.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Presence {
    /// No window draws it, since this count of `WidgetsUi::clock`.
    Hidden { since: u64 },
    /// Drawn by `owner` (`None`: the main window) and by `others`.
    Shown {
        owner: Option<ChatId>,
        at: Showing,
        others: HashMap<Option<ChatId>, Showing>,
    },
}

/// What a window's pass did to a widget's [`Presence`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PresenceChange {
    /// Nothing the runtime needs to hear.
    Same,
    /// It was off screen and now shows.
    CameBack,
    /// The owner's width changed: the runtime lays out at it.
    Width(f32),
    /// No window draws it any more.
    Hid,
}

impl Presence {
    pub(super) fn is_shown(&self) -> bool {
        matches!(self, Self::Shown { .. })
    }

    /// When it went off screen, 0 while on screen (the pool's order).
    pub(super) fn hidden_since(&self) -> u64 {
        match self {
            Self::Hidden { since } => *since,
            Self::Shown { .. } => 0,
        }
    }

    /// The window that lays it out, while shown.
    pub(super) fn owner(&self) -> Option<Option<ChatId>> {
        match self {
            Self::Shown { owner, .. } => Some(*owner),
            Self::Hidden { .. } => None,
        }
    }

    /// The width the runtime lays out at (0 off screen).
    pub(super) fn width(&self) -> f32 {
        match self {
            Self::Shown { at, .. } => at.width,
            Self::Hidden { .. } => 0.0,
        }
    }

    /// `window` draws it.
    pub(super) fn drawn_in(&self, window: Option<ChatId>) -> bool {
        match self {
            Self::Shown { owner, others, .. } => *owner == window || others.contains_key(&window),
            Self::Hidden { .. } => false,
        }
    }

    /// The views of every window that draws it.
    pub(super) fn views(&self) -> Vec<Option<EntityId>> {
        match self {
            Self::Shown { at, others, .. } => std::iter::once(at.view)
                .chain(others.values().map(|showing| showing.view))
                .collect(),
            Self::Hidden { .. } => Vec::new(),
        }
    }

    /// `window`'s pass drew it with `showing`. The first window to show it
    /// owns it; a width change counts from the owner only.
    pub(super) fn show(&mut self, window: Option<ChatId>, showing: Showing) -> PresenceChange {
        match self {
            Self::Hidden { .. } => {
                *self = Self::Shown {
                    owner: window,
                    at: showing,
                    others: HashMap::new(),
                };
                PresenceChange::CameBack
            }
            Self::Shown { owner, at, .. } if *owner == window => {
                at.view = showing.view.or(at.view);
                if (at.width - showing.width).abs() > 0.5 {
                    at.width = showing.width;
                    PresenceChange::Width(showing.width)
                } else {
                    PresenceChange::Same
                }
            }
            Self::Shown { others, .. } => {
                let view = others.get(&window).and_then(|old| old.view);
                others.insert(
                    window,
                    Showing {
                        width: showing.width,
                        view: showing.view.or(view),
                    },
                );
                PresenceChange::Same
            }
        }
    }

    /// `window` stopped drawing it (its pass left it out, or the window
    /// went). The owner leaving hands it to another window that draws it,
    /// at that window's width; the last one leaving hides it at `clock`.
    pub(super) fn leave(&mut self, window: Option<ChatId>, clock: u64) -> PresenceChange {
        let Self::Shown { owner, at, others } = self else {
            return PresenceChange::Same;
        };
        if *owner != window {
            others.remove(&window);
            return PresenceChange::Same;
        }
        // The next owner: the lowest window, so the pick does not depend
        // on the map's order.
        let next = others.keys().copied().min();
        match next.and_then(|next| others.remove(&next).map(|showing| (next, showing))) {
            Some((next, showing)) => {
                let moved = (at.width - showing.width).abs() > 0.5;
                *owner = next;
                *at = showing;
                if moved {
                    PresenceChange::Width(showing.width)
                } else {
                    PresenceChange::Same
                }
            }
            None => {
                *self = Self::Hidden { since: clock };
                PresenceChange::Hid
            }
        }
    }
}

pub(super) struct WidgetEntry {
    /// Its stage; the live stages hold its runtime.
    pub lifecycle: Lifecycle<Runtime>,
    pub identity: Option<Identity>,
    /// The `WidgetData` JSON the runtime has.
    pub data: String,
    /// Public data from the last poll, newer than the message's own.
    pub polled: Option<String>,
    /// The last frame, at the owner's width: image, CSS width, CSS height.
    pub frame: Option<(OwnedImage, f32, f32)>,
    /// The height the bubble gives it at the owner's width, CSS px.
    pub height: f32,
    pub presence: Presence,
    /// The window of its last press (`None`: the main one), where the
    /// links it opens and the copies it makes show while that window draws
    /// it ([`Self::asking_window`]).
    pub input_window: Option<ChatId>,
    /// Stops in a row ([`quill::message_widget::MAX_CRASHES`]).
    pub crashes: u8,
    /// The runtime animates: it ticks while it is shown.
    pub wants_frames: bool,
    pub cursor: String,
    pub poll: Poll,
    pub stored: Stored,
    /// The stored copy changed since the last write.
    pub unsaved: bool,
    /// The widget claims long presses of the current press (a context menu of its own).
    pub hold: bool,
    /// The button of the press the widget took (0 primary, 2 secondary)
    /// until its release.
    pub pressed: Option<i32>,
    /// The mouse is over it (it was told so).
    pub hovered: bool,
    /// The selected text, read after a press.
    pub selection: String,
    /// Queries with TDLib: Quill's token for each, to the token the
    /// running runtime gave it. A new runtime starts empty, so a late
    /// answer to the runtime before never reaches it.
    pub queries: HashMap<u64, u64>,
    /// The theme and bubble color the runtime has, to send only changes.
    pub theme: Option<(String, [u8; 3])>,
}

impl WidgetEntry {
    pub(super) fn new(stored: Stored, height: f32) -> Self {
        Self {
            lifecycle: Lifecycle::Idle,
            identity: None,
            data: String::new(),
            polled: None,
            frame: None,
            height,
            presence: Presence::Hidden { since: 0 },
            input_window: None,
            crashes: 0,
            wants_frames: false,
            cursor: String::new(),
            poll: Poll::Off,
            stored,
            unsaved: false,
            hold: false,
            pressed: None,
            hovered: false,
            selection: String::new(),
            queries: HashMap::new(),
            theme: None,
        }
    }

    /// The runtime, while it is live.
    pub(super) fn runtime(&self) -> Option<&Runtime> {
        self.lifecycle.runtime()
    }

    /// The window its links and copies show in: the window of its last
    /// press while that window still draws it, else its owner.
    pub(super) fn asking_window(&self) -> Option<ChatId> {
        if self.presence.drawn_in(self.input_window) {
            return self.input_window;
        }
        self.presence.owner().flatten()
    }
}

/// How a window's pass drew one widget.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Noted {
    /// The card width, CSS px.
    pub width: f32,
}

/// What the history passes of this frame drew, per window.
#[derive(Default)]
pub(super) struct Passes {
    /// The widgets each window's pass drew, with the width it gave them.
    pub seen: HashMap<Option<ChatId>, HashMap<WidgetKey, Noted>>,
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
    /// The loop taking the runtimes' frames and events, from the first
    /// start on. It is this screen's and ends with it (an account switch).
    pub(super) lander: Option<Task<()>>,
    /// The next public data poll's timer.
    pub(super) poll_timer: Option<(Instant, Task<()>)>,
    /// The next storage write's timer.
    pub(super) save_timer: Option<Task<()>>,
    /// The scale of the window drawn last: widgets start at it (0 before a frame).
    pub(super) scale: f32,
    /// Whether this screen's account still has a widget folder.
    pub(super) storage: WidgetStorage,
}

/// Whether the account on this screen keeps its widgets' storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum WidgetStorage {
    /// Widgets run and their storage is written to the account's folder.
    #[default]
    Kept,
    /// The account logged out and its folder went: no widget starts and
    /// nothing is written, until the screen is replaced (round 18 C #10: a
    /// runtime left alive wrote the folder back).
    Forgotten,
}

/// The last token Quill gave a widget query (`WidgetEntry::queries`). One
/// counter for the process, not one per screen: a screen starts over on
/// every account switch, and a per-screen counter gave a new query the
/// token of a late answer still on its way after switching A, B, A.
static LAST_QUERY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A widget query token no other query of this run had.
pub(super) fn next_query_token() -> u64 {
    LAST_QUERY
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .wrapping_add(1)
}

#[cfg(test)]
#[path = "widgets_state_tests.rs"]
mod tests;
