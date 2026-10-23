//! What the "Clear history" confirmation offers for a chat. tdesktop
//! `boxes/delete_messages_box.cpp` (`DeleteMessagesBox` with
//! `_wipeHistoryJustClear`): one box whose text depends on the kind of chat
//! (`lng_sure_delete_history`, `lng_sure_delete_group_history`,
//! `lng_sure_delete_channel_history`, `lng_sure_delete_saved_messages`) and
//! whose "revoke" checkbox is offered only where the history can be removed
//! for the other side too (`PeerData::canRevokeFullHistory`: here TDLib's
//! `can_be_deleted_for_all_users`). A channel has no checkbox: clearing it
//! always removes it for everyone.
#![deny(clippy::indexing_slicing)]

use crate::state::ChatSummary;
use crate::telegram::envelope::ChatKind;

/// Which question the box asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearTarget {
    /// The user's own Saved Messages.
    SavedMessages,
    /// A private or secret chat.
    Contact,
    /// A basic group or supergroup.
    Group,
    /// A broadcast channel.
    Channel,
}

/// Who the history is removed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revoke {
    /// No choice: this value is sent (`false` clears for the user only).
    Fixed(bool),
    /// A checkbox, unchecked at first ("Also delete for {user}" /
    /// "Delete for everyone").
    Choose,
}

/// The confirmation for one chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearHistoryPlan {
    pub target: ClearTarget,
    pub revoke: Revoke,
    /// A group that can only be cleared for the user says so
    /// (`lng_delete_clear_for_me`).
    pub for_me_note: bool,
}

/// The box for `chat`, or `None` when TDLib allows no clearing at all.
/// `is_saved` marks the user's own chat.
pub fn plan(chat: &ChatSummary, is_saved: bool) -> Option<ClearHistoryPlan> {
    plan_for(
        &chat.kind,
        is_saved,
        chat.can_be_deleted_only_for_self,
        chat.can_be_deleted_for_all_users,
        chat.is_channel(),
    )
}

fn plan_for(
    kind: &ChatKind,
    is_saved: bool,
    only_for_self: bool,
    for_all: bool,
    is_channel: bool,
) -> Option<ClearHistoryPlan> {
    if !only_for_self && !for_all {
        return None;
    }
    let target = if is_saved {
        ClearTarget::SavedMessages
    } else if is_channel {
        ClearTarget::Channel
    } else if matches!(kind, ChatKind::Private { .. } | ChatKind::Secret { .. }) {
        ClearTarget::Contact
    } else {
        ClearTarget::Group
    };
    let revoke = match target {
        // `_revokeJustClearForChannel`: no checkbox, removed for everyone.
        ClearTarget::Channel => Revoke::Fixed(for_all),
        ClearTarget::SavedMessages => Revoke::Fixed(false),
        _ if !for_all => Revoke::Fixed(false),
        // The server accepts only the everyone variant here; tdesktop would
        // offer a checkbox that could not be unchecked.
        _ if !only_for_self => Revoke::Fixed(true),
        _ => Revoke::Choose,
    };
    Some(ClearHistoryPlan {
        target,
        revoke,
        for_me_note: target == ClearTarget::Group && revoke == Revoke::Fixed(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::UserId;

    fn private() -> ChatKind {
        ChatKind::Private { user_id: UserId(5) }
    }

    fn group() -> ChatKind {
        ChatKind::BasicGroup { basic_group_id: 2 }
    }

    #[test]
    fn nothing_to_clear_without_a_flag() {
        assert_eq!(plan_for(&private(), false, false, false, false), None);
    }

    #[test]
    fn a_contact_offers_the_checkbox_when_the_other_side_can_be_cleared() {
        let plan = plan_for(&private(), false, true, true, false).unwrap();
        assert_eq!(plan.target, ClearTarget::Contact);
        assert_eq!(plan.revoke, Revoke::Choose);
        assert!(!plan.for_me_note);
    }

    #[test]
    fn a_contact_without_revoke_clears_for_me_only() {
        let plan = plan_for(&private(), false, true, false, false).unwrap();
        assert_eq!(plan.revoke, Revoke::Fixed(false));
    }

    #[test]
    fn a_group_without_revoke_says_it_clears_for_me() {
        let plan = plan_for(&group(), false, true, false, false).unwrap();
        assert_eq!(plan.target, ClearTarget::Group);
        assert_eq!(plan.revoke, Revoke::Fixed(false));
        assert!(plan.for_me_note);
        let owner = plan_for(&group(), false, true, true, false).unwrap();
        assert_eq!(owner.revoke, Revoke::Choose);
        assert!(!owner.for_me_note);
    }

    #[test]
    fn a_channel_has_no_checkbox() {
        let supergroup = ChatKind::Supergroup {
            supergroup_id: 3,
            is_channel: true,
        };
        let plan = plan_for(&supergroup, false, false, true, true).unwrap();
        assert_eq!(plan.target, ClearTarget::Channel);
        assert_eq!(plan.revoke, Revoke::Fixed(true));
    }

    #[test]
    fn saved_messages_clear_for_me() {
        let plan = plan_for(&private(), true, true, false, false).unwrap();
        assert_eq!(plan.target, ClearTarget::SavedMessages);
        assert_eq!(plan.revoke, Revoke::Fixed(false));
    }

    #[test]
    fn everyone_only_is_not_a_choice() {
        let plan = plan_for(&private(), false, false, true, false).unwrap();
        assert_eq!(plan.revoke, Revoke::Fixed(true));
    }
}
