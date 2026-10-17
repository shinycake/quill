//! Which extra items the chat-row context menu offers, after Telegram
//! Desktop's `Filler` in `window/window_peer_menu.cpp`: View profile (or
//! group / channel info), the "mark as read" entries for unread mentions,
//! reactions and poll votes, and Export chat history.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::ChatKind;

/// The extra items for one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowMenuExtras {
    /// `lng_context_view_profile` / `_group` / `_channel`; `None` for the
    /// Saved Messages chat, which has no profile to view.
    pub view_profile: Option<&'static str>,
    pub read_mentions: bool,
    pub read_reactions: bool,
    pub read_poll_votes: bool,
    /// Hidden for chats with protected content and without a live session.
    pub export: bool,
}

/// What the row's chat looks like to the menu.
#[derive(Debug, Clone, Copy)]
pub struct RowMenuFacts<'a> {
    pub kind: &'a ChatKind,
    pub is_saved_messages: bool,
    pub unread_mentions: i32,
    pub unread_reactions: i32,
    pub unread_poll_votes: i32,
    pub protected: bool,
    pub live: bool,
}

/// The profile entry's label for a chat kind; `None` when it has no profile.
pub fn profile_label(kind: &ChatKind, is_saved_messages: bool) -> Option<&'static str> {
    if is_saved_messages {
        return None;
    }
    match kind {
        ChatKind::Private { .. } | ChatKind::Secret { .. } => Some("View profile"),
        ChatKind::BasicGroup { .. } => Some("View group info"),
        ChatKind::Supergroup { is_channel, .. } => Some(if *is_channel {
            "View channel info"
        } else {
            "View group info"
        }),
        ChatKind::Unknown => None,
    }
}

/// Whether the row menu offers the Mute submenu: not for Saved Messages
/// and not for forums (`Filler::addToggleMuteSubmenu`).
pub fn mute_offered(is_saved_messages: bool, is_forum: bool) -> bool {
    !is_saved_messages && !is_forum
}

/// The entry that takes a chat out of the list, by chat kind
/// (`Filler::addDeleteChat` and `addLeaveChat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitEntry {
    /// `lng_profile_delete_conversation`, for a user or secret chat.
    DeleteChat,
    /// `lng_profile_clear_and_exit`, for a basic group.
    DeleteAndLeave,
    /// `lng_profile_leave_group` / `lng_profile_leave_channel`, for a
    /// supergroup or channel the user is still in.
    Leave { channel: bool },
}

/// `still_in` is false once the user left or was banned from the
/// supergroup or channel.
pub fn exit_entry(kind: &ChatKind, still_in: bool) -> Option<ExitEntry> {
    match kind {
        ChatKind::Private { .. } | ChatKind::Secret { .. } => Some(ExitEntry::DeleteChat),
        ChatKind::BasicGroup { .. } => Some(ExitEntry::DeleteAndLeave),
        ChatKind::Supergroup { is_channel, .. } => still_in.then_some(ExitEntry::Leave {
            channel: *is_channel,
        }),
        ChatKind::Unknown => None,
    }
}

pub fn row_menu_extras(facts: RowMenuFacts<'_>) -> RowMenuExtras {
    let view_profile = profile_label(facts.kind, facts.is_saved_messages);
    RowMenuExtras {
        view_profile,
        read_mentions: facts.unread_mentions > 0,
        read_reactions: facts.unread_reactions > 0,
        read_poll_votes: facts.unread_poll_votes > 0,
        export: facts.live && !facts.protected && facts.kind.is_supported_chat(),
    }
}

#[cfg(test)]
mod tests {
    use super::{RowMenuFacts, row_menu_extras};
    use crate::ids::UserId;
    use crate::telegram::envelope::ChatKind;

    fn facts(kind: &ChatKind) -> RowMenuFacts<'_> {
        RowMenuFacts {
            kind,
            is_saved_messages: false,
            unread_mentions: 0,
            unread_reactions: 0,
            unread_poll_votes: 0,
            protected: false,
            live: true,
        }
    }

    #[test]
    fn profile_label_follows_the_chat_kind() {
        let user = ChatKind::Private { user_id: UserId(1) };
        let group = ChatKind::BasicGroup { basic_group_id: 2 };
        let channel = ChatKind::Supergroup {
            supergroup_id: 3,
            is_channel: true,
        };
        let mega = ChatKind::Supergroup {
            supergroup_id: 4,
            is_channel: false,
        };
        assert_eq!(
            row_menu_extras(facts(&user)).view_profile,
            Some("View profile")
        );
        assert_eq!(
            row_menu_extras(facts(&group)).view_profile,
            Some("View group info")
        );
        assert_eq!(
            row_menu_extras(facts(&channel)).view_profile,
            Some("View channel info")
        );
        assert_eq!(
            row_menu_extras(facts(&mega)).view_profile,
            Some("View group info")
        );
        let mut saved = facts(&user);
        saved.is_saved_messages = true;
        assert_eq!(row_menu_extras(saved).view_profile, None);
        assert_eq!(
            row_menu_extras(facts(&ChatKind::Unknown)).view_profile,
            None
        );
    }

    #[test]
    fn read_entries_need_unread_markers() {
        let user = ChatKind::Private { user_id: UserId(1) };
        let none = row_menu_extras(facts(&user));
        assert!(!none.read_mentions && !none.read_reactions && !none.read_poll_votes);
        let mut some = facts(&user);
        some.unread_mentions = 2;
        some.unread_poll_votes = 1;
        let extras = row_menu_extras(some);
        assert!(extras.read_mentions && !extras.read_reactions && extras.read_poll_votes);
    }

    #[test]
    fn export_needs_a_live_unprotected_chat() {
        let user = ChatKind::Private { user_id: UserId(1) };
        assert!(row_menu_extras(facts(&user)).export);
        let mut protected = facts(&user);
        protected.protected = true;
        assert!(!row_menu_extras(protected).export);
        let mut offline = facts(&user);
        offline.live = false;
        assert!(!row_menu_extras(offline).export);
    }

    #[test]
    fn exit_entry_follows_the_chat_kind() {
        use super::{ExitEntry, exit_entry};
        let user = ChatKind::Private { user_id: UserId(1) };
        let group = ChatKind::BasicGroup { basic_group_id: 2 };
        let channel = ChatKind::Supergroup {
            supergroup_id: 3,
            is_channel: true,
        };
        let mega = ChatKind::Supergroup {
            supergroup_id: 4,
            is_channel: false,
        };
        assert_eq!(exit_entry(&user, true), Some(ExitEntry::DeleteChat));
        assert_eq!(exit_entry(&group, true), Some(ExitEntry::DeleteAndLeave));
        assert_eq!(
            exit_entry(&channel, true),
            Some(ExitEntry::Leave { channel: true })
        );
        assert_eq!(
            exit_entry(&mega, true),
            Some(ExitEntry::Leave { channel: false })
        );
        assert_eq!(exit_entry(&mega, false), None);
        assert_eq!(exit_entry(&ChatKind::Unknown, true), None);
    }

    #[test]
    fn mute_is_not_offered_for_saved_messages_or_forums() {
        use super::mute_offered;
        assert!(mute_offered(false, false));
        assert!(!mute_offered(true, false));
        assert!(!mute_offered(false, true));
    }
}
