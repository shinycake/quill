//! The `supergroup` answer (`getSupergroup`): the viewer's own rights and
//! the supergroup's flags.
use crate::state::*;
use crate::telegram::envelope::GroupsPayload;

impl Session {
    pub(super) fn apply_supergroup_answer(
        &mut self,
        payload: GroupsPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        let GroupsPayload::Supergroup {
            supergroup_id,
            is_forum,
            has_forum_tabs,
            has_forum_sidebar,
            has_automatic_translation,
            username,
            status,
            can_restrict_members,
            can_invite_users,
            can_promote_members,
            can_manage_tags,
            can_manage_topics,
            can_change_info,
            can_send_welcome_messages,
            can_manage_video_chats,
            is_anonymous,
            join_by_request,
            is_broadcast_group,
            sign_messages,
            show_message_sender,
            join_to_send_messages,
            paid_message_star_count,
            direct_messages,
        } = payload
        else {
            return;
        };
        self.set_direct_messages_role(supergroup_id, direct_messages);
        // Same as the other rights: only the `getSupergroup`
        // answer is trusted for the viewer's own status.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSupergroup) {
            self.groups
                .supergroup_manage_video_chats_right
                .insert(supergroup_id, can_manage_video_chats.unwrap_or(false));
            self.groups
                .supergroup_anonymous
                .insert(supergroup_id, is_anonymous);
        }
        self.payments
            .paid_messages
            .dm_price
            .insert(supergroup_id, paid_message_star_count);
        self.groups
            .supergroup_join_to_send
            .insert(supergroup_id, join_to_send_messages);
        self.set_supergroup_forum_tabs(supergroup_id, has_forum_tabs);
        self.set_supergroup_forum_sidebar(supergroup_id, has_forum_sidebar);
        self.set_supergroup_auto_translate(supergroup_id, has_automatic_translation);
        self.apply_supergroup(
            supergroup_id,
            is_forum,
            username,
            status,
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
            pending,
            extra,
            seq,
        );
    }
}
