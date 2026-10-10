//! The keyboard shortcut pack, after Telegram Desktop: reply navigation
//! (`HistoryWidget::replyToPreviousMessage` / `replyToNextMessage`), attach
//! (`chooseAttach`), history scrolling and Delete on a selection
//! (`HistoryInner::keyPressEvent`), and the chat-list commands of
//! `core/shortcuts.cpp` (pinned chats, Saved Messages, Archive, Contacts,
//! first/last chat, folders, mark read, chat menu, chat preview).
//!
//! Every handler returns whether it used the key; the action listener
//! propagates the keystroke when it did not, so the composer (or any other
//! input) keeps keys the history has no claim on.

use super::app::{ChatListFilter, PaneMode, QuillApp};
use super::chat_row::ChatListItem;
use super::menu_states::ChatMenuState;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};

/// What a Cmd/Ctrl+Up / Down press does to the reply target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplyNav {
    /// Reply to this message and show it.
    Reply(i64),
    /// Cmd/Ctrl+Down past the newest message: cancel the reply.
    Clear,
    /// Nothing to do; the key goes on to the focused input.
    Ignore,
}

/// `replyToPreviousMessage` / `replyToNextMessage` over the replyable
/// message ids of the open chat, oldest first. With no reply yet, Up starts
/// at the newest message (only when the window ends at the live edge) and
/// Down does nothing.
pub(super) fn reply_nav(
    ids: &[i64],
    current: Option<i64>,
    forward: bool,
    at_latest: bool,
) -> ReplyNav {
    let Some(current) = current else {
        return match (forward, ids.last()) {
            (false, Some(&last)) if at_latest => ReplyNav::Reply(last),
            _ => ReplyNav::Ignore,
        };
    };
    let Some(pos) = ids.iter().position(|&id| id == current) else {
        return ReplyNav::Ignore;
    };
    if forward {
        match ids.get(pos + 1) {
            Some(&next) => ReplyNav::Reply(next),
            None => ReplyNav::Clear,
        }
    } else if pos > 0 {
        ReplyNav::Reply(ids[pos - 1])
    } else {
        ReplyNav::Ignore
    }
}

/// Where a page key scrolls the history, as a row index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PageTarget {
    /// Put this row at the top of the view.
    Row(usize),
    /// Already on the last page: go to the end.
    End,
}

/// PageUp: the row one page above, the rows on screen being
/// `first..=last` (one row of overlap is kept, like a page of text).
pub(super) fn page_up_target(first: usize, last: usize) -> PageTarget {
    PageTarget::Row(first.saturating_sub(last.saturating_sub(first).max(1)))
}

/// PageDown: the last visible row becomes the top one.
pub(super) fn page_down_target(first: usize, last: usize, count: usize) -> PageTarget {
    if last + 1 >= count {
        PageTarget::End
    } else {
        PageTarget::Row(last.max(first + 1))
    }
}

/// `CheckAndJumpToNearChatsFilter`: the neighbouring folder tab (the first
/// slot is "All chats"); no wrap-around at the ends.
pub(super) fn near_folder(
    tabs: &[Option<i32>],
    current: Option<i32>,
    forward: bool,
) -> Option<Option<i32>> {
    let pos = tabs.iter().position(|tab| *tab == current)?;
    let next = if forward {
        pos.checked_add(1)?
    } else {
        pos.checked_sub(1)?
    };
    tabs.get(next).copied()
}

/// The pinned chats of the list on screen, in on-screen order. A folder
/// shows a subset of the pinned chats, which keep their order.
pub(super) fn visible_pinned(visible: &[ChatId], pinned: &[i64]) -> Vec<ChatId> {
    visible
        .iter()
        .copied()
        .filter(|id| pinned.contains(&id.0))
        .collect()
}

/// The history-scrolling keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum HistoryKey {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

impl QuillApp {
    /// Shortcuts are off while a dialog, viewer or the lock screen owns the
    /// window, and before the account is signed in.
    fn shortcut_blocked(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.pane_mode() != PaneMode::Ready
            || self.account.passcode.locked
            || self.viewer.state.is_open()
            || self.stories.viewer.is_open()
            || window.has_active_dialog(cx)
    }

    fn composer_has_focus(&self, window: &Window, cx: &App) -> bool {
        self.composer.read(cx).focus_handle(cx).is_focused(window)
    }

    fn composer_text_is_empty(&self, cx: &App) -> bool {
        self.composer.read(cx).value().is_empty()
    }

    /// The listed chats, top to bottom.
    fn listed_chat_ids(&self) -> Vec<ChatId> {
        self.chat_list
            .items
            .iter()
            .filter_map(|item| match item {
                ChatListItem::Chat { id, .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    /// Cmd/Ctrl+Up (`forward == false`) and Down: reply to the previous /
    /// next message and scroll to it. On macOS Cmd+Up / Down move the caret
    /// to the start / end of a draft, so tdesktop only replies there with
    /// an empty field.
    pub(super) fn reply_by_key(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) || self.composer_ui.pending_edit.is_some() {
            return false;
        }
        if cfg!(target_os = "macos")
            && self.composer_has_focus(window, cx)
            && !self.composer_text_is_empty(cx)
        {
            return false;
        }
        let Some(session) = self.session() else {
            return false;
        };
        let Some(chat_id) = session.open_chat else {
            return false;
        };
        // tdesktop skips forums; a topic or comment thread is the same
        // kind of sub-view here.
        if session.open_topic.is_some() || self.thread_active() {
            return false;
        }
        let Some(chat) = session.chats.get(&chat_id.0) else {
            return false;
        };
        if !(chat.can_post() && chat.can_send_basic_messages) {
            return false;
        }
        if self
            .composer_ui
            .pending_reply
            .as_ref()
            .is_some_and(|reply| reply.chat_id != chat_id)
        {
            return false;
        }
        let Some(history) = session.histories.get(&chat_id.0) else {
            return false;
        };
        let ids: Vec<i64> = history
            .messages
            .values()
            .filter(|m| {
                m.id.0 > 0
                    && !m.pending
                    && !m.failed
                    && !super::service_row::is_service_row(&m.content)
            })
            .map(|m| m.id.0)
            .collect();
        let current = self
            .composer_ui
            .pending_reply
            .as_ref()
            .map(|reply| reply.message_id.0);
        match reply_nav(&ids, current, forward, !history.has_newer) {
            ReplyNav::Reply(id) => {
                self.jump_to_replied_message(MessageId(id), cx);
                self.begin_reply_from_message(chat_id, MessageId(id), window, cx);
                true
            }
            ReplyNav::Clear => {
                self.clear_reply(cx);
                true
            }
            ReplyNav::Ignore => false,
        }
    }

    /// Cmd/Ctrl+O: the attach picker (`chooseAttach`).
    pub(super) fn attach_by_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.shortcut_blocked(window, cx) || self.composer_ui.pending_edit.is_some() {
            return false;
        }
        let can_attach = self.session().is_some_and(|session| {
            session
                .open_chat
                .and_then(|chat| session.chats.get(&chat.0))
                .is_some_and(|chat| chat.can_post() && chat.can_send_basic_messages)
        });
        if !can_attach {
            return false;
        }
        self.pick_attachments(false, cx);
        true
    }

    /// PageUp / PageDown / Home / End. The composer keeps Home / End (they
    /// never reach this handler: its own binding is deeper) and the page
    /// keys for a draft of several lines; otherwise the page keys scroll
    /// the history, as `InputField::_customUpDown` hands them over.
    pub(super) fn scroll_history_by_key(
        &mut self,
        key: HistoryKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) || self.session().and_then(|s| s.open_chat).is_none() {
            return false;
        }
        if self.composer_has_focus(window, cx)
            && matches!(key, HistoryKey::PageUp | HistoryKey::PageDown)
            && self.composer.read(cx).value().contains('\n')
        {
            return false;
        }
        match key {
            HistoryKey::Top => {
                self.history
                    .scroller
                    .update(cx, |state, cx| state.scroll_to_item(0, cx));
                self.maybe_auto_load_older(cx);
            }
            HistoryKey::Bottom => self.jump_to_latest_messages(cx),
            HistoryKey::PageUp | HistoryKey::PageDown => {
                let Some((first, last)) = self.history.scroll_view_probe.get() else {
                    return false;
                };
                let count = self.history.scroller.read(cx).item_count();
                let target = if key == HistoryKey::PageUp {
                    page_up_target(first, last)
                } else {
                    page_down_target(first, last, count)
                };
                self.history.scroller.update(cx, |state, cx| match target {
                    PageTarget::Row(row) => {
                        state.scroll_to_item(row, cx);
                    }
                    PageTarget::End => state.scroll_to_end(cx),
                });
                if key == HistoryKey::PageUp && first == 0 {
                    self.maybe_auto_load_older(cx);
                }
            }
        }
        cx.notify();
        true
    }

    /// Delete / Backspace with messages selected: the delete box
    /// (`confirmDeleteSelected`). A focused composer with text keeps its
    /// own Backspace.
    pub(super) fn delete_selection_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return false;
        };
        if !self.selecting_in(chat_id) {
            return false;
        }
        if self.composer_has_focus(window, cx) && !self.composer_text_is_empty(cx) {
            return false;
        }
        self.confirm_delete_selection(window, cx);
        true
    }

    /// Cmd/Ctrl+Space: toggle the focused message in the selection, which
    /// starts selection mode (`HistoryInner::keyPressEvent`). With no
    /// focused message it acts on the newest loaded one.
    pub(super) fn toggle_focused_selection_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return false;
        };
        let ids = self.loaded_selectable_ids(chat_id);
        let focus = self
            .message_ui
            .selection_focus
            .filter(|id| ids.contains(id))
            .or(ids.last().copied());
        let Some(focus) = focus else {
            return false;
        };
        self.message_ui.selection_anchor = Some(focus);
        self.message_ui.selection_focus = Some(focus);
        self.toggle_forward_select(chat_id, focus, false, cx);
        true
    }

    /// Up / Down while selecting move the focused row; with Shift they
    /// grow or shrink the range from the row the move started on. A
    /// composer with text keeps its arrow keys.
    pub(super) fn move_selection_focus_by_key(
        &mut self,
        older: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return false;
        };
        if !self.selecting_in(chat_id) {
            return false;
        }
        if self.composer_has_focus(window, cx) && !self.composer_text_is_empty(cx) {
            return false;
        }
        let ids = self.loaded_selectable_ids(chat_id);
        let old = self
            .message_ui
            .selection_focus
            .filter(|id| ids.contains(id));
        let Some(new) = quill::selection_pin::step_focus(&ids, old, older) else {
            return false;
        };
        if extend {
            let anchor = self
                .message_ui
                .selection_anchor
                .filter(|id| ids.contains(id))
                .or(old)
                .unwrap_or(new);
            let (select, deselect) =
                quill::selection_pin::range_delta(&ids, anchor, old.unwrap_or(anchor), new);
            let mut keep = vec![anchor];
            keep.extend(select);
            self.add_to_selection(chat_id, keep);
            if let Some(draft) = self.share.pending_forward.as_mut() {
                let leaving: Vec<_> = deselect
                    .into_iter()
                    .filter(|id| draft.contains(*id))
                    .collect();
                for id in leaving {
                    draft.toggle(chat_id, id, false);
                }
            }
            self.message_ui.selection_anchor = Some(anchor);
        } else {
            self.message_ui.selection_anchor = Some(new);
        }
        self.message_ui.selection_focus = Some(new);
        self.jump_to_replied_message(new, cx);
        true
    }

    /// Cmd/Ctrl+1..8: the Nth pinned chat of the list on screen.
    pub(super) fn open_pinned_by_key(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(session) = self.session() else {
            return false;
        };
        let pinned = session.pinned_chat_ids(self.chat_list.filter == ChatListFilter::Archived);
        let chats = visible_pinned(&self.listed_chat_ids(), &pinned);
        let Some(&chat_id) = chats.get(index) else {
            return false;
        };
        self.select_listed_chat(chat_id, window, cx);
        true
    }

    /// Cmd/Ctrl+0.
    pub(super) fn open_saved_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        self.open_saved_messages(window, cx);
        true
    }

    /// Cmd/Ctrl+9, only with something archived (tdesktop checks the
    /// folder is not empty).
    pub(super) fn open_archive_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        if !self
            .session()
            .is_some_and(|session| session.chats.values().any(|chat| chat.in_archive))
        {
            return false;
        }
        self.open_archive_folder(cx);
        true
    }

    /// Cmd/Ctrl+J.
    pub(super) fn open_contacts_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        self.open_contacts_tab(cx);
        true
    }

    /// Cmd/Ctrl+Alt+Home / End: the first / last listed chat.
    pub(super) fn open_edge_chat_by_key(
        &mut self,
        last: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let ids = self.listed_chat_ids();
        let Some(&id) = (if last { ids.last() } else { ids.first() }) else {
            return false;
        };
        self.select_listed_chat(id, window, cx);
        true
    }

    /// Ctrl+Shift+Up / Down: the previous / next folder tab.
    pub(super) fn step_folder_by_key(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(session) = self.session() else {
            return false;
        };
        let tabs: Vec<Option<i32>> = std::iter::once(None)
            .chain(session.chat_folders.iter().map(|f| Some(f.id)))
            .collect();
        let Some(target) = near_folder(&tabs, self.folders.tab, forward) else {
            return false;
        };
        self.open_folder_tab(target, cx);
        true
    }

    /// Cmd/Ctrl+R: mark the open chat as read (only when it has unread
    /// messages or the manual mark, as `IsUnreadThread`).
    pub(super) fn mark_read_by_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return false;
        };
        let unread = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.unread_count > 0 || chat.is_marked_as_unread);
        if unread {
            self.toggle_chat_marked_as_unread(chat_id, cx);
        }
        true
    }

    /// Cmd/Ctrl+\: the open chat's context menu, at the top right where
    /// tdesktop's top bar menu opens.
    pub(super) fn chat_menu_by_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return false;
        };
        let size = window.viewport_size();
        self.chat_list.preview = None;
        self.chat_list.menu = Some(ChatMenuState {
            chat_id,
            position: point(size.width - px(220.), px(64.)),
        });
        cx.notify();
        true
    }

    /// Cmd/Ctrl+]: the peek preview of the open chat.
    pub(super) fn chat_preview_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return false;
        };
        let size = window.viewport_size();
        self.open_chat_preview(chat_id, point(size.width * 0.5, px(96.)), cx);
        true
    }
}

#[cfg(test)]
mod tests {
    // Explicit imports, not `use super::*`: see keybindings.rs.
    use super::{
        PageTarget, ReplyNav, near_folder, page_down_target, page_up_target, reply_nav,
        visible_pinned,
    };
    use quill::ids::ChatId;

    #[test]
    fn up_without_a_reply_starts_at_the_newest_message() {
        assert_eq!(reply_nav(&[1, 2, 3], None, false, true), ReplyNav::Reply(3));
        // A window that stops short of the live edge has no "newest".
        assert_eq!(reply_nav(&[1, 2, 3], None, false, false), ReplyNav::Ignore);
        assert_eq!(reply_nav(&[], None, false, true), ReplyNav::Ignore);
    }

    #[test]
    fn down_without_a_reply_does_nothing() {
        assert_eq!(reply_nav(&[1, 2, 3], None, true, true), ReplyNav::Ignore);
    }

    #[test]
    fn up_and_down_walk_the_replied_message() {
        assert_eq!(
            reply_nav(&[1, 2, 3], Some(3), false, true),
            ReplyNav::Reply(2)
        );
        assert_eq!(
            reply_nav(&[1, 2, 3], Some(1), true, true),
            ReplyNav::Reply(2)
        );
        // Before the oldest loaded message there is nothing to reply to.
        assert_eq!(
            reply_nav(&[1, 2, 3], Some(1), false, true),
            ReplyNav::Ignore
        );
        // Past the newest, Down cancels the reply.
        assert_eq!(reply_nav(&[1, 2, 3], Some(3), true, true), ReplyNav::Clear);
        // A reply target that left the window is left alone.
        assert_eq!(reply_nav(&[1, 2, 3], Some(9), true, true), ReplyNav::Ignore);
    }

    #[test]
    fn pages_move_by_the_rows_on_screen() {
        assert_eq!(page_up_target(10, 15), PageTarget::Row(5));
        assert_eq!(page_up_target(3, 8), PageTarget::Row(0));
        assert_eq!(page_up_target(4, 4), PageTarget::Row(3));
        assert_eq!(page_down_target(10, 15, 40), PageTarget::Row(15));
        assert_eq!(page_down_target(10, 10, 40), PageTarget::Row(11));
        assert_eq!(page_down_target(30, 39, 40), PageTarget::End);
    }

    #[test]
    fn folders_do_not_wrap() {
        let tabs = [None, Some(2), Some(7)];
        assert_eq!(near_folder(&tabs, None, true), Some(Some(2)));
        assert_eq!(near_folder(&tabs, Some(7), false), Some(Some(2)));
        assert_eq!(near_folder(&tabs, Some(2), false), Some(None));
        assert_eq!(near_folder(&tabs, None, false), None);
        assert_eq!(near_folder(&tabs, Some(7), true), None);
        assert_eq!(near_folder(&tabs, Some(99), true), None);
    }

    #[test]
    fn pinned_chats_keep_the_order_on_screen() {
        let visible = [ChatId(5), ChatId(9), ChatId(1), ChatId(7)];
        assert_eq!(
            visible_pinned(&visible, &[1, 5, 77]),
            vec![ChatId(5), ChatId(1)]
        );
        assert!(visible_pinned(&visible, &[]).is_empty());
    }
}
