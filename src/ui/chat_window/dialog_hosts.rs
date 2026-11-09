//! Which window shows each kit dialog. Every window (the main one and
//! each chat window) has its own kit dialog layer, as each tdesktop
//! window has its own layer (`Window::Controller::show(box)`). A box
//! belongs to the window that opened it, from the moment it opens:
//!
//! - **A per-window kind** (`DialogSpec::per_window`) keeps its open flag in
//!   the window's own state, swapped in with the slot (`chat_ui`,
//!   `message_ui`, `composer_ui`, `pickers`, `viewer`, `share`). The key is
//!   the kind and the window: each window shows its own flag, and a box in
//!   one window never hides or holds the same kind in another.
//! - **A session-wide kind** (the default) has one flag for the account, so
//!   it shows in one window: the one that asked for it last. Every open
//!   names its window as it sets the flag ([`QuillApp::open_box`]: the
//!   window whose state is in place, which is the window of the user
//!   action; [`ChatWindows::open_box_in`] for an answer, with the window of
//!   the flow that asked, such as a link's `flow_window`). Asked for again
//!   from another window while shown, it moves there. An ask made before
//!   the answer that sets the flag waits for it.
//!   `scripts/dialog_hosts.py` rejects an open that names no window.
//! - An open no path named (a reducer's answer nothing asked for) shows in
//!   the window of the last press or key, and stays there once shown.
//!   Pointer moves and scrolls do not count: they only put a window's state
//!   in place.
//! - A closing chat window closes its own boxes, with its state in place,
//!   each with its own close (`QuillApp::close_window_boxes`): its
//!   per-window flags and the session-wide boxes it hosts. A session-wide
//!   one still set when the window went without a close (the platform
//!   destroyed it) is closed by the main window's next sync. A kind with no
//!   close (an incoming call's swap prompt, an agent's request: each must be
//!   answered) is freed and shows in the window of the last press.
#![deny(clippy::indexing_slicing)]

use super::ChatWindows;
use crate::ui::app::QuillApp;
use crate::ui::shell::{DialogKind, QuillShell};
use quill::ids::ChatId;
use std::collections::HashMap;

/// The window a session-wide box belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Host {
    /// The window (`None`: the main window).
    window: Option<ChatId>,
    /// It showed there. An ask that has not shown yet waits for the answer
    /// that sets its flag; one that showed is free again once its flag is
    /// clear.
    shown: bool,
}

/// The windows of the session-wide kit dialogs.
#[derive(Default)]
pub(in crate::ui) struct DialogHosts {
    hosts: HashMap<DialogKind, Host>,
    /// Session-wide kinds whose window closed with no close run, for the
    /// main window's next sync to close.
    left: Vec<DialogKind>,
}

impl QuillApp {
    /// The window whose state is in place asks for the box `kind`, as its
    /// flag is set (or before the request whose answer sets it): the box
    /// shows there, and moves there from another window that shows it.
    /// Input, deferred work (`scoped`) and a window's frame each run with
    /// their window's state in place, so this is the window of the action.
    pub(in crate::ui) fn open_box(&mut self, kind: DialogKind) {
        let here = self.asking_window();
        self.windows.open_box_in(kind, here);
    }

    /// The window of the action running now: the one a flow asked from
    /// (`as_asked_by`), else the one whose state is in place.
    pub(in crate::ui) fn asking_window(&self) -> Option<ChatId> {
        self.windows.asking.unwrap_or(self.windows.scope)
    }

    /// Run `f`, the part of a flow that runs away from the window that
    /// started it (a resolved link, run in the main window's frame), with
    /// `window` as the window its boxes name ([`Self::open_box`]).
    pub(in crate::ui) fn as_asked_by<R>(
        &mut self,
        window: Option<ChatId>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let outer = self.windows.asking.replace(window);
        let result = f(self);
        self.windows.asking = outer;
        result
    }

    /// Run `f` with the asking window's state in place ([`Self::as_asked_by`]),
    /// for a per-window box such a flow opens: its flag goes into that
    /// window's own state. With no asking window, or one that closed, `f`
    /// runs with the state in place now.
    pub(in crate::ui) fn in_asking_window(
        &mut self,
        cx: &mut gpui_kit::Context<Self>,
        f: impl FnOnce(&mut Self, &mut gpui_kit::Context<Self>),
    ) {
        let target = self
            .windows
            .asking
            .flatten()
            .filter(|chat| self.windows.has(*chat))
            .or(self.windows.scope);
        self.in_window_scope(target, cx, f);
    }
}

impl ChatWindows {
    /// The window `window` (`None`: the main window) asks for the box
    /// `kind`: [`QuillApp::open_box`] for an answer whose flow knows the
    /// window that asked. A per-window kind needs no ask: its flag is in
    /// the window's own state.
    pub(in crate::ui) fn open_box_in(&mut self, kind: DialogKind, window: Option<ChatId>) {
        if QuillShell::is_per_window(kind) {
            return;
        }
        // A window that closed asks for nothing.
        let window = window.filter(|chat| self.has(*chat));
        self.dialogs.left.retain(|left| *left != kind);
        self.dialogs.hosts.insert(
            kind,
            Host {
                window,
                shown: false,
            },
        );
    }

    /// Whether the window `host` (`None`: the main window) may show the
    /// kit dialog `kind` when its flag is set with that window's state in
    /// place: any window for a per-window kind, else the window that asked
    /// for it, or with no ask the window of the last press or key.
    pub(in crate::ui) fn dialog_may_show_in(&self, kind: DialogKind, host: Option<ChatId>) -> bool {
        if QuillShell::is_per_window(kind) {
            return true;
        }
        if self.dialogs.left.contains(&kind) {
            return false;
        }
        match self.dialogs.hosts.get(&kind) {
            Some(owner) => owner.window == host,
            None => self.last_press == host,
        }
    }

    /// `host` shows the session-wide box `kind` now: it stays there until
    /// its flag is clear or another window asks for it.
    pub(in crate::ui) fn dialog_opened_in(&mut self, kind: DialogKind, host: Option<ChatId>) {
        if QuillShell::is_per_window(kind) {
            return;
        }
        self.dialogs.hosts.insert(
            kind,
            Host {
                window: host,
                shown: true,
            },
        );
    }

    /// `host` showed `kind` and its flag is clear: it is free again. An
    /// ask from another window, or one still waiting for its answer, stays.
    pub(in crate::ui) fn dialog_closed_in(&mut self, kind: DialogKind, host: Option<ChatId>) {
        let shown_here = Host {
            window: host,
            shown: true,
        };
        if self.dialogs.hosts.get(&kind) == Some(&shown_here) {
            self.dialogs.hosts.remove(&kind);
        }
    }

    /// The session-wide boxes `host` showed.
    pub(in crate::ui) fn dialogs_shown_by(&self, host: Option<ChatId>) -> Vec<DialogKind> {
        self.dialogs
            .hosts
            .iter()
            .filter(|(_, owner)| owner.window == host && owner.shown)
            .map(|(kind, _)| *kind)
            .collect()
    }

    /// The session-wide boxes that belong to `chat`'s window, shown or
    /// asked for.
    pub(in crate::ui) fn dialogs_of_window(&self, chat: ChatId) -> Vec<DialogKind> {
        self.dialogs
            .hosts
            .iter()
            .filter(|(_, owner)| owner.window == Some(chat))
            .map(|(kind, _)| *kind)
            .collect()
    }

    /// The chat window `chat_id` closed: an ask it made goes, and a box it
    /// showed that is still open waits for the main window's sync to close
    /// it (its own close already ran when the window could run it).
    pub(super) fn dialogs_window_closed(&mut self, chat_id: ChatId) {
        let mine: Vec<(DialogKind, Host)> = self
            .dialogs
            .hosts
            .iter()
            .filter(|(_, owner)| owner.window == Some(chat_id))
            .map(|(kind, owner)| (*kind, *owner))
            .collect();
        for (kind, owner) in mine {
            self.dialogs.hosts.remove(&kind);
            if owner.shown && !self.dialogs.left.contains(&kind) {
                self.dialogs.left.push(kind);
            }
        }
    }

    /// The boxes of closed windows, for the main window's sync to close.
    pub(in crate::ui) fn take_left_dialogs(&mut self) -> Vec<DialogKind> {
        std::mem::take(&mut self.dialogs.left)
    }
}
