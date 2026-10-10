//! Adding a bot to a group or channel, after tdesktop's
//! `AddBotToGroupBoxController` (boxes/peers/add_bot_to_chat_box.cpp) and
//! `InviteToChatButton` / `InviteToChatAbout` (info_profile_values.cpp).
//!
//! The profile button and its wording depend on what the bot accepts. The
//! chat picker lists the chats where the person may add admins (the bot then
//! asks for its default rights, which the person can trim) and, when the bot
//! can join groups, the groups where members may be added.

use crate::telegram::envelope::ChatAdminRights;

/// What a bot accepts, from `userTypeBot.can_join_groups` and
/// `botInfo.default_{group,channel}_administrator_rights`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotFacts {
    pub can_join_groups: bool,
    pub group_rights: Option<ChatAdminRights>,
    pub channel_rights: Option<ChatAdminRights>,
}

/// The profile action's label (`lng_profile_*`), or `None` when the bot
/// cannot be added anywhere.
pub fn invite_label(bot: &BotFacts) -> Option<&'static str> {
    match (bot.can_join_groups, bot.channel_rights.is_some()) {
        (false, true) => Some("Add to channel"),
        (false, false) => None,
        (true, true) => Some("Add to group or channel"),
        (true, false) => Some("Add to group"),
    }
}

/// The line under the action (`lng_profile_*_about`).
pub fn invite_about(bot: &BotFacts) -> Option<&'static str> {
    let channel = bot.channel_rights.is_some();
    if !bot.can_join_groups || bot.group_rights.is_none() {
        return channel.then_some("This bot can manage a channel.");
    }
    Some(if channel {
        "This bot can manage a group or channel."
    } else {
        "This bot can manage a group."
    })
}

/// A chat as the picker sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFacts {
    pub chat_id: i64,
    pub title: String,
    pub is_channel: bool,
    /// The person may promote administrators here.
    pub can_add_admins: bool,
    /// The person may invite members here.
    pub can_add_members: bool,
}

/// Which chats a request is about (tdesktop `AddBotToGroupBoxController::Scope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// The profile button, or `?startgroup`: groups as admin or member,
    /// channels as admin.
    #[default]
    All,
    /// `?startgroup=...&admin=...`: groups where the person can add admins.
    GroupAdmin,
    /// `?startchannel&admin=...`: channels where the person can add admins.
    ChannelAdmin,
}

/// One request to add a bot: where it can go, which rights a link asked
/// for, and the `/start` payload a link carried.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Invite {
    pub scope: Scope,
    /// Rights named by the link (`admin=`); they replace the bot's own
    /// defaults when present.
    pub requested_rights: Option<ChatAdminRights>,
    /// `startgroup=<payload>`: sent as the bot's start message instead of
    /// adding the bot as a plain member.
    pub start_parameter: String,
}

/// How the bot would join a chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// As an administrator with these requested rights.
    Admin(ChatAdminRights),
    /// As a plain member.
    Member,
}

/// How `bot` can be added to `chat` for this request, if at all (tdesktop
/// `needToCreateRow` and `addBotToGroup`). Channels take only admins.
pub fn plan_for(bot: &BotFacts, invite: &Invite, chat: &ChatFacts) -> Option<Plan> {
    let admin_rights = |own: Option<ChatAdminRights>| {
        invite
            .requested_rights
            .filter(|_| invite.scope != Scope::All)
            .or(own)
            .unwrap_or_default()
    };
    if chat.is_channel {
        let wants_admin = match invite.scope {
            Scope::ChannelAdmin => true,
            Scope::All => bot.channel_rights.is_some(),
            Scope::GroupAdmin => false,
        };
        return (wants_admin && chat.can_add_admins)
            .then(|| Plan::Admin(admin_rights(bot.channel_rights)));
    }
    let wants_admin = match invite.scope {
        Scope::GroupAdmin => true,
        Scope::All => bot.group_rights.is_some(),
        Scope::ChannelAdmin => false,
    };
    if wants_admin && chat.can_add_admins {
        return Some(Plan::Admin(admin_rights(bot.group_rights)));
    }
    (invite.scope == Scope::All && bot.can_join_groups && chat.can_add_members)
        .then_some(Plan::Member)
}

/// One picker row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub chat_id: i64,
    pub title: String,
    pub is_channel: bool,
    pub plan: Plan,
}

/// The picker rows, in the given chat order: chats the bot joins as an
/// administrator first ("groups and channels I manage"), then the groups it
/// joins as a member.
pub fn choices(bot: &BotFacts, invite: &Invite, chats: &[ChatFacts]) -> Vec<Choice> {
    let mut admin = Vec::new();
    let mut member = Vec::new();
    for chat in chats {
        let Some(plan) = plan_for(bot, invite, chat) else {
            continue;
        };
        let row = Choice {
            chat_id: chat.chat_id,
            title: chat.title.clone(),
            is_channel: chat.is_channel,
            plan,
        };
        match row.plan {
            Plan::Admin(_) => admin.push(row),
            Plan::Member => member.push(row),
        }
    }
    admin.extend(member);
    admin
}

/// Rights indexes (see `ui::groups::ADMIN_RIGHT_LABELS`) that mean something
/// in a group, or in a channel.
pub fn relevant_rights(is_channel: bool) -> &'static [usize] {
    if is_channel {
        // Manage chat, info, post, edit, delete, invite, promote, video
        // chats, stories, direct messages.
        &[0, 1, 2, 3, 4, 5, 9, 10, 11, 12, 13, 14]
    } else {
        // Manage chat, info, delete, invite, restrict, pin, topics,
        // promote, video chats, tags, anonymous.
        &[0, 1, 4, 5, 6, 7, 8, 9, 10, 15, 17]
    }
}

/// Drop the rights the chat type has no use for, so a group never receives
/// "post messages" and a channel never receives "restrict members".
pub fn mask_rights(rights: ChatAdminRights, is_channel: bool) -> ChatAdminRights {
    let mut out = ChatAdminRights {
        can_manage_chat: rights.can_manage_chat,
        can_change_info: rights.can_change_info,
        can_delete_messages: rights.can_delete_messages,
        can_invite_users: rights.can_invite_users,
        can_promote_members: rights.can_promote_members,
        can_manage_video_chats: rights.can_manage_video_chats,
        ..Default::default()
    };
    if is_channel {
        out.can_post_messages = rights.can_post_messages;
        out.can_edit_messages = rights.can_edit_messages;
        out.can_post_stories = rights.can_post_stories;
        out.can_edit_stories = rights.can_edit_stories;
        out.can_delete_stories = rights.can_delete_stories;
        out.can_manage_direct_messages = rights.can_manage_direct_messages;
    } else {
        out.can_restrict_members = rights.can_restrict_members;
        out.can_pin_messages = rights.can_pin_messages;
        out.can_manage_topics = rights.can_manage_topics;
        out.can_manage_tags = rights.can_manage_tags;
        out.is_anonymous = rights.is_anonymous;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rights(manage: bool) -> ChatAdminRights {
        ChatAdminRights {
            can_manage_chat: manage,
            can_delete_messages: true,
            ..Default::default()
        }
    }

    fn bot(join: bool, group: bool, channel: bool) -> BotFacts {
        BotFacts {
            can_join_groups: join,
            group_rights: group.then(|| rights(true)),
            channel_rights: channel.then(|| rights(true)),
        }
    }

    fn chat(id: i64, is_channel: bool, admins: bool, members: bool) -> ChatFacts {
        ChatFacts {
            chat_id: id,
            title: format!("Chat {id}"),
            is_channel,
            can_add_admins: admins,
            can_add_members: members,
        }
    }

    fn all() -> Invite {
        Invite::default()
    }

    #[test]
    fn labels_follow_what_the_bot_accepts() {
        assert_eq!(invite_label(&bot(true, false, false)), Some("Add to group"));
        assert_eq!(
            invite_label(&bot(true, true, true)),
            Some("Add to group or channel")
        );
        assert_eq!(
            invite_label(&bot(false, false, true)),
            Some("Add to channel")
        );
        assert_eq!(invite_label(&bot(false, true, false)), None);
    }

    #[test]
    fn the_about_line_needs_something_to_manage() {
        assert_eq!(invite_about(&bot(true, false, false)), None);
        assert_eq!(
            invite_about(&bot(true, true, false)),
            Some("This bot can manage a group.")
        );
        assert_eq!(
            invite_about(&bot(true, true, true)),
            Some("This bot can manage a group or channel.")
        );
        assert_eq!(
            invite_about(&bot(false, true, true)),
            Some("This bot can manage a channel.")
        );
    }

    #[test]
    fn channels_take_admins_only() {
        let b = bot(true, true, true);
        assert!(matches!(
            plan_for(&b, &all(), &chat(1, true, true, true)),
            Some(Plan::Admin(_))
        ));
        assert_eq!(plan_for(&b, &all(), &chat(1, true, false, true)), None);
        assert_eq!(
            plan_for(&bot(true, true, false), &all(), &chat(1, true, true, true)),
            None
        );
    }

    #[test]
    fn groups_prefer_admin_then_fall_back_to_member() {
        let b = bot(true, true, false);
        assert!(matches!(
            plan_for(&b, &all(), &chat(1, false, true, true)),
            Some(Plan::Admin(_))
        ));
        assert_eq!(
            plan_for(&b, &all(), &chat(1, false, false, true)),
            Some(Plan::Member)
        );
        assert_eq!(plan_for(&b, &all(), &chat(1, false, false, false)), None);
    }

    #[test]
    fn a_bot_that_cannot_join_groups_is_not_added_as_a_member() {
        let b = bot(false, false, true);
        assert_eq!(plan_for(&b, &all(), &chat(1, false, false, true)), None);
    }

    #[test]
    fn admin_rows_come_before_member_rows() {
        let b = bot(true, true, true);
        let rows = choices(
            &b,
            &all(),
            &[
                chat(1, false, false, true),
                chat(2, true, true, false),
                chat(3, false, true, true),
                chat(4, false, false, false),
            ],
        );
        let ids: Vec<i64> = rows.iter().map(|row| row.chat_id).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn a_group_admin_link_lists_only_groups_you_can_promote_in() {
        let b = bot(true, false, true);
        let invite = Invite {
            scope: Scope::GroupAdmin,
            requested_rights: Some(rights(false)),
            start_parameter: "x".into(),
        };
        let rows = choices(
            &b,
            &invite,
            &[
                chat(1, false, true, true),
                chat(2, false, false, true),
                chat(3, true, true, true),
            ],
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].chat_id, 1);
        // The link's rights win over the bot's own defaults.
        assert_eq!(rows[0].plan, Plan::Admin(rights(false)));
    }

    #[test]
    fn a_channel_link_lists_only_channels() {
        let b = bot(true, true, true);
        let invite = Invite {
            scope: Scope::ChannelAdmin,
            ..Default::default()
        };
        let rows = choices(
            &b,
            &invite,
            &[chat(1, false, true, true), chat(3, true, true, true)],
        );
        let ids: Vec<i64> = rows.iter().map(|row| row.chat_id).collect();
        assert_eq!(ids, vec![3]);
    }

    #[test]
    fn masking_keeps_only_rights_the_chat_type_has() {
        let all = ChatAdminRights::all();
        let group = mask_rights(all, false);
        assert!(group.can_restrict_members && group.can_pin_messages);
        assert!(!group.can_post_messages && !group.can_edit_messages);
        let channel = mask_rights(all, true);
        assert!(channel.can_post_messages && channel.can_edit_messages);
        assert!(!channel.can_restrict_members && !channel.can_pin_messages);
        assert!(!channel.is_anonymous);
    }
}
