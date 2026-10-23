//! Restricted members as exceptions: look up a member's current restriction
//! from the loaded Restricted list, and write the end date on its row
//! (tdesktop `ParticipantsBoxController`, `Role::Restricted`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use quill::ids::ChatId;
use quill::lang::{keys, tr_with};
use quill::local_time::{civil_local, full_stamp, now_unix};
use quill::moderation_exceptions::restricted_status;
use quill::state::{MemberListFilter, SupergroupMembersFetch};
use quill::telegram::envelope::{MemberRestriction, MessageSender, ParsedChatMember};

impl QuillApp {
    /// The loaded restriction of `user_id` in `chat_id`, if the Restricted
    /// list has been fetched and holds them.
    pub(super) fn restricted_member_entry(
        &self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Option<MemberRestriction> {
        let session = self.session()?;
        let fetch = session
            .groups
            .supergroup_members
            .get(&(chat_id.0, MemberListFilter::Restricted))?;
        let SupergroupMembersFetch::Loaded { members, .. } = fetch else {
            return None;
        };
        members
            .iter()
            .find(|m| m.member_id == MessageSender::User { user_id })
            .and_then(|m| m.restriction)
    }
}

impl QuillApp {
    /// The name of the admin who set an exception or removed a user
    /// (`chatMember.inviter_user_id`), when known.
    pub(super) fn member_set_by(&self, member: &ParsedChatMember) -> Option<String> {
        if member.inviter_user_id == 0 {
            return None;
        }
        self.session()?
            .user(member.inviter_user_id)
            .map(|user| user.display_name())
    }

    /// "Restricted by Dana Levi on 12 March 2026, 21:42." for an existing
    /// exception (tdesktop `EditRestrictedBox`, `lng_rights_chat_restricted_by`).
    pub(super) fn restricted_by_line(&self, chat_id: ChatId, user_id: i64) -> Option<String> {
        let session = self.session()?;
        let fetch = session
            .groups
            .supergroup_members
            .get(&(chat_id.0, MemberListFilter::Restricted))?;
        let SupergroupMembersFetch::Loaded { members, .. } = fetch else {
            return None;
        };
        let member = members
            .iter()
            .find(|m| m.member_id == MessageSender::User { user_id })?;
        let by = self.member_set_by(member)?;
        let date = full_stamp(&civil_local(i64::from(member.joined_chat_date)));
        Some(tr_with(
            keys::RIGHTS_CHAT_RESTRICTED_BY,
            &[("user", &by), ("date", &date)],
        ))
    }
}

/// "restricted until 12 March 2026, 21:42" for a restricted row.
pub(super) fn restricted_row_status(restriction: MemberRestriction) -> String {
    restricted_status(restriction.until_date, now_unix(), |unix| {
        full_stamp(&civil_local(i64::from(unix)))
    })
}
