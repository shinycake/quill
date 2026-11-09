//! Builders for the TDLib 1.8.70 community methods
//! (`schema/td_api.tl`, the block after `getCommunityFullInfo`).

use super::notification_settings_value;
use crate::ids::RequestId;
use crate::state::CommunityMember;
use crate::telegram::envelope::ChatNotificationSettings;
use serde_json::{Value, json};

fn member_sender(member: CommunityMember) -> Value {
    match member {
        CommunityMember::User(user_id) => json!({
            "@type": "messageSenderUser",
            "user_id": user_id,
        }),
        CommunityMember::Chat(chat_id) => json!({
            "@type": "messageSenderChat",
            "chat_id": chat_id,
        }),
    }
}

/// `getSuitableCommunityChats = Chats;` — the owned supergroups and
/// channels that can be added to a community.
pub fn get_suitable_community_chats(extra: RequestId) -> String {
    json!({
        "@type": "getSuitableCommunityChats",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `getJoinedCommunities = Communities;`
pub fn get_joined_communities(extra: RequestId) -> String {
    json!({
        "@type": "getJoinedCommunities",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `addCommunityChat community_id:int53 chat_id:int53 is_hidden:Bool = Ok;`
/// Without the right to change the list, the method sends a request to
/// the administrators and fails with `COMMUNITY_REQUEST_CREATED`.
pub fn add_community_chat(
    extra: RequestId,
    community_id: i64,
    chat_id: i64,
    is_hidden: bool,
) -> String {
    json!({
        "@type": "addCommunityChat",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "chat_id": chat_id,
        "is_hidden": is_hidden,
    })
    .to_string()
}

/// `removeCommunityChat community_id:int53 chat_id:int53 = Ok;`
pub fn remove_community_chat(extra: RequestId, community_id: i64, chat_id: i64) -> String {
    json!({
        "@type": "removeCommunityChat",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "chat_id": chat_id,
    })
    .to_string()
}

/// `getCommunityAddChatRequests community_id:int53 offset:string
/// limit:int32 = CommunityAddChatRequests;`
pub fn get_community_add_chat_requests(
    extra: RequestId,
    community_id: i64,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getCommunityAddChatRequests",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// `processCommunityAddChatRequest community_id:int53 chat_id:int53
/// approve:Bool = Ok;`
pub fn process_community_add_chat_request(
    extra: RequestId,
    community_id: i64,
    chat_id: i64,
    approve: bool,
) -> String {
    json!({
        "@type": "processCommunityAddChatRequest",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "chat_id": chat_id,
        "approve": approve,
    })
    .to_string()
}

/// `processCommunityAddChatRequests community_id:int53 approve:Bool = Ok;`
pub fn process_community_add_chat_requests(
    extra: RequestId,
    community_id: i64,
    approve: bool,
) -> String {
    json!({
        "@type": "processCommunityAddChatRequests",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "approve": approve,
    })
    .to_string()
}

/// `toggleCommunityAreChatsCollapsed community_id:int53
/// are_chats_collapsed:Bool = Ok;` Expanding removes the community from
/// every chat folder.
pub fn toggle_community_are_chats_collapsed(
    extra: RequestId,
    community_id: i64,
    are_chats_collapsed: bool,
) -> String {
    json!({
        "@type": "toggleCommunityAreChatsCollapsed",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "are_chats_collapsed": are_chats_collapsed,
    })
    .to_string()
}

/// `setCommunityNotificationSettings community_id:int53
/// notification_settings:chatNotificationSettings = Ok;`
pub fn set_community_notification_settings(
    extra: RequestId,
    community_id: i64,
    settings: &ChatNotificationSettings,
) -> String {
    json!({
        "@type": "setCommunityNotificationSettings",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "notification_settings": notification_settings_value(settings),
    })
    .to_string()
}

/// `toggleCommunityMemberIsBanned community_id:int53
/// member_id:MessageSender is_banned:Bool = Ok;`
pub fn toggle_community_member_is_banned(
    extra: RequestId,
    community_id: i64,
    member: CommunityMember,
    is_banned: bool,
) -> String {
    json!({
        "@type": "toggleCommunityMemberIsBanned",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "member_id": member_sender(member),
        "is_banned": is_banned,
    })
    .to_string()
}

/// `getCommunityMemberChats community_id:int53 member_id:MessageSender =
/// CommunityMemberChats;`
pub fn get_community_member_chats(
    extra: RequestId,
    community_id: i64,
    member: CommunityMember,
) -> String {
    json!({
        "@type": "getCommunityMemberChats",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "member_id": member_sender(member),
    })
    .to_string()
}
