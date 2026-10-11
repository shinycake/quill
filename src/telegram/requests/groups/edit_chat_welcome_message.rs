//! Methods moved out of `groups.rs` to keep files under 1000 lines.

use super::*;

/// Slice G2: `editChatWelcomeMessage` (TDLib 1.8.67,
/// `schema/td_api.tl:12646`):
/// `editChatWelcomeMessage chat_id:int53 welcome_message_id:int32
/// input_message_content:InputMessageContent = Ok;`
pub fn edit_chat_welcome_message(
    extra: RequestId,
    chat_id: ChatId,
    welcome_message_id: i32,
    text: &str,
) -> String {
    json!({
        "@type": "editChatWelcomeMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "welcome_message_id": welcome_message_id,
        "input_message_content": welcome_message_input_content(text),
    })
    .to_string()
}

/// Slice G2: `deleteChatWelcomeMessage` (TDLib 1.8.67,
/// `schema/td_api.tl:12651`):
/// `deleteChatWelcomeMessage chat_id:int53 welcome_message_id:int32 = Ok;`
pub fn delete_chat_welcome_message(
    extra: RequestId,
    chat_id: ChatId,
    welcome_message_id: i32,
) -> String {
    json!({
        "@type": "deleteChatWelcomeMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "welcome_message_id": welcome_message_id,
    })
    .to_string()
}

/// Slice G1: `setSupergroupUsername` (TDLib 1.8.67, `schema/td_api.tl:15136`):
/// `setSupergroupUsername supergroup_id:int53 username:string = Ok;`
/// "Changes the editable username of a supergroup or channel, requires
/// owner privileges". Empty string removes the username.
pub fn set_supergroup_username(extra: RequestId, supergroup_id: i64, username: &str) -> String {
    json!({
        "@type": "setSupergroupUsername",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "username": username,
    })
    .to_string()
}

/// Slice (communities backend core): `createCommunity` (TDLib 1.8.67,
/// `schema/td_api.tl:11806`):
/// `createCommunity name:string chat_id:int53 is_chat_hidden:Bool = CommunityId;`
/// "Creates a new community for the given chat. Returns identifier of
/// the created community" — "Identifier of the chat in the community;
/// only chats with owned bots and owned basic group, supergroup and
/// channel chats are allowed; basic group chats will be automatically
/// upgraded to supergroup chats". The response is `communityId`; the
/// driver chains it into `getCommunityFullInfo`. Empty names are
/// refused client-side by the driver.
pub fn create_community(
    extra: RequestId,
    name: &str,
    chat_id: i64,
    is_chat_hidden: bool,
) -> String {
    json!({
        "@type": "createCommunity",
        "@extra": extra.as_extra(),
        "name": name,
        "chat_id": chat_id,
        "is_chat_hidden": is_chat_hidden,
    })
    .to_string()
}

/// `getCommunityFullInfo` (TDLib 1.8.68, `schema/td_api.tl:12178`):
/// `getCommunityFullInfo community_id:int53 = CommunityFullInfo;`
/// "Returns full information about a community". Replaced 1.8.67's
/// `loadCommunityFullInfo` (which answered `ok` and sent the data through
/// `updateCommunityFullInfo`); the answer is now the pack itself.
pub fn get_community_full_info(extra: RequestId, community_id: i64) -> String {
    json!({
        "@type": "getCommunityFullInfo",
        "@extra": extra.as_extra(),
        "community_id": community_id,
    })
    .to_string()
}

/// Slice (communities backend core): `setCommunityName` (TDLib 1.8.67,
/// `schema/td_api.tl:11811`):
/// `setCommunityName community_id:int53 name:string = Ok;`
/// "Changes name of the given community; requires can_change_info
/// administrator right in the community". Empty names are refused
/// client-side by the driver; the new name arrives via `updateCommunity`
/// and the driver refetches the full-info pack on success.
pub fn set_community_name(extra: RequestId, community_id: i64, name: &str) -> String {
    json!({
        "@type": "setCommunityName",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "name": name,
    })
    .to_string()
}

/// `setCommunityPhoto` (TDLib 1.8.68, `schema/td_api.tl:12195`):
/// `setCommunityPhoto community_id:int53 photo:InputChatPhoto = Ok;`
/// "requires can_change_info administrator right in the community";
/// `photo` is an `InputChatPhoto` object, or null to delete the photo
/// (same shape as `setChatPhoto`).
pub fn set_community_photo(extra: RequestId, community_id: i64, photo: Value) -> String {
    json!({
        "@type": "setCommunityPhoto",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "photo": photo,
    })
    .to_string()
}

/// `setCommunityPermissions` (TDLib 1.8.68, `schema/td_api.tl:12200`):
/// `setCommunityPermissions community_id:int53 permissions:communityPermissions = Ok;`
/// "Changes permissions of regular members in the given community;
/// requires can_ban_members administrator right". `communityPermissions`
/// has a single flag, `can_edit_chat_list`.
pub fn set_community_permissions(
    extra: RequestId,
    community_id: i64,
    can_edit_chat_list: bool,
) -> String {
    json!({
        "@type": "setCommunityPermissions",
        "@extra": extra.as_extra(),
        "community_id": community_id,
        "permissions": {
            "@type": "communityPermissions",
            "can_edit_chat_list": can_edit_chat_list,
        },
    })
    .to_string()
}

/// `deleteCommunity` (TDLib 1.8.68, `schema/td_api.tl:12203`):
/// `deleteCommunity community_id:int53 = Ok;` — "requires owner
/// privileges".
pub fn delete_community(extra: RequestId, community_id: i64) -> String {
    json!({
        "@type": "deleteCommunity",
        "@extra": extra.as_extra(),
        "community_id": community_id,
    })
    .to_string()
}

/// `getChatMessageCount` (schema 1.8.67, line 11989) for one filter, from
/// the server (`return_local: false`).
pub fn get_chat_message_count(extra: RequestId, chat_id: ChatId, filter: &str) -> String {
    json!({
        "@type": "getChatMessageCount",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "filter": { "@type": filter },
        "return_local": false,
    })
    .to_string()
}

/// Subsection tabs: `setForumTopicNotificationSettings` (TDLib 1.8.67,
/// `schema/td_api.tl:12707`) — the same `chatNotificationSettings` body as
/// `setChatNotificationSettings`, addressed to one topic.
pub fn set_forum_topic_notification_settings(
    extra: RequestId,
    chat_id: ChatId,
    forum_topic_id: i32,
    settings: &crate::telegram::envelope::ChatNotificationSettings,
) -> String {
    let mut value: Value = serde_json::from_str(
        &super::super::chats::set_chat_notification_settings(extra, chat_id, settings),
    )
    .unwrap_or_else(|_| json!({}));
    value["@type"] = json!("setForumTopicNotificationSettings");
    value["forum_topic_id"] = json!(forum_topic_id);
    value.to_string()
}

/// Subsection tabs: `getForumTopic` (TDLib 1.8.67, `schema/td_api.tl:12679`)
/// — one topic with TDLib's own `unread_count`, refetched when the topic's
/// read position or messages change.
pub fn get_forum_topic(extra: RequestId, chat_id: ChatId, forum_topic_id: i32) -> String {
    json!({
        "@type": "getForumTopic",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
    })
    .to_string()
}

/// `banChatMember chat_id:int53 member_id:MessageSender
/// banned_until_date:int32 revoke_messages:Bool = Ok` (TDLib 1.8.67,
/// `schema/td_api.tl:13605`). Telegram Desktop's "Remove from group" in a
/// basic group (`messages.deleteChatUser`): the only removal TDLib offers
/// there, since `setChatMemberStatus` cannot ban in basic groups.
pub fn ban_chat_member(
    extra: RequestId,
    chat_id: i64,
    user_id: i64,
    banned_until_date: i32,
    revoke_messages: bool,
) -> String {
    json!({
        "@type": "banChatMember",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "member_id": { "@type": "messageSenderUser", "user_id": user_id },
        "banned_until_date": banned_until_date,
        "revoke_messages": revoke_messages,
    })
    .to_string()
}

/// `canTransferOwnership = CanTransferOwnershipResult` (schema line 13608):
/// whether this session may hand a chat over (2-step verification on for
/// 7 days, session older than 24 hours).
pub fn can_transfer_ownership(extra: RequestId) -> String {
    json!({
        "@type": "canTransferOwnership",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `transferChatOwnership chat_id:int53 user_id:int53 password:string = Ok`
/// (schema line 13614). `password` is the user's 2-step verification
/// password; callers must not log the returned JSON.
pub fn transfer_chat_ownership(
    extra: RequestId,
    chat_id: i64,
    user_id: i64,
    password: &str,
) -> String {
    json!({
        "@type": "transferChatOwnership",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "user_id": user_id,
        "password": password,
    })
    .to_string()
}

/// `getChatOwnerAfterLeaving chat_id:int53 = User` (schema line 13619):
/// who inherits a chat when its owner leaves.
pub fn get_chat_owner_after_leaving(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "getChatOwnerAfterLeaving",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}
