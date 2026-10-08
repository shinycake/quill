use super::format_entity_json;
use crate::composer::parse_format_markup;
use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::{Value, json};

/// `getForumTopics` (TDLib 1.8.67, `schema/td_api.tl:12701`):
/// `getForumTopics chat_id:int53 query:string offset_date:int32
/// offset_message_id:int53 offset_forum_topic_id:int32 limit:int32 =
/// ForumTopics`. First page: empty query, all offsets 0.
pub fn get_forum_topics(
    extra: RequestId,
    chat_id: ChatId,
    query: &str,
    offset_date: i32,
    offset_message_id: MessageId,
    offset_forum_topic_id: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "getForumTopics",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "query": query,
        "offset_date": offset_date,
        "offset_message_id": offset_message_id.0,
        "offset_forum_topic_id": offset_forum_topic_id,
        "limit": limit,
    })
    .to_string()
}

/// `getSupergroup` (TDLib 1.8.67, `schema/td_api.tl:11510`):
/// `getSupergroup supergroup_id:int53 = Supergroup`. Response carries
/// `supergroup.is_forum` (schema line 2746), which is how Quill learns a
/// supergroup is a forum (`chatTypeSupergroup` itself has no forum flag).
pub fn get_supergroup(extra: RequestId, supergroup_id: i64) -> String {
    json!({
        "@type": "getSupergroup",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
    })
    .to_string()
}

/// `schema/td_api.tl:11513`):
/// `getSupergroupFullInfo supergroup_id:int53 = SupergroupFullInfo;`
/// Response is `supergroupFullInfo` (carries no id — correlated via the
/// pending request in `Session::apply`).
pub fn get_supergroup_full_info(extra: RequestId, supergroup_id: i64) -> String {
    json!({
        "@type": "getSupergroupFullInfo",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
    })
    .to_string()
}

/// Phase D2: `getChatStatistics` (TDLib 1.8.67, `schema/td_api.tl:15760`):
/// `getChatStatistics chat_id:int53 is_dark:Bool = ChatStatistics;`
/// Response is `chatStatisticsChannel` / `chatStatisticsSupergroup`.
/// Usable only when `supergroupFullInfo.can_get_statistics` is true
/// (checked by the driver before sending); TDLib errors otherwise.
pub fn get_chat_statistics(extra: RequestId, chat_id: i64, is_dark: bool) -> String {
    json!({
        "@type": "getChatStatistics",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "is_dark": is_dark,
    })
    .to_string()
}

/// Phase D3a: `getChatInviteLinks` (TDLib 1.8.67, `schema/td_api.tl:14138`):
/// `getChatInviteLinks chat_id:int53 creator_user_id:int53 is_revoked:Bool offset_date:int32 offset_invite_link:string limit:int32 = ChatInviteLinks;`
/// Returns the chat's invite links, filterable by creator and revocation
/// state; pagination goes through offset_date/offset_invite_link.
pub fn get_chat_invite_links(
    extra: RequestId,
    chat_id: i64,
    creator_user_id: i64,
    is_revoked: bool,
    offset_date: i32,
    offset_invite_link: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getChatInviteLinks",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "creator_user_id": creator_user_id,
        "is_revoked": is_revoked,
        "offset_date": offset_date,
        "offset_invite_link": offset_invite_link,
        "limit": limit,
    })
    .to_string()
}

/// Phase D3a: `createChatInviteLink` (TDLib 1.8.67, `schema/td_api.tl:14097`):
/// `createChatInviteLink chat_id:int53 name:string expiration_date:int32 member_limit:int32 creates_join_request:Bool = ChatInviteLink;`
/// Creates a new invite link for the chat with optional name, expiration
/// date (0 = none), member limit (0 = unlimited), and join-request mode.
pub fn create_chat_invite_link(
    extra: RequestId,
    chat_id: i64,
    name: &str,
    expiration_date: i32,
    member_limit: i32,
    creates_join_request: bool,
) -> String {
    json!({
        "@type": "createChatInviteLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "name": name,
        "expiration_date": expiration_date,
        "member_limit": member_limit,
        "creates_join_request": creates_join_request,
    })
    .to_string()
}

/// Phase D3a: `editChatInviteLink` (TDLib 1.8.67, `schema/td_api.tl:14115`):
/// `editChatInviteLink chat_id:int53 invite_link:string name:string expiration_date:int32 member_limit:int32 creates_join_request:Bool = ChatInviteLink;`
/// Edits an existing invite link's name, expiration, member limit, and
/// join-request mode. Returns the updated link.
pub fn edit_chat_invite_link(
    extra: RequestId,
    chat_id: i64,
    invite_link: &str,
    name: &str,
    expiration_date: i32,
    member_limit: i32,
    creates_join_request: bool,
) -> String {
    json!({
        "@type": "editChatInviteLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "invite_link": invite_link,
        "name": name,
        "expiration_date": expiration_date,
        "member_limit": member_limit,
        "creates_join_request": creates_join_request,
    })
    .to_string()
}

/// Phase D3a: `revokeChatInviteLink` (TDLib 1.8.67, `schema/td_api.tl:14152`):
/// `revokeChatInviteLink chat_id:int53 invite_link:string = ChatInviteLinks;`
/// Revokes an invite link; this is the only delete path — 1.8.67 has no
/// `deleteChatInviteLink`. Returns the updated link list.
pub fn revoke_chat_invite_link(extra: RequestId, chat_id: i64, invite_link: &str) -> String {
    json!({
        "@type": "revokeChatInviteLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "invite_link": invite_link,
    })
    .to_string()
}

/// Phase D3a: `getChatJoinRequests` (TDLib 1.8.67, `schema/td_api.tl:14174`):
/// `getChatJoinRequests chat_id:int53 invite_link:string query:string offset_request:chatJoinRequest limit:int32 = ChatJoinRequests;`
/// Returns pending join requests for a chat (optionally filtered by
/// invite link and a search query). First page uses an empty
/// `chatJoinRequest` offset, as official clients do.
pub fn get_chat_join_requests(
    extra: RequestId,
    chat_id: i64,
    invite_link: &str,
    query: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getChatJoinRequests",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "invite_link": invite_link,
        "query": query,
        "offset_request": {
            "@type": "chatJoinRequest",
        },
        "limit": limit,
    })
    .to_string()
}

/// Phase D3a: `processChatJoinRequest` (TDLib 1.8.67, `schema/td_api.tl:14177`):
/// `processChatJoinRequest chat_id:int53 user_id:int53 approve:Bool = Ok;`
/// Approves or declines a user's pending request to join the chat.
pub fn process_chat_join_request(
    extra: RequestId,
    chat_id: i64,
    user_id: i64,
    approve: bool,
) -> String {
    json!({
        "@type": "processChatJoinRequest",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "user_id": user_id,
        "approve": approve,
    })
    .to_string()
}

/// Phase D3b: `getChatAdministrators` (TDLib 1.8.67, `schema/td_api.tl:13632`):
/// `getChatAdministrators chat_id:int53 = ChatAdministrators;`
/// Returns the chat's administrator list (owner first); the response
/// carries no chat id, so it is correlated via `PendingRequest::chat_id`.
pub fn get_chat_administrators(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "getChatAdministrators",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// Phase D3b: `setChatMemberStatus` (TDLib 1.8.67,
/// `schema/td_api.tl:13592`):
/// `setChatMemberStatus chat_id:int53 member_id:MessageSender
/// status:ChatMemberStatus = Ok;`
/// Promotes, edits, or demotes a member depending on `status`
/// (`chatMemberStatusAdministrator` / `chatMemberStatusMember`). The
/// member change itself arrives later as `updateChatMember`.
pub fn set_chat_member_status(
    extra: RequestId,
    chat_id: i64,
    member_id: &Value,
    status: &Value,
) -> String {
    json!({
        "@type": "setChatMemberStatus",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "member_id": member_id,
        "status": status,
    })
    .to_string()
}

/// Phase D3b: `chatMemberStatusAdministrator` JSON (TDLib 1.8.67,
/// `schema/td_api.tl:2500`):
/// `chatMemberStatusAdministrator can_be_edited:Bool
/// rights:chatAdministratorRights = ChatMemberStatus;`
/// Used for both promote and edit-rights `setChatMemberStatus` calls.
pub fn chat_member_status_administrator_json(
    can_be_edited: bool,
    rights: &crate::telegram::envelope::ChatAdminRights,
) -> Value {
    json!({
        "@type": "chatMemberStatusAdministrator",
        "can_be_edited": can_be_edited,
        "rights": rights.to_json(),
    })
}

/// Phase D3b: `chatMemberStatusMember` JSON (TDLib 1.8.67,
/// `schema/td_api.tl:2504`):
/// `chatMemberStatusMember member_until_date:int32 = ChatMemberStatus;`
/// Demoting an admin is a `setChatMemberStatus` to plain member status.
pub fn chat_member_status_member_json() -> Value {
    json!({
        "@type": "chatMemberStatusMember",
        "member_until_date": 0,
    })
}

/// Phase D3b: `getSupergroupMembers` (TDLib 1.8.67,
/// `schema/td_api.tl:15238`):
/// `getSupergroupMembers supergroup_id:int53 filter:SupergroupMembersFilter
/// offset:int32 limit:int32 = ChatMembers;`
/// Drives the promote flow's member picker (recent members or a search
/// filter). The response carries no supergroup id, so it is correlated
/// via `PendingRequest::chat_id`.
pub fn get_supergroup_members(
    extra: RequestId,
    supergroup_id: i64,
    filter: &Value,
    offset: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "getSupergroupMembers",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "filter": filter,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// Slice G1: `getBasicGroupFullInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:11507`):
/// `getBasicGroupFullInfo basic_group_id:int53 = BasicGroupFullInfo;`
/// The response's `members:vector<chatMember>` (line 2714) is the member
/// list for basic groups. Correlated via `PendingRequest::chat_id`.
pub fn get_basic_group_full_info(extra: RequestId, basic_group_id: i64) -> String {
    json!({
        "@type": "getBasicGroupFullInfo",
        "@extra": extra.as_extra(),
        "basic_group_id": basic_group_id,
    })
    .to_string()
}

/// Phase D3b: `supergroupMembersFilterRecent` (TDLib 1.8.67,
/// `schema/td_api.tl:2559`) — the member picker's default filter.
pub fn supergroup_members_filter_recent_json() -> Value {
    json!({ "@type": "supergroupMembersFilterRecent" })
}

/// `searchChatMembers chat_id:int53 query:string limit:int32
/// filter:ChatMembersFilter = ChatMembers;` — members of any group whose
/// name or username matches `query` (the composer's `@` suggestions).
pub fn search_chat_members(extra: RequestId, chat_id: ChatId, query: &str, limit: i32) -> String {
    json!({
        "@type": "searchChatMembers",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "query": query,
        "limit": limit,
        "filter": null,
    })
    .to_string()
}

/// Phase D3b: `supergroupMembersFilterSearch` (TDLib 1.8.67,
/// `schema/td_api.tl:2568`):
/// `supergroupMembersFilterSearch query:string = SupergroupMembersFilter;`
pub fn supergroup_members_filter_search_json(query: &str) -> Value {
    json!({
        "@type": "supergroupMembersFilterSearch",
        "query": query,
    })
}

/// Slice G1: `supergroupMembersFilterAdministrators` (TDLib 1.8.67,
/// `schema/td_api.tl:2563`):
/// `supergroupMembersFilterAdministrators = SupergroupMembersFilter;`
/// Returns the owner and administrators.
pub fn supergroup_members_filter_administrators_json() -> Value {
    json!({ "@type": "supergroupMembersFilterAdministrators" })
}

/// Slice G1: `supergroupMembersFilterRestricted` (TDLib 1.8.67,
/// `schema/td_api.tl:2571`):
/// `supergroupMembersFilterRestricted query:string =
/// SupergroupMembersFilter;` — restricted members, administrators only.
pub fn supergroup_members_filter_restricted_json(query: &str) -> Value {
    json!({
        "@type": "supergroupMembersFilterRestricted",
        "query": query,
    })
}

/// Slice G1: `supergroupMembersFilterBanned` (TDLib 1.8.67,
/// `schema/td_api.tl:2574`):
/// `supergroupMembersFilterBanned query:string = SupergroupMembersFilter;`
/// — banned users, administrators only.
pub fn supergroup_members_filter_banned_json(query: &str) -> Value {
    json!({
        "@type": "supergroupMembersFilterBanned",
        "query": query,
    })
}

/// Slice G1: `chatMemberStatusRestricted` JSON (TDLib 1.8.67,
/// `schema/td_api.tl:2510`):
/// `chatMemberStatusRestricted is_member:Bool restricted_until_date:int32
/// permissions:chatPermissions = ChatMemberStatus;`
/// Used by the restrict flow's `setChatMemberStatus` call.
pub fn chat_member_status_restricted_json(
    is_member: bool,
    restricted_until_date: i32,
    permissions: &Value,
) -> Value {
    json!({
        "@type": "chatMemberStatusRestricted",
        "is_member": is_member,
        "restricted_until_date": restricted_until_date,
        "permissions": permissions,
    })
}

/// Slice G1: `chatMemberStatusBanned` JSON (TDLib 1.8.67,
/// `schema/td_api.tl:2517`):
/// `chatMemberStatusBanned banned_until_date:int32 = ChatMemberStatus;`
/// Used by the ban flow's `setChatMemberStatus` call.
pub fn chat_member_status_banned_json(banned_until_date: i32) -> Value {
    json!({
        "@type": "chatMemberStatusBanned",
        "banned_until_date": banned_until_date,
    })
}

/// Slice G1: `createNewBasicGroupChat` (TDLib 1.8.67,
/// `schema/td_api.tl:13327`):
/// `createNewBasicGroupChat user_ids:vector<int53> title:string
/// message_auto_delete_time:int32 = CreatedBasicGroupChat;`
/// Title is 1-128 characters. The new chat arrives as `updateNewChat`;
/// the `CreatedBasicGroupChat` answer carries its id.
pub fn create_new_basic_group_chat(extra: RequestId, user_ids: &[i64], title: &str) -> String {
    json!({
        "@type": "createNewBasicGroupChat",
        "@extra": extra.as_extra(),
        "user_ids": user_ids,
        "title": title,
        "message_auto_delete_time": 0,
    })
    .to_string()
}

/// Slice G1: `createNewSupergroupChat` (TDLib 1.8.67,
/// `schema/td_api.tl:13337`):
/// `createNewSupergroupChat title:string is_forum:Bool is_channel:Bool
/// description:string location:chatLocation message_auto_delete_time:int32
/// for_import:Bool = Chat;`
/// `location` is null for an ordinary supergroup/channel (schema line
/// 13335: "pass null to create an ordinary supergroup chat").
pub fn create_new_supergroup_chat(
    extra: RequestId,
    title: &str,
    is_channel: bool,
    description: &str,
) -> String {
    json!({
        "@type": "createNewSupergroupChat",
        "@extra": extra.as_extra(),
        "title": title,
        "is_forum": false,
        "is_channel": is_channel,
        "description": description,
        "location": Value::Null,
        "message_auto_delete_time": 0,
        "for_import": false,
    })
    .to_string()
}

/// Slice G1: `toggleSupergroupIsBroadcastGroup` (TDLib 1.8.67,
/// `schema/td_api.tl:15221`):
/// `toggleSupergroupIsBroadcastGroup supergroup_id:int53 = Ok;`
/// "Upgrades supergroup to a broadcast group; requires owner privileges".
/// One-way per the schema description (no parameter to convert back — a
/// broadcast group stays a broadcast group; Telegram has no reverse API).
pub fn toggle_supergroup_is_broadcast_group(extra: RequestId, supergroup_id: i64) -> String {
    json!({
        "@type": "toggleSupergroupIsBroadcastGroup",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
    })
    .to_string()
}

/// Slice G1: `addChatMembers` (TDLib 1.8.67, `schema/td_api.tl:13584`):
/// `addChatMembers chat_id:int53 user_ids:vector<int53> =
/// FailedToAddMembers;`
/// "Adds multiple new members to a chat; requires can_invite_users member
/// right. Currently, this method is available only in supergroups and
/// channels." Max 20 users per call for supergroups, 100 for channels.
pub fn add_chat_members(extra: RequestId, chat_id: i64, user_ids: &[i64]) -> String {
    json!({
        "@type": "addChatMembers",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "user_ids": user_ids,
    })
    .to_string()
}

/// Slice G1: `addChatMember` (TDLib 1.8.67, `schema/td_api.tl:13578`):
/// `addChatMember chat_id:int53 user_id:int53 forward_limit:int32 =
/// FailedToAddMembers;`
/// "Adds a new member to a chat; requires can_invite_users member right."
/// The singular variant used for basic groups (`addChatMembers` is
/// supergroups and channels only, line 13580).
pub fn add_chat_member(extra: RequestId, chat_id: i64, user_id: i64) -> String {
    json!({
        "@type": "addChatMember",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "user_id": user_id,
        "forward_limit": 0,
    })
    .to_string()
}

/// Slice G1: `setChatPermissions` (TDLib 1.8.67, `schema/td_api.tl:13464`):
/// `setChatPermissions chat_id:int53 permissions:chatPermissions = Ok;`
/// "Supported only for basic groups and supergroups. Requires
/// can_restrict_members administrator right".
pub fn set_chat_permissions(extra: RequestId, chat_id: i64, permissions: &Value) -> String {
    json!({
        "@type": "setChatPermissions",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "permissions": permissions,
    })
    .to_string()
}

/// Slice G1: `replacePrimaryChatInviteLink` (TDLib 1.8.67,
/// `schema/td_api.tl:14089`):
/// `replacePrimaryChatInviteLink chat_id:int53 = ChatInviteLink;`
/// "Replaces current primary invite link for a chat with a new primary
/// invite link. Available for basic groups, supergroups, and channels.
/// Requires administrator privileges and can_invite_users right".
pub fn replace_primary_chat_invite_link(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "replacePrimaryChatInviteLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// Slice G1: `toggleSupergroupJoinByRequest` (TDLib 1.8.67,
/// `schema/td_api.tl:15188`):
/// `toggleSupergroupJoinByRequest supergroup_id:int53 join_by_request:Bool
/// guard_bot_user_id:int53 apply_to_invite_links:Bool = Ok;`
/// No guard bot in Quill (`guard_bot_user_id: 0`, ignored when
/// `join_by_request == false` per the schema); the change is not applied
/// to existing invite links (`apply_to_invite_links: false`).
pub fn toggle_supergroup_join_by_request(
    extra: RequestId,
    supergroup_id: i64,
    join_by_request: bool,
) -> String {
    json!({
        "@type": "toggleSupergroupJoinByRequest",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "join_by_request": join_by_request,
        "guard_bot_user_id": 0,
        "apply_to_invite_links": false,
    })
    .to_string()
}

/// Slice G2: `toggleSupergroupSignMessages` (TDLib 1.8.67,
/// `schema/td_api.tl:15175`):
/// `toggleSupergroupSignMessages supergroup_id:int53 sign_messages:Bool
/// show_message_sender:Bool = Ok;`
/// "Toggles whether sender signature or link to the account is added to
/// sent messages in a channel; requires can_change_info member right".
/// `show_message_sender` only takes effect when `sign_messages` is true
/// (Telegram X sends `sign && show`).
pub fn toggle_supergroup_sign_messages(
    extra: RequestId,
    supergroup_id: i64,
    sign_messages: bool,
    show_message_sender: bool,
) -> String {
    json!({
        "@type": "toggleSupergroupSignMessages",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "sign_messages": sign_messages,
        "show_message_sender": sign_messages && show_message_sender,
    })
    .to_string()
}

/// Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled` (TDLib 1.8.67,
/// `schema/td_api.tl:15212`):
/// `toggleSupergroupHasAggressiveAntiSpamEnabled supergroup_id:int53
/// has_aggressive_anti_spam_enabled:Bool = Ok;`
/// "Toggles whether aggressive anti-spam checks are enabled in the
/// supergroup. Can be called only if
/// supergroupFullInfo.can_toggle_aggressive_anti_spam == true".
pub fn toggle_supergroup_aggressive_anti_spam(
    extra: RequestId,
    supergroup_id: i64,
    enabled: bool,
) -> String {
    json!({
        "@type": "toggleSupergroupHasAggressiveAntiSpamEnabled",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "has_aggressive_anti_spam_enabled": enabled,
    })
    .to_string()
}

/// Slice G2: `createForumTopic` (TDLib 1.8.67, `schema/td_api.tl:12665`):
/// `createForumTopic chat_id:int53 name:string is_name_implicit:Bool
/// icon:forumTopicIcon = ForumTopicInfo;`
/// "Creates a topic in a forum supergroup chat ...; requires
/// can_manage_topics administrator or can_create_topics member right".
/// Icon is a required parameter; Quill sends the default blue
/// (0x6FB9F0) with no custom emoji (Telegram X default icon color).
pub fn create_forum_topic(extra: RequestId, chat_id: ChatId, name: &str) -> String {
    json!({
        "@type": "createForumTopic",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "name": name,
        "is_name_implicit": false,
        "icon": {
            "@type": "forumTopicIcon",
            "color": 0x6FB9F0,
            "custom_emoji_id": 0
        },
    })
    .to_string()
}

/// Slice G2: `editForumTopic` (TDLib 1.8.67, `schema/td_api.tl:12674`):
/// `editForumTopic chat_id:int53 forum_topic_id:int32 name:string
/// edit_icon_custom_emoji:Bool icon_custom_emoji_id:int64 = Ok;`
/// Quill edits the name only (`edit_icon_custom_emoji: false`).
pub fn edit_forum_topic(
    extra: RequestId,
    chat_id: ChatId,
    forum_topic_id: i32,
    name: &str,
) -> String {
    json!({
        "@type": "editForumTopic",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
        "name": name,
        "edit_icon_custom_emoji": false,
        "icon_custom_emoji_id": 0,
    })
    .to_string()
}

/// Slice G2: `toggleForumTopicIsClosed` (TDLib 1.8.67,
/// `schema/td_api.tl:12713`):
/// `toggleForumTopicIsClosed chat_id:int53 forum_topic_id:int32
/// is_closed:Bool = Ok;`
pub fn toggle_forum_topic_closed(
    extra: RequestId,
    chat_id: ChatId,
    forum_topic_id: i32,
    is_closed: bool,
) -> String {
    json!({
        "@type": "toggleForumTopicIsClosed",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
        "is_closed": is_closed,
    })
    .to_string()
}

/// Slice G2: `toggleForumTopicIsPinned` (TDLib 1.8.67,
/// `schema/td_api.tl:12725`):
/// `toggleForumTopicIsPinned chat_id:int53 forum_topic_id:int32
/// is_pinned:Bool = Ok;`
pub fn toggle_forum_topic_pinned(
    extra: RequestId,
    chat_id: ChatId,
    forum_topic_id: i32,
    is_pinned: bool,
) -> String {
    json!({
        "@type": "toggleForumTopicIsPinned",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
        "is_pinned": is_pinned,
    })
    .to_string()
}

/// Slice G2: `deleteForumTopic` (TDLib 1.8.67, `schema/td_api.tl:12736`):
/// `deleteForumTopic chat_id:int53 forum_topic_id:int32 = Ok;`
pub fn delete_forum_topic(extra: RequestId, chat_id: ChatId, forum_topic_id: i32) -> String {
    json!({
        "@type": "deleteForumTopic",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "forum_topic_id": forum_topic_id,
    })
    .to_string()
}

/// Slice G2: `toggleGeneralForumTopicIsHidden` (TDLib 1.8.67,
/// `schema/td_api.tl:12718`): "Toggles whether a General topic is hidden
/// in a forum supergroup chat; requires can_manage_topics administrator
/// right". There is no per-topic hide constructor in the pinned schema —
/// only the General topic can be hidden.
pub fn toggle_general_forum_topic_hidden(
    extra: RequestId,
    chat_id: ChatId,
    is_hidden: bool,
) -> String {
    json!({
        "@type": "toggleGeneralForumTopicIsHidden",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "is_hidden": is_hidden,
    })
    .to_string()
}

/// Slice G2: `getMessageThreadHistory` (TDLib 1.8.67,
/// `schema/td_api.tl:11839`):
/// `getMessageThreadHistory chat_id:int53 message_id:int53
/// from_message_id:int53 offset:int32 limit:int32 = Messages;`
/// "Returns messages in a message thread of a message. ... Message
/// thread of a channel message is in the channel's linked supergroup."
/// Used for the channel-comments viewer.
pub fn get_message_thread_history(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    from_message_id: MessageId,
    limit: i32,
) -> String {
    json!({
        "@type": "getMessageThreadHistory",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "from_message_id": from_message_id.0,
        "offset": 0,
        "limit": limit,
    })
    .to_string()
}

/// Slice G2: `getChatBoostStatus` (TDLib 1.8.67, `schema/td_api.tl:13917`):
/// `getChatBoostStatus chat_id:int53 = ChatBoostStatus;`
/// "Returns the current boost status for a supergroup or a channel chat".
pub fn get_chat_boost_status(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatBoostStatus",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// Slice G2: `getAvailableChatBoostSlots` (TDLib 1.8.67,
/// `schema/td_api.tl:13914`): "Returns the list of available chat boost
/// slots for the current user".
pub fn get_available_chat_boost_slots(extra: RequestId) -> String {
    json!({
        "@type": "getAvailableChatBoostSlots",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice G2: `boostChat` (TDLib 1.8.67, `schema/td_api.tl:13922`):
/// `boostChat chat_id:int53 slot_ids:vector<int32> = ChatBoostSlots;`
/// "Boosts a chat and returns the list of available chat boost slots for
/// the current user after the boost".
pub fn boost_chat(extra: RequestId, chat_id: ChatId, slot_ids: &[i32]) -> String {
    json!({
        "@type": "boostChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "slot_ids": slot_ids,
    })
    .to_string()
}

/// Slice G2: shared `inputMessageText` block for the welcome-message
/// requests — same markup→entities treatment as sends.
fn welcome_message_input_content(text: &str) -> Value {
    let (clean_text, entities) = parse_format_markup(text);
    let entities_json: Vec<Value> = entities.iter().map(format_entity_json).collect();
    json!({
        "@type": "inputMessageText",
        "text": {
            "@type": "formattedText",
            "text": clean_text,
            "entities": entities_json
        },
        "link_preview_options": Value::Null,
        "clear_draft": false
    })
}

/// Slice G2: `loadChatWelcomeMessages` (TDLib 1.8.67,
/// `schema/td_api.tl:12630`): "Loads welcome messages of a chat; requires
/// can_send_welcome_messages administrator right in the chat. The loaded
/// messages will be sent through updateChatWelcomeMessages".
pub fn load_chat_welcome_messages(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "loadChatWelcomeMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// Slice G2: `addChatWelcomeMessage` (TDLib 1.8.67, `schema/td_api.tl:12639`):
/// `addChatWelcomeMessage chat_id:int53
/// input_message_content:InputMessageContent = Ok;`
pub fn add_chat_welcome_message(extra: RequestId, chat_id: ChatId, text: &str) -> String {
    json!({
        "@type": "addChatWelcomeMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "input_message_content": welcome_message_input_content(text),
    })
    .to_string()
}

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
/// driver chains it into `loadCommunityFullInfo`. Empty names are
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

/// Slice (communities backend core): `loadCommunityFullInfo` (TDLib
/// 1.8.67, `schema/td_api.tl:11799`):
/// `loadCommunityFullInfo community_id:int53 = Ok;`
/// "Returns full information about a community. The data will be sent
/// through update" — i.e. as `updateCommunityFullInfo`, which the
/// reducer applies directly (it carries its own `community_id`).
pub fn load_community_full_info(extra: RequestId, community_id: i64) -> String {
    json!({
        "@type": "loadCommunityFullInfo",
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

/// The info panel's media rows, in Telegram Desktop's order: the
/// `searchMessagesFilter*` constructor and its row label (singular, plural).
pub const MEDIA_COUNT_FILTERS: [(&str, &str, &str); 7] = [
    ("searchMessagesFilterPhoto", "photo", "photos"),
    ("searchMessagesFilterVideo", "video", "videos"),
    ("searchMessagesFilterDocument", "file", "files"),
    ("searchMessagesFilterAudio", "audio file", "audio files"),
    ("searchMessagesFilterUrl", "shared link", "shared links"),
    (
        "searchMessagesFilterVoiceNote",
        "voice message",
        "voice messages",
    ),
    ("searchMessagesFilterAnimation", "GIF", "GIFs"),
];

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
    let mut value: Value = serde_json::from_str(&super::chats::set_chat_notification_settings(
        extra, chat_id, settings,
    ))
    .expect("request builders emit valid JSON");
    value["@type"] = json!("setForumTopicNotificationSettings");
    value["forum_topic_id"] = json!(forum_topic_id);
    value.to_string()
}
