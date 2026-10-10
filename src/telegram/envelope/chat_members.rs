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

/// Typed `chatMember` (TDLib 1.8.67). Only `member_id` and `status` are kept;
/// `tag` / `inviter_user_id` / `joined_chat_date` stay out of this slice.
/// `admin_can_post_messages` carries `rights.can_post_messages` from
/// `chatMemberStatusAdministrator` (schema 1.8.67:
/// `chatAdministratorRights ... can_post_messages:Bool ...`), driving the
/// channel-admin composer gate; `None` for every other status or when the
/// rights block is absent. `admin_can_invite_users` carries
/// `rights.can_invite_users` (schema 1.8.67, line 1092), driving the
/// Phase D3a invite-link / join-request management gate; `None` for every
/// other status or when the rights block is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChatMember {
    pub member_id: MessageSender,
    pub status: ChannelMemberStatus,
    pub admin_can_post_messages: Option<bool>,
    pub admin_can_invite_users: Option<bool>,
    /// Phase D3b: the full `rights` block from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only for an
    /// administrator with a parsed rights block; `None` for every other
    /// status or an absent rights block. Drives the promote/edit-rights
    /// flows and the `can_promote_members` gate.
    pub admin_rights: Option<ChatAdminRights>,
    /// Slice G1: `chatMember.tag` (schema 1.8.67, line 2526) — the
    /// admin custom title (set via `setChatMemberTag`, line 13598).
    pub tag: String,
    /// Slice G1: `can_be_edited` from `chatMemberStatusAdministrator`
    /// (schema 1.8.67, line 2500). False for every other status.
    /// Telegram X refuses ban/restrict/promote against the creator and
    /// non-editable admins (`ProfileController` `YouCantBanX`); the
    /// member dialog mirrors that gate.
    pub can_be_edited: bool,
    /// `chatMemberStatusRestricted`: the member's own rights and when the
    /// restriction ends. `None` for every other status.
    pub restriction: Option<MemberRestriction>,
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

/// `rights.can_restrict_members` from a `chatMemberStatusAdministrator`
/// block (TDLib 1.8.67, lines 2500/1092); `None` for any other status or a
/// missing/absent rights block. `setChatSlowModeDelay` requires this
/// right (schema line 13551).
pub(crate) fn parse_restrict_members_right(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatMemberStatusAdministrator") {
        return None;
    }
    value
        .get("rights")
        .and_then(|rights| rights.get("can_restrict_members"))
        .and_then(Value::as_bool)
}

/// Phase D3a: `rights.can_invite_users` from a
/// `chatMemberStatusAdministrator` block (TDLib 1.8.67,
/// `chatAdministratorRights`, schema line 1092); `None` for any other
/// status or a missing/absent rights block. Managing invite links and
/// processing join requests requires this right (or creator status).
pub(crate) fn parse_invite_users_right(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatMemberStatusAdministrator") {
        return None;
    }
    value
        .get("rights")
        .and_then(|rights| rights.get("can_invite_users"))
        .and_then(Value::as_bool)
}

/// Phase D3b: `rights.can_promote_members` from a
/// `chatMemberStatusAdministrator` block (TDLib 1.8.67,
/// `chatAdministratorRights`, schema line 1092); `None` for any other
/// status or a missing/absent rights block. Promoting/demoting members
/// and editing admin rights requires this right (or creator status).
pub(crate) fn parse_promote_members_right(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatMemberStatusAdministrator") {
        return None;
    }
    value
        .get("rights")
        .and_then(|rights| rights.get("can_promote_members"))
        .and_then(Value::as_bool)
}

/// Slice G1: `rights.can_manage_tags` from a
/// `chatMemberStatusAdministrator` block (TDLib 1.8.67,
/// `chatAdministratorRights`, schema line 1092); `None` for any other
/// status or a missing/absent rights block. Changing another member's
/// custom title (`setChatMemberTag`, line 13598) requires this right
/// (or creator status).
pub(crate) fn parse_manage_tags_right(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatMemberStatusAdministrator") {
        return None;
    }
    value
        .get("rights")
        .and_then(|rights| rights.get("can_manage_tags"))
        .and_then(Value::as_bool)
}

/// Slice G2: `rights.can_manage_topics` from own
/// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092); `None`
/// for any other status or a missing rights block. Forum topic
/// management requires this right (or creator status).
pub(crate) fn parse_manage_topics_right(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatMemberStatusAdministrator") {
        return None;
    }
    value
        .get("rights")
        .and_then(|rights| rights.get("can_manage_topics"))
        .and_then(Value::as_bool)
}

/// Slice G2: `rights.can_change_info` from own
/// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092); `None`
/// for any other status or a missing rights block.
/// `toggleSupergroupSignMessages` requires this right (schema 1.8.67,
/// line 15175).
pub(crate) fn parse_change_info_right(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatMemberStatusAdministrator") {
        return None;
    }
    value
        .get("rights")
        .and_then(|rights| rights.get("can_change_info"))
        .and_then(Value::as_bool)
}

/// Slice G2: `rights.can_send_welcome_messages` from own
/// `chatMemberStatusAdministrator` (schema 1.8.67, line 1090); `None`
/// for any other status or a missing rights block. Welcome-message
/// management requires this right (or creator status).
pub(crate) fn parse_send_welcome_messages_right(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatMemberStatusAdministrator") {
        return None;
    }
    value
        .get("rights")
        .and_then(|rights| rights.get("can_send_welcome_messages"))
        .and_then(Value::as_bool)
}

/// `chatMember` (TDLib 1.8.67). Returns `None` when `member_id` or `status`
/// is missing or unparseable.
pub(crate) fn parse_chat_member(value: Option<&Value>) -> Option<ParsedChatMember> {
    let value = value.filter(|v| !v.is_null())?;
    let member_id = parse_message_sender(value.get("member_id")).ok()?;
    let (status, admin_can_post_messages) = parse_channel_member_status(value.get("status"))?;
    let admin_rights = if status == ChannelMemberStatus::Administrator {
        parse_chat_admin_rights(value.get("status").and_then(|s| s.get("rights")))
    } else {
        None
    };
    Some(ParsedChatMember {
        member_id,
        status,
        admin_can_post_messages,
        admin_can_invite_users: parse_invite_users_right(value.get("status")),
        admin_rights,
        tag: value
            .get("tag")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        can_be_edited: value
            .get("status")
            .and_then(|status| status.get("can_be_edited"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        restriction: parse_member_restriction(value.get("status")),
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
