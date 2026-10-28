//! Screen-reader focus in the message history, after Telegram Desktop's
//! `HistoryInner` (`history/history_inner_widget.cpp`):
//!
//! - The list's children are the messages (an album is one) with the
//!   "N Unread Messages" bar as its own child. One of them holds
//!   accessibility focus; GPUI reports it as the focused history's active
//!   descendant, so the screen reader reads it whenever focus enters the
//!   history or moves to another message (`announceAccessibilityFocus`).
//! - Focus entering the history keeps the message focused before, or picks
//!   the first message after the unread bar, else the newest
//!   (`announceAccessibilityFocusedChild`).
//! - New messages are announced only in one case, as in tdesktop: the
//!   history held focus while the chat had no messages yet, so the first
//!   ones to arrive take focus and are read (`checkAnnounceFirstMessages`).
//!   Messages arriving in a chat that already shows some are not read out.
//! - While a screen reader runs, Up / Down, Page Up / Page Down, Home and
//!   End move the focus, Shift+Up / Down grow the selection, and Space plays
//!   or pauses the focused voice message, audio or video message when
//!   nothing is selected (`keyPressEvent`, `playPauseFocusedMedia`).
//!
//! The focused message is the selection's focus row (`selection_focus`),
//! the same row Ctrl+Space toggles, as tdesktop shares
//! `_accessibilityFocusedItem` between the two.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::history_row::HistoryRow;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::message_a11y::{MessageA11yContext, message_accessibility_name};
use quill::state::{ReplyState, message_time_hhmm};
use quill::telegram::envelope::MessageContent;

/// Accessibility focus bookkeeping of the history.
#[derive(Debug, Default)]
pub(crate) struct HistoryA11y {
    /// The unread bar holds accessibility focus (tdesktop: a focused index
    /// with no item).
    pub(super) bar_focused: bool,
    /// The history took focus while this chat had no messages: its first
    /// messages take focus when they arrive (`_announceFirstMessages`).
    pub(super) announce_first: Option<ChatId>,
    /// The history (or something inside it) has keyboard focus.
    pub(super) focused: bool,
    /// Tests: act as if a screen reader read the window (the test platform
    /// has no accessibility client).
    #[cfg(test)]
    pub(super) force_screen_reader: bool,
}

/// One accessibility child of the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum A11yItem {
    /// The "N Unread Messages" bar above the row at this index.
    Bar(usize),
    /// The message row at this index.
    Row(usize),
}

/// A key that moves accessibility focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum A11yKey {
    Up,
    Down,
    /// Page Up / Page Down move by this many children (a screen of rows).
    PageUp(usize),
    PageDown(usize),
    Home,
    End,
}

/// The history's accessibility children in order: the unread bar sits
/// before the row that carries the divider.
pub(crate) fn a11y_items(rows: &[HistoryRow]) -> Vec<A11yItem> {
    let mut items = Vec::with_capacity(rows.len().saturating_add(1));
    for (ix, row) in rows.iter().enumerate() {
        if row.unread_divider() {
            items.push(A11yItem::Bar(ix));
        }
        items.push(A11yItem::Row(ix));
    }
    items
}

/// The child focus goes to when nothing is focused: the one after the
/// unread bar, else the last.
pub(crate) fn default_focus(items: &[A11yItem]) -> Option<usize> {
    let last = items.len().checked_sub(1)?;
    let bar = items
        .iter()
        .position(|item| matches!(item, A11yItem::Bar(_)));
    Some(match bar {
        Some(bar) if bar < last => bar.saturating_add(1),
        _ => last,
    })
}

/// Where `key` takes focus among `count` children, `None` when it stays.
pub(crate) fn step(count: usize, current: Option<usize>, key: A11yKey) -> Option<usize> {
    let last = count.checked_sub(1)?;
    let next = match (key, current) {
        (A11yKey::Up | A11yKey::Down, None) => last,
        (A11yKey::Up, Some(at)) => at.saturating_sub(1),
        (A11yKey::Down, Some(at)) => at.saturating_add(1).min(last),
        // tdesktop pages from the focused child only.
        (A11yKey::PageUp(_) | A11yKey::PageDown(_), None) => return None,
        (A11yKey::PageUp(page), Some(at)) => at.saturating_sub(page.max(1)),
        (A11yKey::PageDown(page), Some(at)) => at.saturating_add(page.max(1)).min(last),
        (A11yKey::Home, _) => 0,
        (A11yKey::End, _) => last,
    };
    (Some(next) != current).then_some(next)
}

impl QuillApp {
    /// A screen reader is on and reads the window. Neither the agent UI
    /// driver, which only forces the tree to be built, nor another
    /// accessibility client (macOS `AXVisualSupportAgent`, a window manager)
    /// counts: the user's keys keep their ordinary meaning.
    pub(super) fn screen_reader_on(&self, window: &Window) -> bool {
        #[cfg(test)]
        if self.message_ui.a11y.force_screen_reader {
            return true;
        }
        window.is_a11y_system_active() && quill::screen_reader::running()
    }

    /// Something reads the accessibility tree this frame: a screen reader,
    /// or the agent UI driver. Then every slice renders uncached so the tree
    /// is whole (`app_slice::embed_slice`).
    pub(super) fn a11y_tree_wanted(&self, window: &Window) -> bool {
        #[cfg(test)]
        if self.message_ui.a11y.force_screen_reader {
            return true;
        }
        window.is_a11y_active()
    }

    /// The accessibility child focused now, if it is still in the list.
    pub(super) fn history_a11y_current(&self, items: &[A11yItem]) -> Option<usize> {
        if self.message_ui.a11y.bar_focused {
            return items
                .iter()
                .position(|item| matches!(item, A11yItem::Bar(_)));
        }
        let focus = self.message_ui.selection_focus?;
        items.iter().position(|item| match item {
            A11yItem::Row(ix) => self
                .history
                .rows
                .get(*ix)
                .is_some_and(|row| row.contains(focus)),
            A11yItem::Bar(_) => false,
        })
    }

    /// Focus `item`: the bar, or a message (the selection's focus row).
    fn set_history_a11y_focus(&mut self, item: A11yItem, cx: &mut Context<Self>) {
        // The list keeps rendered rows until they are remeasured: redo the
        // row losing focus and the one gaining it.
        let items = a11y_items(&self.history.rows);
        let previous_row = self
            .history_a11y_current(&items)
            .and_then(|at| items.get(at))
            .map(|item| match item {
                A11yItem::Row(ix) | A11yItem::Bar(ix) => *ix,
            });
        let ix = match item {
            A11yItem::Bar(ix) => {
                self.message_ui.a11y.bar_focused = true;
                ix
            }
            A11yItem::Row(ix) => {
                let Some(id) = self.history.rows.get(ix).and_then(HistoryRow::first_id) else {
                    return;
                };
                self.message_ui.a11y.bar_focused = false;
                self.message_ui.selection_focus = Some(id);
                self.message_ui.selection_anchor = Some(id);
                ix
            }
        };
        self.message_ui.a11y.announce_first = None;
        // Bring the row on screen only when it is off screen.
        let on_screen = self
            .history
            .scroll_view_probe
            .get()
            .is_some_and(|(first, last)| first < ix && ix < last);
        self.history.scroller.update(cx, |state, cx| {
            for row in previous_row.into_iter().chain([ix]) {
                let _ = state.remeasure_items(row..row + 1, cx);
            }
            if !on_screen {
                state.scroll_to_item(ix, cx);
            }
        });
        cx.notify();
    }

    /// Focus entered the history (`focusInEvent`).
    pub(super) fn history_a11y_focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.message_ui.a11y.focused = true;
        let items = a11y_items(&self.history.rows);
        if items.is_empty() {
            self.message_ui.a11y.announce_first = self.viewed_chat();
            return;
        }
        self.message_ui.a11y.announce_first = None;
        if !self.screen_reader_on(window) {
            return;
        }
        if let Some(kept) = self
            .history_a11y_current(&items)
            .and_then(|at| items.get(at))
        {
            // The kept child is reported again with the focus change.
            let (A11yItem::Row(row) | A11yItem::Bar(row)) = *kept;
            self.history.scroller.update(cx, |state, cx| {
                let _ = state.remeasure_items(row..row + 1, cx);
            });
            cx.notify();
            return;
        }
        if let Some(item) = default_focus(&items).and_then(|at| items.get(at).copied()) {
            self.set_history_a11y_focus(item, cx);
        }
    }

    /// The history rows were rebuilt: the first messages of a chat whose
    /// empty history holds focus take accessibility focus.
    pub(super) fn history_a11y_rows_changed(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.message_ui.a11y.announce_first else {
            return;
        };
        if self.viewed_chat() != Some(chat) {
            self.message_ui.a11y.announce_first = None;
            return;
        }
        if !self.message_ui.a11y.focused || self.history.rows.is_empty() {
            return;
        }
        let items = a11y_items(&self.history.rows);
        if let Some(item) = default_focus(&items).and_then(|at| items.get(at).copied()) {
            self.set_history_a11y_focus(item, cx);
        }
    }

    /// A navigation key while the history has focus and a screen reader
    /// runs. `Some(handled)` when the key belongs to accessibility focus;
    /// `None` leaves it to the ordinary handling.
    pub(super) fn history_a11y_key(
        &mut self,
        key: A11yKey,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        if !self.screen_reader_on(window) || !self.message_ui.history_focus.is_focused(window) {
            return None;
        }
        let items = a11y_items(&self.history.rows);
        if items.is_empty() {
            return None;
        }
        if extend {
            // Shift+Up / Down: grow the selection from the focused message.
            let chat = self.session().and_then(|s| s.open_chat)?;
            self.message_ui.a11y.bar_focused = false;
            if !self.selecting_in(chat) {
                self.toggle_focused_selection_by_key(window, cx);
            }
            let older = key == A11yKey::Up;
            let _ = self.step_selection_focus(older, true, window, cx);
            return Some(true);
        }
        let current = self.history_a11y_current(&items);
        let next = step(items.len(), current, key)?;
        let item = items.get(next).copied()?;
        self.set_history_a11y_focus(item, cx);
        Some(true)
    }

    /// Space with nothing selected: play or pause the focused message's
    /// voice message, audio file or video message (`playPauseFocusedMedia`).
    pub(super) fn history_a11y_play_pause(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.screen_reader_on(window) || self.message_ui.a11y.bar_focused {
            return false;
        }
        let Some(focus) = self.message_ui.selection_focus else {
            return false;
        };
        let Some(message) = self.history.rows.iter().find_map(|row| match row {
            HistoryRow::Single(inputs) if inputs.message.id == focus => {
                Some(inputs.message.clone())
            }
            _ => None,
        }) else {
            return false;
        };
        let chat = message.chat_id;
        match &message.content {
            MessageContent::VoiceNote(note) => {
                let (file, listened, duration) =
                    (note.file_id, note.is_listened, f64::from(note.duration));
                self.press_media(file, cx, |this, cx| {
                    this.toggle_voice_playback(chat, message.id, file, listened, duration, cx);
                });
            }
            MessageContent::Audio(audio) => {
                let (file, duration) = (audio.file_id, f64::from(audio.duration));
                self.press_media(file, cx, |this, cx| {
                    this.toggle_audio_playback(chat, message.id, file, duration, cx);
                });
            }
            MessageContent::VideoNote(note) => {
                if note.is_secret {
                    return false;
                }
                // The same as a click on the circle.
                self.play_video_note(chat, message.id, note, cx);
            }
            _ => return false,
        }
        true
    }

    /// What the focused row or bar needs at render: `(row focused, bar
    /// focused)` for the row at `ix`. Nothing is focused while no screen
    /// reader reads the window.
    pub(super) fn history_a11y_row_focus(&self, ix: usize) -> (bool, bool) {
        if !self.slices.a11y_active.get() {
            return (false, false);
        }
        let Some(row) = self.history.rows.get(ix) else {
            return (false, false);
        };
        if self.message_ui.a11y.bar_focused {
            return (false, row.unread_divider());
        }
        let focused = self
            .message_ui
            .selection_focus
            .is_some_and(|focus| row.contains(focus));
        (focused, false)
    }

    /// The row's accessible name (tdesktop `MessageAccessibilityName`). An
    /// album reads as its first item, the one carrying the caption.
    pub(super) fn history_row_a11y_name(&self, row: &HistoryRow) -> String {
        let message = match row {
            HistoryRow::Single(inputs) => &inputs.message,
            HistoryRow::Album { messages, .. } => {
                let Some(message) = messages
                    .iter()
                    .find(|message| has_caption(&message.content))
                    .or_else(|| messages.first())
                else {
                    return String::new();
                };
                message
            }
        };
        let chrome = row.chrome();
        let session = self.session();
        let sender_name = (!message.is_outgoing)
            .then(|| session.and_then(|s| s.sender_name_and_accent(message.sender).0))
            .flatten();
        let reply = chrome
            .reply_header
            .as_ref()
            .filter(|header| header.state == ReplyState::Ready)
            .map(|header| (header.name.clone(), header.text.clone()));
        let context = MessageA11yContext {
            sender_name,
            receipt: Some(chrome.receipt),
            via_bot: chrome.via_bot.clone(),
            reply,
            forwarded_from: chrome
                .forward_header
                .as_ref()
                .map(|header| header.name.clone()),
            time: message_time_hhmm(message.date),
        };
        let file = |id| session.and_then(|s| s.file(id)).cloned();
        message_accessibility_name(message, &context, &file)
    }
}

fn has_caption(content: &MessageContent) -> bool {
    match content {
        MessageContent::Photo(photo) => !photo.caption.is_empty(),
        MessageContent::Video(video) => !video.caption.is_empty(),
        MessageContent::Document(doc) => !doc.caption.is_empty(),
        MessageContent::Audio(audio) => !audio.caption.is_empty(),
        _ => false,
    }
}

/// Track keyboard focus entering and leaving the history.
pub(crate) fn install(handle: &FocusHandle, window: &mut Window, cx: &mut Context<QuillApp>) {
    cx.on_focus_in(handle, window, |this, window, cx| {
        // Focus listeners run at the end of a draw, where a notify does not
        // schedule the next frame: pick the child once the draw is done,
        // as the window the history is in.
        let scope = this.window_scope_of(window);
        cx.defer_in(window, move |this, window, cx| {
            this.in_window_scope(scope, cx, |this, cx| this.history_a11y_focus_in(window, cx));
        });
    })
    .detach();
    cx.on_focus_out(handle, window, |this, _, window, cx| {
        this.in_window_of(window, cx, |this, _, _| {
            this.message_ui.a11y.focused = false
        });
    })
    .detach();
}

/// Message ids are only unique within a chat: the focused row's element id.
pub(crate) fn row_element_id(message: MessageId) -> ElementId {
    ElementId::from(("history-a11y-row", message.0 as u64))
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "history_a11y_tests.rs"]
mod tests;
