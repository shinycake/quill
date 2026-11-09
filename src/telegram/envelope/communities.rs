use super::*;
use serde_json::Value;

/// Slice (communities backend core): `community` (TDLib 1.8.68,
/// `schema/td_api.tl:2612`):
/// `community id:int53 have_access:Bool name:string photo:chatPhotoInfo date:int32 status:CommunityMemberStatus permissions:communityPermissions = Community;`
/// `photo` (chatPhotoInfo) is not parsed (no consumer yet). `status` and
/// `permissions` are flattened into the rights the TDLib 1.8.68
/// management methods check (`setCommunityPermissions`,
/// `deleteCommunity`; docs/decisions/codex-tdlib-1.8.68.md).
// ponytail: flat scalar subset; nested photo object when a consumer needs it.

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCommunity {
    pub id: i64,
    /// Schema line 2299: when false the community is inaccessible and
    /// "Identifier of the community can't be passed to any method".
    pub have_access: bool,
    pub name: String,
    pub date: i32,
    /// `status` is `communityMemberStatusCreator` — the only status
    /// `deleteCommunity` accepts ("requires owner privileges").
    pub is_owner: bool,
    /// The owner, or an administrator whose
    /// `communityAdministratorRights.can_change_info` is set
    /// (`setCommunityName` / `setCommunityPhoto`).
    pub can_change_info: bool,
    /// The owner, or an administrator whose
    /// `communityAdministratorRights.can_ban_members` is set — the right
    /// `setCommunityPermissions` requires.
    pub can_ban_members: bool,
    /// `permissions.can_edit_chat_list` — regular members may change the
    /// chats added to the community.
    pub members_can_edit_chat_list: bool,
    /// The owner, or an administrator whose
    /// `communityAdministratorRights.can_edit_chat_list` is set: adds
    /// chats directly and handles the add-chat requests (TDLib 1.8.70
    /// `addCommunityChat`, `processCommunityAddChatRequest`).
    pub can_edit_chat_list: bool,
    /// TDLib 1.8.70 `are_chats_collapsed`: every chat of the community
    /// shows as one entry in the chat list (tdesktop "Show as One Chat").
    pub are_chats_collapsed: bool,
}

/// Slice (communities backend core): `communityChat` (TDLib 1.8.68,
/// `schema/td_api.tl:2618`):
/// `communityChat chat_id:int53 can_view_history:Bool is_hidden:Bool = CommunityChat;`
/// `is_hidden` is read-only — there is NO schema method to toggle it
/// (concept-level scan; 265/267/268 stay BLOCKED).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCommunityChat {
    pub chat_id: i64,
    pub can_view_history: bool,
    pub is_hidden: bool,
}

/// Slice (communities backend core): `communityFullInfo` (TDLib 1.8.68,
/// `schema/td_api.tl:2626`; also the direct `getCommunityFullInfo` answer):
/// `communityFullInfo photo:chatPhoto chats:vector<communityChat> administrator_count:int32 banned_count:int32 add_chat_request_count:int32 = CommunityFullInfo;`
/// `photo` intentionally skipped (no consumer yet; see ParsedCommunity note).
// ponytail: flat scalar subset; nested chatPhoto when a consumer needs it.

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCommunityFullInfo {
    pub chats: Vec<ParsedCommunityChat>,
    pub administrator_count: i32,
    pub banned_count: i32,
    pub add_chat_request_count: i32,
    /// TDLib 1.8.70: the community's own notification settings; its
    /// chats fall back to them before the scope defaults.
    pub notification_settings: ChatNotificationSettings,
}

pub(crate) fn parse_community(value: &Value) -> Option<ParsedCommunity> {
    if value.get("@type").and_then(Value::as_str) != Some("community") {
        return None;
    }
    let status = value.get("status");
    let status_type = status
        .and_then(|s| s.get("@type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let is_owner = status_type == "communityMemberStatusCreator";
    let admin_right = |right: &str| {
        status_type == "communityMemberStatusAdministrator"
            && status
                .and_then(|s| s.get("rights"))
                .and_then(|r| r.get(right))
                .and_then(Value::as_bool)
                .unwrap_or(false)
    };
    Some(ParsedCommunity {
        id: int53(value.get("id")).ok()?,
        have_access: value
            .get("have_access")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        date: int53(value.get("date")).ok().unwrap_or(0).sat_i32(),
        is_owner,
        can_change_info: is_owner || admin_right("can_change_info"),
        can_ban_members: is_owner || admin_right("can_ban_members"),
        members_can_edit_chat_list: value
            .get("permissions")
            .and_then(|p| p.get("can_edit_chat_list"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_edit_chat_list: is_owner || admin_right("can_edit_chat_list"),
        are_chats_collapsed: value
            .get("are_chats_collapsed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) fn parse_community_chat(value: &Value) -> Option<ParsedCommunityChat> {
    if value.get("@type").and_then(Value::as_str) != Some("communityChat") {
        return None;
    }
    Some(ParsedCommunityChat {
        chat_id: int53(value.get("chat_id")).ok()?,
        can_view_history: value
            .get("can_view_history")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_hidden: value
            .get("is_hidden")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) fn parse_community_full_info(value: &Value) -> Option<ParsedCommunityFullInfo> {
    if value.get("@type").and_then(Value::as_str) != Some("communityFullInfo") {
        return None;
    }
    Some(ParsedCommunityFullInfo {
        chats: value
            .get("chats")
            .and_then(Value::as_array)
            .map(|chats| chats.iter().filter_map(parse_community_chat).collect())
            .unwrap_or_default(),
        administrator_count: int53(value.get("administrator_count"))
            .ok()
            .unwrap_or(0)
            .sat_i32(),
        banned_count: int53(value.get("banned_count")).ok().unwrap_or(0).sat_i32(),
        add_chat_request_count: int53(value.get("add_chat_request_count"))
            .ok()
            .unwrap_or(0)
            .sat_i32(),
        notification_settings: parse_chat_notification_settings(value.get("notification_settings")),
    })
}

/// TDLib 1.8.70 `communityAddChatRequest chat_id:int53 is_hidden:Bool
/// sender_user_id:int53 date:int32`: a chat someone suggested for the
/// community, waiting for an administrator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityAddChatRequest {
    pub chat_id: i64,
    /// The chat will be visible only to community administrators.
    pub is_hidden: bool,
    pub sender_user_id: i64,
    pub date: i32,
}

/// TDLib 1.8.70 `communityAddChatRequests total_count:int32
/// requests:vector<communityAddChatRequest> next_offset:string`; an
/// empty `next_offset` ends the list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunityAddChatRequests {
    pub total_count: i32,
    pub requests: Vec<CommunityAddChatRequest>,
    pub next_offset: String,
}

/// TDLib 1.8.70 `communityMemberChats owned_chat_ids:vector<int53>
/// joined_chat_ids:vector<int53>`: the chats of a community a member owns
/// (they leave the community when the member is banned) and joined.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunityMemberChats {
    pub owned_chat_ids: Vec<i64>,
    pub joined_chat_ids: Vec<i64>,
}

fn int53_list(value: Option<&Value>) -> Vec<i64> {
    value
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(|id| int53(Some(id)).ok()).collect())
        .unwrap_or_default()
}

/// `communities community_ids:vector<int53>` (`getJoinedCommunities`).
pub(crate) fn parse_community_ids(value: &Value) -> Vec<i64> {
    int53_list(value.get("community_ids"))
}

pub(crate) fn parse_community_add_chat_requests(value: &Value) -> CommunityAddChatRequests {
    CommunityAddChatRequests {
        total_count: int53(value.get("total_count")).unwrap_or(0).sat_i32(),
        requests: value
            .get("requests")
            .and_then(Value::as_array)
            .map(|requests| {
                requests
                    .iter()
                    .filter_map(|request| {
                        Some(CommunityAddChatRequest {
                            chat_id: int53(request.get("chat_id")).ok()?,
                            is_hidden: request
                                .get("is_hidden")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            sender_user_id: int53_or_zero(request.get("sender_user_id")),
                            date: int53_or_zero(request.get("date")).sat_i32(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        next_offset: value
            .get("next_offset")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    }
}

pub(crate) fn parse_community_member_chats(value: &Value) -> CommunityMemberChats {
    CommunityMemberChats {
        owned_chat_ids: int53_list(value.get("owned_chat_ids")),
        joined_chat_ids: int53_list(value.get("joined_chat_ids")),
    }
}
