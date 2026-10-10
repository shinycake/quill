//! Which extra items the chat-row context menu offers, after Telegram
//! Desktop's `Filler` in `window/window_peer_menu.cpp`: View profile (or
//! group / channel info), the "mark as read" entries for unread mentions,
//! reactions and poll votes, and Export chat history.

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

pub fn row_menu_extras(facts: RowMenuFacts<'_>) -> RowMenuExtras {
    let view_profile = if facts.is_saved_messages {
        None
    } else {
        match facts.kind {
            ChatKind::Private { .. } | ChatKind::Secret { .. } => Some("View profile"),
            ChatKind::BasicGroup { .. } => Some("View group info"),
            ChatKind::Supergroup { is_channel, .. } => Some(if *is_channel {
                "View channel info"
            } else {
                "View group info"
            }),
            ChatKind::Unknown => None,
        }
    };
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
}
