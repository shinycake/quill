use super::*;
use serde_json::Value;

/// Own `chatMemberStatus*` for a broadcast channel (TDLib 1.8.67).
/// Drives the composer gate (admins post in 2.3) and the join/leave affordance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMemberStatus {
    Creator,
    Administrator,
    Member,
    Restricted,
    Left,
    Banned,
    Unknown,
}

impl ChannelMemberStatus {
    pub fn is_admin(self) -> bool {
        matches!(
            self,
            ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator
        )
    }

    pub fn is_joined(self) -> bool {
        matches!(
            self,
            ChannelMemberStatus::Creator
                | ChannelMemberStatus::Administrator
                | ChannelMemberStatus::Member
        )
    }
}

/// A member's standing in a chat, with what only that standing has
/// (`chatMemberStatus*`, TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberStatus {
    Creator {
        /// The owner posts as the group.
        is_anonymous: bool,
    },
    Administrator {
        /// Phase D3b: the full `rights` block (`chatAdministratorRights`,
        /// schema line 1092); `None` when the block is absent. Drives the
        /// promote/edit-rights flows, the channel-admin composer gate
        /// (`can_post_messages`), the invite-link / join-request gate
        /// (`can_invite_users`, Phase D3a) and the `can_promote_members` gate.
        rights: Option<ChatAdminRights>,
        /// Slice G1: `can_be_edited` (schema 1.8.67, line 2500).
        /// Telegram X refuses ban/restrict/promote against the creator and
        /// non-editable admins (`ProfileController` `YouCantBanX`); the
        /// member dialog mirrors that gate.
        can_be_edited: bool,
    },
    Member,
    /// The member's own rights and when the restriction ends.
    Restricted(MemberRestriction),
    Left,
    Banned,
    Unknown,
}

impl MemberStatus {
    /// The administrator's parsed rights block (`None` for every other
    /// standing, or an absent block).
    pub fn rights(&self) -> Option<ChatAdminRights> {
        match self {
            Self::Administrator { rights, .. } => *rights,
            _ => None,
        }
    }

    /// The standing posts as the group (`rights.is_anonymous` of an
    /// administrator, `is_anonymous` of the owner).
    pub fn is_anonymous(&self) -> bool {
        match self {
            Self::Creator { is_anonymous } => *is_anonymous,
            Self::Administrator { rights, .. } => rights.is_some_and(|rights| rights.is_anonymous),
            _ => false,
        }
    }

    /// The standing without its data.
    pub fn kind(&self) -> ChannelMemberStatus {
        match self {
            Self::Creator { .. } => ChannelMemberStatus::Creator,
            Self::Administrator { .. } => ChannelMemberStatus::Administrator,
            Self::Member => ChannelMemberStatus::Member,
            Self::Restricted(_) => ChannelMemberStatus::Restricted,
            Self::Left => ChannelMemberStatus::Left,
            Self::Banned => ChannelMemberStatus::Banned,
            Self::Unknown => ChannelMemberStatus::Unknown,
        }
    }
}

/// Typed `chatMember` (TDLib 1.8.67). Only `member_id` and `status` are kept
/// besides the tag and who invited them; the administrator's rights and
/// the restriction ride the status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChatMember {
    pub member_id: MessageSender,
    pub standing: MemberStatus,
    /// Slice G1: `chatMember.tag` (schema 1.8.67, line 2526) — the
    /// admin custom title (set via `setChatMemberTag`, line 13598).
    pub tag: String,
    /// `chatMember.inviter_user_id`: who invited, promoted or banned the
    /// member; 0 when unknown. The Exceptions and Removed users lists say
    /// "Restricted by" / "Removed by" this user.
    pub inviter_user_id: i64,
    /// `chatMember.joined_chat_date`: when the member joined, was promoted
    /// or was banned (unix seconds).
    pub joined_chat_date: i32,
}

impl ParsedChatMember {
    /// The standing without its data.
    pub fn status(&self) -> ChannelMemberStatus {
        self.standing.kind()
    }

    /// `rights.can_post_messages` of an administrator with a rights block.
    pub fn admin_can_post_messages(&self) -> Option<bool> {
        self.admin_rights().map(|rights| rights.can_post_messages)
    }

    /// `rights.can_invite_users` of an administrator with a rights block.
    pub fn admin_can_invite_users(&self) -> Option<bool> {
        self.admin_rights().map(|rights| rights.can_invite_users)
    }

    /// The administrator's parsed rights block.
    pub fn admin_rights(&self) -> Option<ChatAdminRights> {
        self.standing.rights()
    }

    /// An administrator the viewer may edit; false for every other status.
    pub fn can_be_edited(&self) -> bool {
        matches!(
            self.standing,
            MemberStatus::Administrator {
                can_be_edited: true,
                ..
            }
        )
    }

    /// `chatMemberStatusRestricted`: the member's own rights and when the
    /// restriction ends.
    pub fn restriction(&self) -> Option<MemberRestriction> {
        match &self.standing {
            MemberStatus::Restricted(restriction) => Some(*restriction),
            _ => None,
        }
    }
}

/// A member's personal restriction (`chatMemberStatusRestricted`, TDLib
/// 1.8.67): `restricted_until_date` (0 means forever) and the rights the
/// member keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberRestriction {
    pub until_date: i32,
    pub permissions: ChatPermissions,
}

/// Phase D3b: `chatAdministratorRights` (TDLib 1.8.67,
/// `schema/td_api.tl:1092`):
/// `chatAdministratorRights can_manage_chat:Bool can_change_info:Bool
/// can_post_messages:Bool can_edit_messages:Bool can_delete_messages:Bool
/// can_invite_users:Bool can_restrict_members:Bool can_pin_messages:Bool
/// can_manage_topics:Bool can_promote_members:Bool
/// can_manage_video_chats:Bool can_post_stories:Bool can_edit_stories:Bool
/// can_delete_stories:Bool can_manage_direct_messages:Bool
/// can_manage_tags:Bool can_send_welcome_messages:Bool is_anonymous:Bool =
/// ChatAdministratorRights;`
/// Fields are declared in schema order. Missing JSON fields parse to
/// `false` (deny-by-default); TDLib always sends the full block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatAdminRights {
    pub can_manage_chat: bool,
    pub can_change_info: bool,
    pub can_post_messages: bool,
    pub can_edit_messages: bool,
    pub can_delete_messages: bool,
    pub can_invite_users: bool,
    pub can_restrict_members: bool,
    pub can_pin_messages: bool,
    pub can_manage_topics: bool,
    pub can_promote_members: bool,
    pub can_manage_video_chats: bool,
    pub can_post_stories: bool,
    pub can_edit_stories: bool,
    pub can_delete_stories: bool,
    pub can_manage_direct_messages: bool,
    pub can_manage_tags: bool,
    pub can_send_welcome_messages: bool,
    pub is_anonymous: bool,
}

impl ChatAdminRights {
    /// All rights granted. A UI convenience for the promote dialog's
    /// default checkbox state — not a server fact.
    pub fn all() -> Self {
        Self {
            can_manage_chat: true,
            can_change_info: true,
            can_post_messages: true,
            can_edit_messages: true,
            can_delete_messages: true,
            can_invite_users: true,
            can_restrict_members: true,
            can_pin_messages: true,
            can_manage_topics: true,
            can_promote_members: true,
            can_manage_video_chats: true,
            can_post_stories: true,
            can_edit_stories: true,
            can_delete_stories: true,
            can_manage_direct_messages: true,
            can_manage_tags: true,
            can_send_welcome_messages: true,
            is_anonymous: true,
        }
    }

    /// Serialize as `chatAdministratorRights` JSON for
    /// `setChatMemberStatus` (schema 1.8.67, lines 2500/1092).
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "@type": "chatAdministratorRights",
            "can_manage_chat": self.can_manage_chat,
            "can_change_info": self.can_change_info,
            "can_post_messages": self.can_post_messages,
            "can_edit_messages": self.can_edit_messages,
            "can_delete_messages": self.can_delete_messages,
            "can_invite_users": self.can_invite_users,
            "can_restrict_members": self.can_restrict_members,
            "can_pin_messages": self.can_pin_messages,
            "can_manage_topics": self.can_manage_topics,
            "can_promote_members": self.can_promote_members,
            "can_manage_video_chats": self.can_manage_video_chats,
            "can_post_stories": self.can_post_stories,
            "can_edit_stories": self.can_edit_stories,
            "can_delete_stories": self.can_delete_stories,
            "can_manage_direct_messages": self.can_manage_direct_messages,
            "can_manage_tags": self.can_manage_tags,
            "can_send_welcome_messages": self.can_send_welcome_messages,
            "is_anonymous": self.is_anonymous,
        })
    }
}

/// Phase D3b: parse a `chatAdministratorRights` block (TDLib 1.8.67,
/// schema line 1092); `None` unless `@type` matches or the value is
/// absent/null.
pub fn parse_chat_admin_rights(value: Option<&Value>) -> Option<ChatAdminRights> {
    let value = value.filter(|v| !v.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("chatAdministratorRights") {
        return None;
    }
    let right = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    Some(ChatAdminRights {
        can_manage_chat: right("can_manage_chat"),
        can_change_info: right("can_change_info"),
        can_post_messages: right("can_post_messages"),
        can_edit_messages: right("can_edit_messages"),
        can_delete_messages: right("can_delete_messages"),
        can_invite_users: right("can_invite_users"),
        can_restrict_members: right("can_restrict_members"),
        can_pin_messages: right("can_pin_messages"),
        can_manage_topics: right("can_manage_topics"),
        can_promote_members: right("can_promote_members"),
        can_manage_video_chats: right("can_manage_video_chats"),
        can_post_stories: right("can_post_stories"),
        can_edit_stories: right("can_edit_stories"),
        can_delete_stories: right("can_delete_stories"),
        can_manage_direct_messages: right("can_manage_direct_messages"),
        can_manage_tags: right("can_manage_tags"),
        can_send_welcome_messages: right("can_send_welcome_messages"),
        is_anonymous: right("is_anonymous"),
    })
}

/// Slice G1: `chatPermissions` (TDLib 1.8.67, `schema/td_api.tl:1070`):
/// `chatPermissions can_send_basic_messages:Bool can_send_audios:Bool
/// can_send_documents:Bool can_send_photos:Bool can_send_videos:Bool
/// can_send_video_notes:Bool can_send_voice_notes:Bool can_send_polls:Bool
/// can_send_other_messages:Bool can_add_link_previews:Bool
/// can_react_to_messages:Bool can_edit_tag:Bool can_change_info:Bool
/// can_invite_users:Bool can_pin_messages:Bool can_create_topics:Bool =
/// ChatPermissions;`
/// Fields are declared in schema order. Missing JSON fields parse to
/// `false` (deny-by-default); TDLib always sends the full block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatPermissions {
    pub can_send_basic_messages: bool,
    pub can_send_audios: bool,
    pub can_send_documents: bool,
    pub can_send_photos: bool,
    pub can_send_videos: bool,
    pub can_send_video_notes: bool,
    pub can_send_voice_notes: bool,
    pub can_send_polls: bool,
    pub can_send_other_messages: bool,
    pub can_add_link_previews: bool,
    pub can_react_to_messages: bool,
    pub can_edit_tag: bool,
    pub can_change_info: bool,
    pub can_invite_users: bool,
    pub can_pin_messages: bool,
    pub can_create_topics: bool,
}

impl ChatPermissions {
    /// All permissions granted. The permissions editor starts from the
    /// chat's current block, not from this.
    pub fn all() -> Self {
        Self {
            can_send_basic_messages: true,
            can_send_audios: true,
            can_send_documents: true,
            can_send_photos: true,
            can_send_videos: true,
            can_send_video_notes: true,
            can_send_voice_notes: true,
            can_send_polls: true,
            can_send_other_messages: true,
            can_add_link_previews: true,
            can_react_to_messages: true,
            can_edit_tag: true,
            can_change_info: true,
            can_invite_users: true,
            can_pin_messages: true,
            can_create_topics: true,
        }
    }

    /// Serialize as `chatPermissions` JSON for `setChatPermissions`
    /// (schema 1.8.67, line 13464).
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "@type": "chatPermissions",
            "can_send_basic_messages": self.can_send_basic_messages,
            "can_send_audios": self.can_send_audios,
            "can_send_documents": self.can_send_documents,
            "can_send_photos": self.can_send_photos,
            "can_send_videos": self.can_send_videos,
            "can_send_video_notes": self.can_send_video_notes,
            "can_send_voice_notes": self.can_send_voice_notes,
            "can_send_polls": self.can_send_polls,
            "can_send_other_messages": self.can_send_other_messages,
            "can_add_link_previews": self.can_add_link_previews,
            "can_react_to_messages": self.can_react_to_messages,
            "can_edit_tag": self.can_edit_tag,
            "can_change_info": self.can_change_info,
            "can_invite_users": self.can_invite_users,
            "can_pin_messages": self.can_pin_messages,
            "can_create_topics": self.can_create_topics,
        })
    }
}

/// Slice G1: parse a `chatPermissions` block (TDLib 1.8.67, schema line
/// 1070); `None` unless `@type` matches or the value is absent/null.
pub fn parse_chat_permissions(value: Option<&Value>) -> Option<ChatPermissions> {
    let value = value.filter(|v| !v.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("chatPermissions") {
        return None;
    }
    let perm = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    Some(ChatPermissions {
        can_send_basic_messages: perm("can_send_basic_messages"),
        can_send_audios: perm("can_send_audios"),
        can_send_documents: perm("can_send_documents"),
        can_send_photos: perm("can_send_photos"),
        can_send_videos: perm("can_send_videos"),
        can_send_video_notes: perm("can_send_video_notes"),
        can_send_voice_notes: perm("can_send_voice_notes"),
        can_send_polls: perm("can_send_polls"),
        can_send_other_messages: perm("can_send_other_messages"),
        can_add_link_previews: perm("can_add_link_previews"),
        can_react_to_messages: perm("can_react_to_messages"),
        can_edit_tag: perm("can_edit_tag"),
        can_change_info: perm("can_change_info"),
        can_invite_users: perm("can_invite_users"),
        can_pin_messages: perm("can_pin_messages"),
        can_create_topics: perm("can_create_topics"),
    })
}

/// Phase D3b: `chatAdministrator` (TDLib 1.8.67, `schema/td_api.tl:2482`):
/// `chatAdministrator user_id:int53 custom_title:string is_owner:Bool
/// can_be_edited:Bool = ChatAdministrator;`
/// One entry of the `chatAdministrators` response. This schema version has
/// no rights block here (and no `setChatAdministratorCustomTitle`), so
/// per-admin rights come from `getChatMember` on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAdministratorEntry {
    pub user_id: i64,
    pub custom_title: String,
    pub is_owner: bool,
    pub can_be_edited: bool,
}

/// `chatMemberStatus*` (TDLib 1.8.67). Unknown constructors map to `Unknown`;
/// the field itself stays required. Also returns `rights.can_post_messages`
/// (`Some`) when the status is `chatMemberStatusAdministrator` and the
/// `rights` block parses; `None` otherwise.
pub(crate) fn parse_channel_member_status(
    value: Option<&Value>,
) -> Option<(ChannelMemberStatus, Option<bool>)> {
    let value = value?;
    let status = match value.get("@type").and_then(Value::as_str) {
        Some("chatMemberStatusCreator") => ChannelMemberStatus::Creator,
        Some("chatMemberStatusAdministrator") => ChannelMemberStatus::Administrator,
        Some("chatMemberStatusMember") => ChannelMemberStatus::Member,
        Some("chatMemberStatusRestricted") => ChannelMemberStatus::Restricted,
        Some("chatMemberStatusLeft") => ChannelMemberStatus::Left,
        Some("chatMemberStatusBanned") => ChannelMemberStatus::Banned,
        _ => ChannelMemberStatus::Unknown,
    };
    let admin_can_post_messages = if status == ChannelMemberStatus::Administrator {
        value
            .get("rights")
            .and_then(|rights| rights.get("can_post_messages"))
            .and_then(Value::as_bool)
    } else {
        None
    };
    Some((status, admin_can_post_messages))
}

/// `chatMember` (TDLib 1.8.67). Returns `None` when `member_id` or `status`
/// is missing or unparseable.
pub(crate) fn parse_chat_member(value: Option<&Value>) -> Option<ParsedChatMember> {
    let value = value.filter(|v| !v.is_null())?;
    let member_id = parse_message_sender(value.get("member_id")).ok()?;
    let standing = parse_member_status(value.get("status"))?;
    Some(ParsedChatMember {
        member_id,
        standing,
        tag: value
            .get("tag")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        inviter_user_id: value
            .get("inviter_user_id")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        joined_chat_date: value
            .get("joined_chat_date")
            .and_then(Value::as_i64)
            .and_then(|date| i32::try_from(date).ok())
            .unwrap_or(0),
    })
}

/// A `chatMemberStatus*` value with its data.
pub(crate) fn parse_member_status(value: Option<&Value>) -> Option<MemberStatus> {
    let value = value?;
    Some(match value.get("@type").and_then(Value::as_str) {
        Some("chatMemberStatusCreator") => MemberStatus::Creator {
            is_anonymous: value
                .get("is_anonymous")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        Some("chatMemberStatusAdministrator") => MemberStatus::Administrator {
            rights: parse_chat_admin_rights(value.get("rights")),
            can_be_edited: value
                .get("can_be_edited")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        Some("chatMemberStatusMember") => MemberStatus::Member,
        Some("chatMemberStatusRestricted") => {
            MemberStatus::Restricted(parse_member_restriction(Some(value))?)
        }
        Some("chatMemberStatusLeft") => MemberStatus::Left,
        Some("chatMemberStatusBanned") => MemberStatus::Banned,
        _ => MemberStatus::Unknown,
    })
}

/// `chatMemberStatusRestricted` (schema 1.8.67: `is_member:Bool
/// restricted_until_date:int32 permissions:chatPermissions`); `None` for
/// any other status. A restricted status without a rights block denies
/// everything, as TDLib does for a missing block.
pub(crate) fn parse_member_restriction(value: Option<&Value>) -> Option<MemberRestriction> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatMemberStatusRestricted") {
        return None;
    }
    Some(MemberRestriction {
        until_date: value
            .get("restricted_until_date")
            .and_then(Value::as_i64)
            .map(|date| date.clamp(0, i64::from(i32::MAX)) as i32)
            .unwrap_or(0),
        permissions: parse_chat_permissions(value.get("permissions")).unwrap_or_default(),
    })
}

/// Phase D3b: `chatAdministrator` (TDLib 1.8.67, `schema/td_api.tl:2482`).
pub(crate) fn parse_chat_administrator(value: Option<&Value>) -> Option<ChatAdministratorEntry> {
    let value = value.filter(|v| !v.is_null())?;
    Some(ChatAdministratorEntry {
        user_id: int53(value.get("user_id")).ok()?,

        custom_title: value
            .get("custom_title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        is_owner: value
            .get("is_owner")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_be_edited: value
            .get("can_be_edited")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// `canTransferOwnershipResult*` (TDLib 1.8.67, `schema/td_api.tl:8568`):
/// whether this session may transfer a chat's ownership. The `retry_after`
/// values are seconds until the check can pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanTransferOwnershipResult {
    Ok,
    /// The account has no 2-step verification password.
    PasswordNeeded,
    /// The password was set less than 7 days ago.
    PasswordTooFresh {
        retry_after: i32,
    },
    /// This session logged in less than 24 hours ago.
    SessionTooFresh {
        retry_after: i32,
    },
}

pub(crate) fn parse_can_transfer_ownership_result(
    value: &Value,
) -> Option<CanTransferOwnershipResult> {
    let retry_after = || {
        value
            .get("retry_after")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, i64::from(i32::MAX)) as i32
    };
    match value.get("@type").and_then(Value::as_str)? {
        "canTransferOwnershipResultOk" => Some(CanTransferOwnershipResult::Ok),
        "canTransferOwnershipResultPasswordNeeded" => {
            Some(CanTransferOwnershipResult::PasswordNeeded)
        }
        "canTransferOwnershipResultPasswordTooFresh" => {
            Some(CanTransferOwnershipResult::PasswordTooFresh {
                retry_after: retry_after(),
            })
        }
        "canTransferOwnershipResultSessionTooFresh" => {
            Some(CanTransferOwnershipResult::SessionTooFresh {
                retry_after: retry_after(),
            })
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SupergroupMemberCounts {
    pub administrators: i32,
    pub restricted: i32,
    pub banned: i32,
}

impl SupergroupMemberCounts {
    pub(crate) fn parse(info: Option<&Value>) -> Self {
        let count = |name: &str| {
            info.and_then(|info| info.get(name))
                .and_then(Value::as_i64)
                .map_or(0, |n| i32::try_from(n.max(0)).unwrap_or(i32::MAX))
        };
        Self {
            administrators: count("administrator_count"),
            restricted: count("restricted_count"),
            banned: count("banned_count"),
        }
    }
}

#[cfg(test)]
#[path = "composer_hint_tests.rs"]
mod composer_hint_tests;
