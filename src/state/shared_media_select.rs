//! Multi-select in the shared-media gallery (tdesktop
//! `Info::Media::ListWidget` selection): which rows are ticked and what the
//! group may do. The rules are TDLib's `messageProperties`, recorded when a
//! row is ticked so the header never offers what one of them forbids.
#![deny(clippy::indexing_slicing)]

use super::{SharedMediaState, SharedMediaTab};
use crate::ids::MessageId;
use crate::lang::{keys, trn};
use crate::telegram::envelope::MessageActions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedMedia {
    pub message_id: MessageId,
    pub can_forward: bool,
    pub can_delete: bool,
    /// Deleting may also remove it for everyone else.
    pub can_revoke: bool,
}

impl SelectedMedia {
    /// What `actions` (`None` while TDLib has not answered) allows. An
    /// unknown message allows nothing, so a Forward or Delete offered for
    /// the group is never a guess.
    pub fn from_actions(message_id: MessageId, actions: Option<&MessageActions>) -> Self {
        Self {
            message_id,
            can_forward: actions.is_some_and(|a| a.can_be_forwarded),
            can_delete: actions
                .is_some_and(|a| a.can_be_deleted_only_for_self || a.can_be_deleted_for_all_users),
            can_revoke: actions.is_some_and(|a| a.can_be_deleted_for_all_users),
        }
    }
}

/// One entry of a gallery row's context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaMenuItem {
    GoToMessage,
    CancelDownload,
    ShowInFolder,
    Forward,
    Delete,
    Select,
    /// Over a ticked row while rows are ticked: act on the whole group.
    ForwardSelected,
    DeleteSelected,
    ClearSelection,
}

/// Most rows that can be ticked (tdesktop `MaxSelectedItems`; TDLib's
/// `forwardMessages` takes at most this many).
pub const MAX_SELECTED_MEDIA: usize = 100;

/// What decides which entries a row's menu offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaMenuFacts {
    /// The row is a message (stories and gifts are not).
    pub is_message: bool,
    /// The message's file is being downloaded.
    pub downloading: bool,
    /// The message's file is on disk.
    pub local: bool,
    pub can_forward: bool,
    pub can_delete: bool,
    pub selected: bool,
    /// Ticking is allowed (tdesktop `hasSelectRestriction` is false).
    pub can_select: bool,
    /// What the whole ticked group may do.
    pub group_can_forward: bool,
    pub group_can_delete: bool,
}

/// The menu of a gallery row in Telegram Desktop's order
/// (`Info::Media::ListWidget::showContextMenu`): Go to message, Cancel
/// download or Show in folder, Forward, Delete, Select; over a ticked row
/// it is Forward selected, Delete selected, Clear selection.
pub fn media_menu_items(facts: &MediaMenuFacts) -> Vec<MediaMenuItem> {
    let mut items = Vec::new();
    if !facts.is_message {
        return items;
    }
    if facts.selected {
        // tdesktop `OverSelectedItems`: the menu acts on the group.
        if facts.group_can_forward {
            items.push(MediaMenuItem::ForwardSelected);
        }
        if facts.group_can_delete {
            items.push(MediaMenuItem::DeleteSelected);
        }
        items.push(MediaMenuItem::ClearSelection);
        return items;
    }
    items.push(MediaMenuItem::GoToMessage);
    if facts.downloading {
        items.push(MediaMenuItem::CancelDownload);
    } else if facts.local {
        items.push(MediaMenuItem::ShowInFolder);
    }
    if facts.can_forward {
        items.push(MediaMenuItem::Forward);
    }
    if facts.can_delete {
        items.push(MediaMenuItem::Delete);
    }
    if facts.can_select {
        items.push(MediaMenuItem::Select);
    }
    items
}

impl SharedMediaState {
    /// Messages that were deleted leave every tab and the selection.
    pub fn remove_messages(&mut self, chat_id: crate::ids::ChatId, ids: &[MessageId]) {
        if self.chat_id != Some(chat_id) {
            return;
        }
        self.selection.retain(|s| !ids.contains(&s.message_id));
        for tab in self.tabs.iter_mut() {
            let before = tab.items.len();
            tab.items.retain(|item| !ids.contains(&item.message_id));
            let removed = before.saturating_sub(tab.items.len());
            tab.total_count = tab
                .total_count
                .saturating_sub(i32::try_from(removed).unwrap_or(0));
        }
    }

    pub fn is_selected(&self, message_id: MessageId) -> bool {
        self.selection.iter().any(|s| s.message_id == message_id)
    }

    /// Tick or untick a message. Ticking past [`MAX_SELECTED_MEDIA`] does
    /// nothing; false then.
    pub fn toggle_selected(&mut self, entry: SelectedMedia) -> bool {
        match self
            .selection
            .iter()
            .position(|s| s.message_id == entry.message_id)
        {
            Some(at) => {
                self.selection.remove(at);
            }
            None if self.selection.len() >= MAX_SELECTED_MEDIA => return false,
            None => self.selection.push(entry),
        }
        true
    }

    /// A `getMessageProperties` answer from the shared slot: refresh the
    /// ticked row it describes. The slot is shared with other callers, so an
    /// answer for another chat's message with the same id is ignored.
    pub fn refresh_from_answer(
        &mut self,
        answer: Option<(crate::ids::ChatId, MessageId, MessageActions)>,
    ) -> bool {
        match answer {
            Some((chat, id, actions)) if self.chat_id == Some(chat) => {
                self.replace_selected(SelectedMedia::from_actions(id, Some(&actions)))
            }
            _ => false,
        }
    }

    /// Refresh a ticked message's rules once TDLib has answered. True when
    /// something changed.
    pub fn replace_selected(&mut self, entry: SelectedMedia) -> bool {
        match self
            .selection
            .iter_mut()
            .find(|s| s.message_id == entry.message_id)
        {
            Some(slot) if *slot != entry => {
                *slot = entry;
                true
            }
            _ => false,
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    pub fn selected_ids(&self) -> Vec<MessageId> {
        let mut ids: Vec<MessageId> = self.selection.iter().map(|s| s.message_id).collect();
        ids.sort_by_key(|id| id.0);
        ids
    }

    /// tdesktop `canForwardAll`.
    pub fn can_forward_selection(&self) -> bool {
        !self.selection.is_empty() && self.selection.iter().all(|s| s.can_forward)
    }

    /// tdesktop `canDeleteAll`.
    pub fn can_delete_selection(&self) -> bool {
        !self.selection.is_empty() && self.selection.iter().all(|s| s.can_delete)
    }

    /// "Delete for everyone" is offered when every ticked message allows it.
    pub fn can_revoke_selection(&self) -> bool {
        !self.selection.is_empty() && self.selection.iter().all(|s| s.can_revoke)
    }
}

impl SharedMediaTab {
    /// The header title over a selection of `count` rows
    /// (tdesktop `lng_media_selected_*`).
    pub fn selection_title(self, count: usize) -> String {
        let key = match self {
            SharedMediaTab::Media => keys::MEDIA_SELECTED_MEDIA,
            SharedMediaTab::Photos => keys::MEDIA_SELECTED_PHOTO,
            SharedMediaTab::Videos => keys::MEDIA_SELECTED_VIDEO,
            SharedMediaTab::Gifs => keys::MEDIA_SELECTED_GIF,
            SharedMediaTab::Files => keys::MEDIA_SELECTED_FILE,
            SharedMediaTab::Music => keys::MEDIA_SELECTED_SONG,
            SharedMediaTab::Voice => keys::MEDIA_SELECTED_AUDIO,
            SharedMediaTab::Links => keys::MEDIA_SELECTED_LINK,
            SharedMediaTab::Polls => keys::MEDIA_SELECTED_POLL,
            SharedMediaTab::Stories | SharedMediaTab::Gifts => keys::MEDIA_SELECTED_MESSAGE,
        };
        trn(key, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, forward: bool, delete: bool) -> SelectedMedia {
        SelectedMedia {
            message_id: MessageId(id),
            can_forward: forward,
            can_delete: delete,
            can_revoke: false,
        }
    }

    #[test]
    fn ticking_twice_unticks() {
        let mut state = SharedMediaState::default();
        state.toggle_selected(entry(5, true, true));
        assert!(state.is_selected(MessageId(5)));
        state.toggle_selected(entry(5, true, true));
        assert!(!state.is_selected(MessageId(5)));
        assert!(state.selection.is_empty());
    }

    #[test]
    fn one_forbidden_message_hides_the_group_action() {
        let mut state = SharedMediaState::default();
        state.toggle_selected(entry(9, true, true));
        state.toggle_selected(entry(3, true, false));
        assert!(state.can_forward_selection());
        assert!(!state.can_delete_selection());
        assert_eq!(state.selected_ids(), vec![MessageId(3), MessageId(9)]);
    }

    #[test]
    fn unknown_properties_allow_nothing() {
        let unknown = SelectedMedia::from_actions(MessageId(1), None);
        assert!(!unknown.can_forward && !unknown.can_delete && !unknown.can_revoke);
        let actions = MessageActions {
            can_be_forwarded: true,
            can_be_deleted_only_for_self: true,
            ..MessageActions::default()
        };
        let known = SelectedMedia::from_actions(MessageId(1), Some(&actions));
        assert!(known.can_forward && known.can_delete && !known.can_revoke);
    }

    #[test]
    fn leaving_the_tab_or_chat_drops_the_selection() {
        let mut state = SharedMediaState::default();
        state.open_for(crate::ids::ChatId(4));
        state.toggle_selected(entry(5, true, true));
        state.select_tab(SharedMediaTab::Files);
        assert!(state.selection.is_empty());
        state.toggle_selected(entry(6, true, true));
        state.close();
        assert!(state.selection.is_empty());
    }

    fn facts() -> MediaMenuFacts {
        MediaMenuFacts {
            is_message: true,
            can_select: true,
            can_forward: true,
            can_delete: true,
            ..MediaMenuFacts::default()
        }
    }

    #[test]
    fn menu_follows_tdesktop_order() {
        use MediaMenuItem::*;
        let local = MediaMenuFacts {
            local: true,
            ..facts()
        };
        assert_eq!(
            media_menu_items(&local),
            [GoToMessage, ShowInFolder, Forward, Delete, Select]
        );
        let busy = MediaMenuFacts {
            downloading: true,
            local: true,
            ..facts()
        };
        assert_eq!(
            media_menu_items(&busy),
            [GoToMessage, CancelDownload, Forward, Delete, Select]
        );
    }

    #[test]
    fn over_a_ticked_row_the_menu_acts_on_the_group() {
        use MediaMenuItem::*;
        let ticked = MediaMenuFacts {
            selected: true,
            group_can_forward: true,
            group_can_delete: true,
            ..facts()
        };
        assert_eq!(
            media_menu_items(&ticked),
            [ForwardSelected, DeleteSelected, ClearSelection]
        );
        let mixed = MediaMenuFacts {
            group_can_forward: false,
            ..ticked
        };
        assert_eq!(media_menu_items(&mixed), [DeleteSelected, ClearSelection]);
    }

    #[test]
    fn select_is_hidden_when_nothing_can_be_ticked() {
        use MediaMenuItem::*;
        let restricted = MediaMenuFacts {
            can_select: false,
            ..facts()
        };
        assert_eq!(
            media_menu_items(&restricted),
            [GoToMessage, Forward, Delete]
        );
    }

    #[test]
    fn at_most_a_hundred_rows_can_be_ticked() {
        let mut state = SharedMediaState::default();
        for id in 1..=MAX_SELECTED_MEDIA as i64 {
            assert!(state.toggle_selected(entry(id, true, true)));
        }
        assert!(!state.toggle_selected(entry(500, true, true)));
        assert_eq!(state.selection.len(), MAX_SELECTED_MEDIA);
        assert!(
            state.toggle_selected(entry(1, true, true)),
            "unticking works"
        );
        assert!(state.toggle_selected(entry(500, true, true)));
    }

    #[test]
    fn an_answer_for_another_chats_message_is_ignored() {
        use crate::ids::ChatId;
        let mut state = SharedMediaState::default();
        state.open_for(ChatId(4));
        state.toggle_selected(SelectedMedia::from_actions(MessageId(7), None));
        let allowed = MessageActions {
            can_be_forwarded: true,
            can_be_deleted_for_all_users: true,
            ..MessageActions::default()
        };
        assert!(!state.refresh_from_answer(Some((ChatId(9), MessageId(7), allowed))));
        assert!(!state.can_delete_selection(), "same id, other chat");
        assert!(state.refresh_from_answer(Some((ChatId(4), MessageId(7), allowed))));
        assert!(state.can_delete_selection() && state.can_revoke_selection());
        assert!(!state.refresh_from_answer(None));
    }

    #[test]
    fn menu_offers_only_what_telegram_allows() {
        use MediaMenuItem::*;
        let protected = MediaMenuFacts {
            can_forward: false,
            can_delete: false,
            ..facts()
        };
        assert_eq!(media_menu_items(&protected), [GoToMessage, Select]);
        assert!(media_menu_items(&MediaMenuFacts::default()).is_empty());
    }

    fn item(id: i64) -> crate::state::SharedMediaItem {
        crate::state::SharedMediaItem {
            message_id: MessageId(id),
            glyph: "",
            label: String::new(),
            message: None,
            date: 0,
            action: crate::state::SharedMediaAction::Message,
            visual: crate::state::SharedMediaVisual::None,
            meta: crate::state::SharedMediaMeta::None,
            file: crate::ids::FileId(0),
        }
    }

    #[test]
    fn deleted_messages_leave_the_tabs_and_the_selection() {
        use crate::ids::ChatId;
        let mut state = SharedMediaState::default();
        state.open_for(ChatId(4));
        state.toggle_selected(entry(5, true, true));
        state.toggle_selected(entry(6, true, true));
        let tab = state.tab_state(SharedMediaTab::Files);
        tab.total_count = 2;
        tab.items = vec![item(5), item(6)];
        state.remove_messages(ChatId(9), &[MessageId(5)]);
        assert_eq!(state.selection.len(), 2, "another chat is untouched");
        state.remove_messages(ChatId(4), &[MessageId(5)]);
        let tab = state.tab_state(SharedMediaTab::Files);
        assert_eq!(tab.items.len(), 1);
        assert_eq!(tab.total_count, 1);
        assert_eq!(state.selected_ids(), vec![MessageId(6)]);
    }

    #[test]
    fn titles_count_by_tab() {
        assert_eq!(SharedMediaTab::Photos.selection_title(1), "1 Photo");
        assert_eq!(SharedMediaTab::Photos.selection_title(3), "3 Photos");
        assert_eq!(SharedMediaTab::Links.selection_title(2), "2 shared links");
    }
}
