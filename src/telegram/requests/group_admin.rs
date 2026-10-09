//! B7: group and channel admin toggle request builders (topics, history
//! for new members, join-to-send, hidden members, protected content,
//! discussion group, allowed reactions, basic group upgrade). Schema
//! references are TDLib 1.8.67 `schema/td_api.tl`.
use crate::ids::RequestId;
use crate::telegram::envelope::ChatAvailableReactions;
use serde_json::json;

/// `toggleSupergroupIsForum supergroup_id:int53 is_forum:Bool
/// has_forum_tabs:Bool = Ok;` (line 15218). Requires owner privileges;
/// discussion supergroups can't become forums. `has_forum_tabs` is
/// ignored when `is_forum` is false.
pub fn toggle_supergroup_is_forum(
    extra: RequestId,
    supergroup_id: i64,
    is_forum: bool,
    has_forum_tabs: bool,
) -> String {
    json!({
        "@type": "toggleSupergroupIsForum",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "is_forum": is_forum,
        "has_forum_tabs": is_forum && has_forum_tabs,
    })
    .to_string()
}

/// `toggleSupergroupIsAllHistoryAvailable supergroup_id:int53
/// is_all_history_available:Bool = Ok;` (line 15191). Requires the
/// `can_change_info` right.
pub fn toggle_supergroup_is_all_history_available(
    extra: RequestId,
    supergroup_id: i64,
    is_all_history_available: bool,
) -> String {
    json!({
        "@type": "toggleSupergroupIsAllHistoryAvailable",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "is_all_history_available": is_all_history_available,
    })
    .to_string()
}

/// `toggleSupergroupJoinToSendMessages supergroup_id:int53
/// join_to_send_messages:Bool = Ok;` (line 15180). Requires the
/// `can_restrict_members` right; not for broadcast groups.
pub fn toggle_supergroup_join_to_send_messages(
    extra: RequestId,
    supergroup_id: i64,
    join_to_send_messages: bool,
) -> String {
    json!({
        "@type": "toggleSupergroupJoinToSendMessages",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "join_to_send_messages": join_to_send_messages,
    })
    .to_string()
}

/// `toggleSupergroupHasHiddenMembers supergroup_id:int53
/// has_hidden_members:Bool = Ok;` (line 15207). Only when
/// `supergroupFullInfo.can_hide_members`.
pub fn toggle_supergroup_has_hidden_members(
    extra: RequestId,
    supergroup_id: i64,
    has_hidden_members: bool,
) -> String {
    json!({
        "@type": "toggleSupergroupHasHiddenMembers",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "has_hidden_members": has_hidden_members,
    })
    .to_string()
}

/// `toggleChatHasProtectedContent chat_id:int53
/// has_protected_content:Bool = Ok;` (line 13504). Owner only in groups
/// and channels.
pub fn toggle_chat_has_protected_content(
    extra: RequestId,
    chat_id: i64,
    has_protected_content: bool,
) -> String {
    json!({
        "@type": "toggleChatHasProtectedContent",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "has_protected_content": has_protected_content,
    })
    .to_string()
}

/// `getSuitableDiscussionChats = Chats;` (line 11687): the groups the
/// user may set as a channel's discussion group.
pub fn get_suitable_discussion_chats(extra: RequestId) -> String {
    json!({
        "@type": "getSuitableDiscussionChats",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `setChatDiscussionGroup chat_id:int53 discussion_chat_id:int53 = Ok;`
/// (line 13539). From the channel: `chat_id` is the channel and
/// `discussion_chat_id` the group (0 unlinks). From the group side
/// (unlink only): `chat_id` is 0 and `discussion_chat_id` the group.
pub fn set_chat_discussion_group(
    extra: RequestId,
    chat_id: i64,
    discussion_chat_id: i64,
) -> String {
    json!({
        "@type": "setChatDiscussionGroup",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "discussion_chat_id": discussion_chat_id,
    })
    .to_string()
}

/// `setChatAvailableReactions chat_id:int53
/// available_reactions:ChatAvailableReactions = Ok;` (line 13527).
/// Requires the `can_change_info` right. "No reactions" is a `Some` with
/// an empty list.
pub fn set_chat_available_reactions(
    extra: RequestId,
    chat_id: i64,
    available_reactions: &ChatAvailableReactions,
) -> String {
    json!({
        "@type": "setChatAvailableReactions",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "available_reactions": available_reactions.to_tdlib_json(),
    })
    .to_string()
}

/// `upgradeBasicGroupChatToSupergroupChat chat_id:int53 = Chat;` (line
/// 13343). Owner only; deactivates the basic group.
pub fn upgrade_basic_group_chat_to_supergroup_chat(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "upgradeBasicGroupChatToSupergroupChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}
