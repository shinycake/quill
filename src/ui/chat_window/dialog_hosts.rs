//! Which window shows each kit dialog. Every window (the main one and
//! each chat window) has its own kit dialog layer, as each tdesktop
//! window has its own layer (`Window::Controller::show(box)`). A box
//! belongs to the window it opened in, stays there, and goes with it:
//!
//! - A kind with no owner opens in the window of the last input.
//! - Once shown, a kind is owned by that window: no other window shows
//!   it, whatever its input, until its flag is clear.
//! - An open site whose flag is session-wide and can be asked for again
//!   from another window while shown claims the kind for the asking
//!   window ([`ChatWindows::claim_dialog`]): the box moves there instead
//!   of the shown one changing under the other window.
//! - A closing chat window's boxes are closed by the next shell to sync,
//!   each with its own close (`DialogSpec::close`), so none moves to
//!   another window. A kind with no close (an incoming call's swap
//!   prompt, an agent's request: each must be answered) is freed and
//!   shows in the window of the last input.
#![deny(clippy::indexing_slicing)]

use super::ChatWindows;
use crate::ui::shell::DialogKind;
use quill::ids::ChatId;
use std::collections::HashMap;

/// The owners of the shown kit dialogs.
#[derive(Default)]
pub(in crate::ui) struct DialogHosts {
    /// Kind to the window that shows it (`None`: the main window).
    owners: HashMap<DialogKind, Option<ChatId>>,
    /// Kinds whose window closed, waiting for their close.
    left: Vec<DialogKind>,
}

impl ChatWindows {
    /// Whether the window `host` (`None`: the main window) may show the
    /// kit dialog `kind`: its owner, or with no owner the window the last
    /// input happened in.
    pub(in crate::ui) fn dialog_may_show_in(&self, kind: DialogKind, host: Option<ChatId>) -> bool {
        if self.dialogs.left.contains(&kind) {
            return false;
        }
        match self.dialogs.owners.get(&kind) {
            Some(owner) => *owner == host,
            None => self.last_input == host,
        }
    }

    /// `host` shows `kind` now.
    pub(in crate::ui) fn dialog_opened_in(&mut self, kind: DialogKind, host: Option<ChatId>) {
        self.dialogs.owners.insert(kind, host);
    }

    /// `host` no longer shows `kind`. Another window's claim stays.
    pub(in crate::ui) fn dialog_closed_in(&mut self, kind: DialogKind, host: Option<ChatId>) {
        if self.dialogs.owners.get(&kind) == Some(&host) {
            self.dialogs.owners.remove(&kind);
        }
    }

    /// The kinds `host` owns.
    pub(in crate::ui) fn dialogs_of(&self, host: Option<ChatId>) -> Vec<DialogKind> {
        self.dialogs
            .owners
            .iter()
            .filter(|(_, owner)| **owner == host)
            .map(|(kind, _)| *kind)
            .collect()
    }

    /// The box `kind` is asked for by the window whose state is in place
    /// (its input): it shows there, and leaves the window that showed it.
    pub(in crate::ui) fn claim_dialog(&mut self, kind: DialogKind) {
        let here = self.scope;
        self.dialogs.left.retain(|left| *left != kind);
        self.dialogs.owners.insert(kind, here);
    }

    /// The chat window `chat_id` closed: its boxes wait for their close.
    pub(super) fn dialogs_window_closed(&mut self, chat_id: ChatId) {
        let gone: Vec<DialogKind> = self.dialogs_of(Some(chat_id));
        for kind in gone {
            self.dialogs.owners.remove(&kind);
            if !self.dialogs.left.contains(&kind) {
                self.dialogs.left.push(kind);
            }
        }
    }

    /// The boxes of closed windows, for the shell to close.
    pub(in crate::ui) fn take_left_dialogs(&mut self) -> Vec<DialogKind> {
        std::mem::take(&mut self.dialogs.left)
    }
}
