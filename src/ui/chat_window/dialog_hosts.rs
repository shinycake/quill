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
//!   window that asked, see [`Asker`]; [`ChatWindows::open_box_in`] for an
//!   answer, with the window of the flow that asked, such as a link's
//!   `flow_window`). Asked for again from another window while shown, it
//!   moves there. An ask made before the answer that sets the flag waits
//!   for it.
//!   `scripts/dialog_hosts.py` rejects an open that names no window, and
//!   an `open_box` (or a queued link) that runs where no window asked: the
//!   poll loop, the main window's own upkeep and the toast drain. Those
//!   carry the asking window with the request and open under
//!   [`QuillApp::as_asked_by`].
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

/// The window a session-wide box belongs to (`None`: the main window),
/// and how far it got there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Host {
    /// The window asked for it: it waits for the answer that sets its
    /// flag, and shows there then.
    Asked(Option<ChatId>),
    /// It showed in the window: free again once its flag is clear.
    Shown(Option<ChatId>),
}

impl Host {
    fn window(self) -> Option<ChatId> {
        match self {
            Self::Asked(window) | Self::Shown(window) => window,
        }
    }
}

/// The windows of the session-wide kit dialogs.
#[derive(Default)]
pub(in crate::ui) struct DialogHosts {
    hosts: HashMap<DialogKind, Host>,
    /// Session-wide kinds whose window closed with no close run, for the
    /// main window's next sync to close.
    left: Vec<DialogKind>,
}

/// Who asks for what the code running now opens: a box
/// ([`QuillApp::open_box`]), a link ([`QuillApp::queue_deep_link`]) or a
/// toast ([`QuillApp::set_note`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::ui) enum Asker {
    /// The window whose state is in place: input runs with the window of
    /// the press or key in place, a window's frame with its own, deferred
    /// work (`scoped`) with the window it started in. The default.
    InPlace,
    /// The window a flow that runs away from it carried (`None`: the main
    /// window), while that part runs ([`QuillApp::as_asked_by`]).
    Window(Option<ChatId>),
    /// No window asked: the poll loop, the main window's own upkeep and
    /// the toast drain ([`QuillApp::unasked`]). The window in place there
    /// is the main one whichever window the user acted in, so an open that
    /// did not carry its window shows in the window of the last press.
    Nobody,
}

impl QuillApp {
    /// The window that asked for what runs now (`None`: the main window),
    /// or `None` when no window did ([`Asker::Nobody`]).
    pub(in crate::ui) fn asking_window(&self) -> Option<Option<ChatId>> {
        self.windows.asking_window()
    }

    /// The window showing `chat` (`None`: the main window), as the window
    /// that asked for an answer about it: a request made from a message,
    /// a button or a bar of that chat, which shows in one window only.
    pub(in crate::ui) fn window_showing(&self, chat: ChatId) -> Option<ChatId> {
        self.windows.has_window(chat).then_some(chat)
    }

    /// The window that asked for what runs now asks for the box `kind`, as
    /// its flag is set (or before the request whose answer sets it): the
    /// box shows there, and moves there from another window that shows it.
    /// Where no window asked ([`Asker::Nobody`]) it shows in the window of
    /// the last press; such a site carries its window instead
    /// (`scripts/dialog_hosts.py`).
    pub(in crate::ui) fn open_box(&mut self, kind: DialogKind) {
        let here = self.asking_window_or_last_press();
        self.windows.open_box_in(kind, here);
    }

    /// The window of the action running now, or the window of the last
    /// press where no window asked ([`Asker::Nobody`]).
    pub(in crate::ui) fn asking_window_or_last_press(&self) -> Option<ChatId> {
        self.windows.asking_window_or_last_press()
    }

    /// Run `f`, the part of a flow that runs away from the window that
    /// started it (a resolved link, an answer drained by the poll loop),
    /// with `window` as the window that asked: its boxes and links name it.
    pub(in crate::ui) fn as_asked_by<R>(
        &mut self,
        window: Option<ChatId>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_asker(Asker::Window(window), f)
    }

    /// Run `f`, work no window asked for (the poll loop, the main
    /// window's own upkeep, the toast drain), as [`Asker::Nobody`].
    pub(in crate::ui) fn unasked<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.with_asker(Asker::Nobody, f)
    }

    /// Run `f`, a window's own frame upkeep, as the window in place
    /// ([`Asker::InPlace`]), inside work no window asked for.
    pub(in crate::ui) fn asked_in_place<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.with_asker(Asker::InPlace, f)
    }

    pub(super) fn with_asker<R>(&mut self, asker: Asker, f: impl FnOnce(&mut Self) -> R) -> R {
        let outer = std::mem::replace(&mut self.windows.asker, asker);
        let result = f(self);
        self.windows.asker = outer;
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
            .asking_window()
            .unwrap_or(self.windows.last_press)
            .filter(|chat| self.windows.has(*chat));
        self.in_window_scope(target, cx, f);
    }
}

impl ChatWindows {
    /// [`QuillApp::asking_window`], for code that holds other parts of
    /// the app borrowed (a note set with the client in hand).
    ///
    /// A carried window that has closed since the flow started asks for
    /// nothing: the main window takes what it asks for, as a box does
    /// (`open_box_in`). Without this a toast was queued for a window that
    /// never draws again, and held up every toast behind it.
    pub(in crate::ui) fn asking_window(&self) -> Option<Option<ChatId>> {
        match self.asker {
            Asker::InPlace => Some(self.scope),
            Asker::Window(window) => Some(window.filter(|chat| self.has(*chat))),
            Asker::Nobody => None,
        }
    }

    /// [`QuillApp::asking_window_or_last_press`].
    pub(in crate::ui) fn asking_window_or_last_press(&self) -> Option<ChatId> {
        self.asking_window().unwrap_or(self.last_press)
    }

    /// The last press or key was in the window whose state is in place.
    pub(in crate::ui) fn pressed_here(&self) -> bool {
        self.last_press == self.scope
    }

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
        self.dialogs.hosts.insert(kind, Host::Asked(window));
    }

    /// The window that shows, or asked for, the session-wide box `kind`
    /// (`None`: the main window); with neither, the window of the last
    /// press. An answer that follows up on a box (its request failed on a
    /// limit) opens its own box where that one is.
    pub(in crate::ui) fn window_of_box(&self, kind: DialogKind) -> Option<ChatId> {
        match self.dialogs.hosts.get(&kind) {
            Some(owner) => owner.window(),
            None => self.last_press,
        }
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
            Some(owner) => owner.window() == host,
            None => self.last_press == host,
        }
    }

    /// `host` shows the session-wide box `kind` now: it stays there until
    /// its flag is clear or another window asks for it.
    pub(in crate::ui) fn dialog_opened_in(&mut self, kind: DialogKind, host: Option<ChatId>) {
        if QuillShell::is_per_window(kind) {
            return;
        }
        self.dialogs.hosts.insert(kind, Host::Shown(host));
    }

    /// `host` showed `kind` and its flag is clear: it is free again. An
    /// ask from another window, or one still waiting for its answer, stays.
    pub(in crate::ui) fn dialog_closed_in(&mut self, kind: DialogKind, host: Option<ChatId>) {
        if self.dialogs.hosts.get(&kind) == Some(&Host::Shown(host)) {
            self.dialogs.hosts.remove(&kind);
        }
    }

    /// The session-wide boxes `host` showed.
    pub(in crate::ui) fn dialogs_shown_by(&self, host: Option<ChatId>) -> Vec<DialogKind> {
        self.dialogs
            .hosts
            .iter()
            .filter(|(_, owner)| **owner == Host::Shown(host))
            .map(|(kind, _)| *kind)
            .collect()
    }

    /// The session-wide boxes that belong to `chat`'s window, shown or
    /// asked for.
    pub(in crate::ui) fn dialogs_of_window(&self, chat: ChatId) -> Vec<DialogKind> {
        self.dialogs
            .hosts
            .iter()
            .filter(|(_, owner)| owner.window() == Some(chat))
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
            .filter(|(_, owner)| owner.window() == Some(chat_id))
            .map(|(kind, owner)| (*kind, *owner))
            .collect();
        for (kind, owner) in mine {
            self.dialogs.hosts.remove(&kind);
            if matches!(owner, Host::Shown(_)) && !self.dialogs.left.contains(&kind) {
                self.dialogs.left.push(kind);
            }
        }
    }

    /// The boxes of closed windows, for the main window's sync to close.
    pub(in crate::ui) fn take_left_dialogs(&mut self) -> Vec<DialogKind> {
        std::mem::take(&mut self.dialogs.left)
    }
}
