//! What `updateSupergroup` and `supergroupFullInfo` say about one
//! supergroup that the viewer's controls read: own rights and the group's
//! flags. Each is one value per supergroup, replaced whole by the update that
//! carries it, so the rights and flags of one answer never mix with those of
//! another.
use super::*;

/// The viewer's rights and the group's flags from `updateSupergroup` /
/// `getSupergroup` (schema 1.8.67, lines 1090-1092 and 2731-2746). A right
/// absent from the update is lacking; an unknown supergroup has none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupergroupFacts {
    /// `setChatSlowModeDelay` and join-by-request need it.
    pub can_restrict_members: bool,
    /// Invite-link and join-request management.
    pub can_invite_users: bool,
    /// Admin management.
    pub can_promote_members: bool,
    /// Custom titles of other members.
    pub can_manage_tags: bool,
    /// Forum topic management.
    pub can_manage_topics: bool,
    /// `toggleSupergroupSignMessages` requires it.
    pub can_change_info: bool,
    /// Welcome-message management.
    pub can_send_welcome_messages: bool,
    /// The "Approve new members" toggle.
    pub join_by_request: bool,
    /// Set by `toggleSupergroupIsBroadcastGroup` (one-way upgrade).
    pub is_broadcast_group: bool,
    /// The channel "Sign messages" toggle.
    pub sign_messages: bool,
    /// The "Show authors" toggle.
    pub show_message_sender: bool,
}

impl SupergroupFacts {
    /// The facts one `updateSupergroup` / `getSupergroup` carries. `rights`
    /// are, in order, `can_restrict_members`, `can_invite_users`,
    /// `can_promote_members`, `can_manage_tags`, `can_manage_topics`,
    /// `can_change_info` and `can_send_welcome_messages`; one the update
    /// does not name is lacking.
    pub(crate) fn from_update(
        rights: [Option<bool>; 7],
        join_by_request: bool,
        is_broadcast_group: bool,
        sign_messages: bool,
        show_message_sender: bool,
    ) -> Self {
        let [
            can_restrict_members,
            can_invite_users,
            can_promote_members,
            can_manage_tags,
            can_manage_topics,
            can_change_info,
            can_send_welcome_messages,
        ] = rights.map(|right| right.unwrap_or(false));
        Self {
            can_restrict_members,
            can_invite_users,
            can_promote_members,
            can_manage_tags,
            can_manage_topics,
            can_change_info,
            can_send_welcome_messages,
            join_by_request,
            is_broadcast_group,
            sign_messages,
            show_message_sender,
        }
    }
}

/// The anti-spam state of a supergroup from `supergroupFullInfo` (schema
/// 1.8.67, line 2792).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupergroupAntiSpam {
    /// `has_aggressive_anti_spam_enabled`.
    pub enabled: bool,
    /// `can_toggle_aggressive_anti_spam`: the only gate for the toggle.
    pub can_toggle: bool,
}

impl GroupsState {
    /// What the last `updateSupergroup` of `supergroup_id` said; every right
    /// lacking and every flag off while none was seen.
    pub fn supergroup(&self, supergroup_id: i64) -> SupergroupFacts {
        self.supergroups
            .get(&supergroup_id)
            .copied()
            .unwrap_or_default()
    }

    /// The supergroup's facts are what `facts` says, whole.
    pub(crate) fn set_supergroup_facts(&mut self, supergroup_id: i64, facts: SupergroupFacts) {
        self.supergroups.insert(supergroup_id, facts);
    }

    /// An optimistic edit of one flag (a toggle sent), or the rollback of
    /// one.
    pub(crate) fn edit_supergroup(
        &mut self,
        supergroup_id: i64,
        edit: impl FnOnce(&mut SupergroupFacts),
    ) {
        edit(self.supergroups.entry(supergroup_id).or_default());
    }

    /// The anti-spam state of `supergroup_id`; off and not toggleable while
    /// none was fetched.
    pub fn supergroup_anti_spam(&self, supergroup_id: i64) -> SupergroupAntiSpam {
        self.anti_spam
            .get(&supergroup_id)
            .copied()
            .unwrap_or_default()
    }

    /// The supergroup's anti-spam state is what `anti_spam` says, whole.
    pub(crate) fn set_supergroup_anti_spam(
        &mut self,
        supergroup_id: i64,
        anti_spam: SupergroupAntiSpam,
    ) {
        self.anti_spam.insert(supergroup_id, anti_spam);
    }

    /// An optimistic edit of the anti-spam flag, or the rollback of one.
    pub(crate) fn edit_supergroup_anti_spam(
        &mut self,
        supergroup_id: i64,
        edit: impl FnOnce(&mut SupergroupAntiSpam),
    ) {
        edit(self.anti_spam.entry(supergroup_id).or_default());
    }
}
