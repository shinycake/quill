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
        date: int53(value.get("date")).ok().unwrap_or(0) as i32,
        is_owner,
        can_change_info: is_owner || admin_right("can_change_info"),
        can_ban_members: is_owner || admin_right("can_ban_members"),
        members_can_edit_chat_list: value
            .get("permissions")
            .and_then(|p| p.get("can_edit_chat_list"))
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
        administrator_count: int53(value.get("administrator_count")).ok().unwrap_or(0) as i32,
        banned_count: int53(value.get("banned_count")).ok().unwrap_or(0) as i32,
        add_chat_request_count: int53(value.get("add_chat_request_count")).ok().unwrap_or(0) as i32,
    })
}
