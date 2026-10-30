//! The Edit box's manage rows and the counts beside them (tdesktop
//! `Controller::fillManageSection`, `info_profile_values.cpp`): which rows
//! the viewer gets, from the cached chat, full info and fetched lists.
use super::*;
use crate::group_manage::{AutoTranslate, ReactionsValue, Signatures, auto_translate_level};
use crate::permissions_box::permissions_count;

/// One Edit box's manage rows. `None` / false: the row is not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ManageRows {
    /// "Reactions", with its value once the chat reported one.
    pub reactions: Option<Option<ReactionsValue>>,
    /// "Permissions": rows on and rows in the box.
    pub permissions: Option<(usize, usize)>,
    /// "Invite links": the viewer's links.
    pub invite_links: Option<i32>,
    pub administrators: Option<i32>,
    /// "Members" in a group, "Subscribers" in a channel.
    pub members: Option<i32>,
    /// "Join Requests": shown while any are pending.
    pub join_requests: i32,
    /// "Removed users": channels and broadcast groups (a group lists them
    /// in its permissions box).
    pub removed_users: Option<i32>,
    pub recent_actions: bool,
    pub auto_translate: Option<AutoTranslate>,
    pub signatures: Option<Signatures>,
}

impl Session {
    /// The manage rows of `chat_id`'s Edit box, following tdesktop's gates
    /// (`canEditPermissions`, `canHaveInviteLink`, `canViewAdmins`,
    /// `canViewMembers`, `canViewBanned`, `canEditSignatures`,
    /// `canEditAutoTranslate`). Unknown rights hide a row.
    pub fn manage_rows(&self, chat_id: ChatId) -> ManageRows {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return ManageRows::default();
        };
        let status = self.group_own_status(chat_id);
        let owner = self.chat_is_owner(chat_id);
        let admin = owner || status == Some(ChannelMemberStatus::Administrator);
        let member = admin
            || matches!(
                status,
                Some(ChannelMemberStatus::Member | ChannelMemberStatus::Restricted)
            );
        let can_restrict = self.chat_can_restrict_members(chat_id);
        let can_change_info = self.chat_can_change_info(chat_id);
        let pending = self
            .groups
            .pending_join_request_counts
            .get(&chat_id.0)
            .copied()
            .unwrap_or(0);
        let members_total = self
            .group_member_counts(chat)
            .map_or(0, |(members, _)| members);
        let invite_links = self.chat_can_invite_users(chat_id).then(|| {
            match self.groups.invite_links.get(&chat_id.0) {
                Some(InviteLinkFetch::Loaded(list)) => list.total_count,
                _ => 0,
            }
        });
        let reactions = self.group_admin_controls(chat_id).reactions.then(|| {
            self.chat_available_reactions(chat_id)
                .map(ReactionsValue::of)
        });
        let permissions = |is_forum: bool| {
            let defaults = chat.permissions.unwrap_or_else(ChatPermissions::all);
            permissions_count(&defaults, is_forum)
        };
        match chat.kind {
            ChatKind::BasicGroup { .. } => {
                let admins = match self.groups.basic_group_members.get(&chat_id.0) {
                    Some(SupergroupMembersFetch::Loaded { members, .. }) => members
                        .iter()
                        .filter(|m| {
                            matches!(
                                m.status,
                                ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator
                            )
                        })
                        .count()
                        .try_into()
                        .unwrap_or(i32::MAX),
                    _ => 0,
                };
                ManageRows {
                    reactions,
                    permissions: can_restrict.then(|| permissions(false)),
                    invite_links,
                    administrators: member.then_some(admins),
                    members: member.then_some(members_total),
                    join_requests: pending,
                    ..ManageRows::default()
                }
            }
            ChatKind::Supergroup {
                supergroup_id,
                is_channel,
            } => {
                let broadcast_group = self
                    .groups
                    .supergroup_is_broadcast
                    .get(&supergroup_id)
                    .copied()
                    .unwrap_or(false);
                let full = self.supergroup_full_info(supergroup_id);
                let counts = full.map(|info| info.member_counts).unwrap_or_default();
                let hidden = full.is_some_and(|info| info.admin.has_hidden_members);
                let channel_settings = is_channel && can_change_info;
                ManageRows {
                    reactions,
                    permissions: (!is_channel && !broadcast_group && can_restrict)
                        .then(|| permissions(chat.is_forum_chat())),
                    invite_links,
                    administrators: (!is_channel || admin).then_some(counts.administrators),
                    members: (admin || (!is_channel && member && !hidden)).then_some(members_total),
                    join_requests: pending,
                    removed_users: ((is_channel || broadcast_group) && admin && can_restrict)
                        .then_some(counts.banned),
                    recent_actions: self.chat_can_view_event_log(chat_id),
                    auto_translate: channel_settings.then(|| AutoTranslate {
                        on: self.chat_auto_translate(chat_id),
                        level: self
                            .groups
                            .chat_boost_status
                            .get(&chat_id.0)
                            .map(|(level, _)| *level),
                        required: auto_translate_level(
                            self.groups.chat_boost_features.get(&chat_id.0),
                        ),
                    }),
                    signatures: channel_settings.then(|| {
                        Signatures::new(
                            self.groups
                                .supergroup_sign_messages
                                .get(&supergroup_id)
                                .copied()
                                .unwrap_or(false),
                            self.groups
                                .supergroup_show_message_sender
                                .get(&supergroup_id)
                                .copied()
                                .unwrap_or(false),
                        )
                    }),
                }
            }
            _ => ManageRows::default(),
        }
    }
}
