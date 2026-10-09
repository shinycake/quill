use crate::ids::{ChatId, MessageId, RequestId, TopicId};
use serde_json::{Value, json};

pub fn load_chats(extra: RequestId, limit: i32) -> String {
    load_chats_list(extra, json!({ "@type": "chatListMain" }), limit)
}

/// `loadChats(chatListArchive)` — TDLib reports archived chats' positions
/// only once the archive list is loaded.
pub fn load_archive_chats(extra: RequestId, limit: i32) -> String {
    load_chats_list(extra, json!({ "@type": "chatListArchive" }), limit)
}

/// `loadChats` for an arbitrary chat list (Phase 7.1: `chatListFolder`).
/// Schema 1.8.67: `loadChats chat_list:ChatList limit:int32 = Ok` (line 11595).
pub fn load_chats_list(extra: RequestId, chat_list: Value, limit: i32) -> String {
    json!({
        "@type": "loadChats",
        "@extra": extra.as_extra(),
        "chat_list": chat_list,
        "limit": limit,
    })
    .to_string()
}

/// `searchChats` (TDLib 1.8.67). Offline title/username search of known chats.
/// `type_filter` is null = all chat types (`SearchChatTypeFilter`).
pub fn search_chats(extra: RequestId, query: &str, limit: i32) -> String {
    json!({
        "@type": "searchChats",
        "@extra": extra.as_extra(),
        "query": query,
        "type_filter": Value::Null,
        "limit": limit,
    })
    .to_string()
}

/// `searchChatsOnServer` (TDLib 1.8.67, schema line 11621): like
/// `searchChats` but asks the server, so chats missing from the local
/// cache (the share box's "server search") are found too.
pub fn search_chats_on_server(extra: RequestId, query: &str, limit: i32) -> String {
    json!({
        "@type": "searchChatsOnServer",
        "@extra": extra.as_extra(),
        "query": query,
        "type_filter": Value::Null,
        "limit": limit,
    })
    .to_string()
}

/// `searchPublicChats` (TDLib 1.8.67, schema line 11609). Public username /
/// title lookup across all public chats (private chats, supergroups,
/// channels) — unlike `searchChats`, not limited to known chats.
/// `type_filter` is null = all chat types. Returns `chats`.
pub fn search_public_chats(extra: RequestId, query: &str) -> String {
    json!({
        "@type": "searchPublicChats",
        "@extra": extra.as_extra(),
        "query": query,
        "type_filter": Value::Null,
    })
    .to_string()
}

/// `searchPublicChat` (TDLib 1.8.67, schema line 11603):
/// `searchPublicChat username:string = Chat;`
/// Singular public-username lookup — returns the chat itself (unlike the
/// plural `searchPublicChats`, which returns ids). The inline-mode slice
/// uses it for `@botname` → bot user id resolution: a bot username
/// resolves to its private chat (`chatTypePrivate.user_id`).
pub fn search_public_chat(extra: RequestId, username: &str) -> String {
    json!({
        "@type": "searchPublicChat",
        "@extra": extra.as_extra(),
        "username": username,
    })
    .to_string()
}

/// `searchMessages` (TDLib 1.8.67). `chat_list` null = all lists (official
/// clients / Unigram); schema: only Main and Archive are searchable.
/// `filter` null = all message types; `community_filter` selects the
/// `searchMessagesChatTypeFilterCommunity` constructor when `Some(id)`,
/// `None` keeps the historical null-filter behavior (all chat types).
pub fn search_messages(
    extra: RequestId,
    query: &str,
    limit: i32,
    community_filter: Option<i64>,
) -> String {
    search_messages_filtered(
        extra,
        query,
        limit,
        &SearchMessagesFilters {
            community_id: community_filter,
            ..SearchMessagesFilters::default()
        },
    )
}

/// The narrowing arguments of a global `searchMessages`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchMessagesFilters {
    /// `searchMessagesChatTypeFilterCommunity`; wins over `chat_type`
    /// (TDLib takes one chat-type filter).
    pub community_id: Option<i64>,
    pub chat_type: crate::search_filters::SearchChatType,
    pub media: crate::search_filters::SearchMediaKind,
    /// `min_date`, 0 = no bound.
    pub min_date: i32,
}

/// `searchMessages` (schema 1.8.67, line 11877) with the chat-type, media
/// and date filters of the global search bar.
pub fn search_messages_filtered(
    extra: RequestId,
    query: &str,
    limit: i32,
    filters: &SearchMessagesFilters,
) -> String {
    let chat_type_filter = match (filters.community_id, filters.chat_type.constructor()) {
        (Some(id), _) => json!({
            "@type": "searchMessagesChatTypeFilterCommunity",
            "community_id": id,
        }),
        (None, Some(constructor)) => json!({ "@type": constructor }),
        (None, None) => Value::Null,
    };
    let filter = filters
        .media
        .constructor()
        .map_or(Value::Null, search_messages_filter_json);
    json!({
        "@type": "searchMessages",
        "@extra": extra.as_extra(),
        "chat_list": Value::Null,
        "query": query,
        "offset": "",
        "limit": limit,
        "filter": filter,
        "chat_type_filter": chat_type_filter,
        "min_date": filters.min_date,
        "max_date": 0,
    })
    .to_string()
}

/// `searchRecentlyFoundChats` (TDLib 1.8.67). Offline; empty `query` is the
/// recently-found list (official empty-search surface). Up to 50 chats.
pub fn search_recently_found_chats(extra: RequestId, query: &str, limit: i32) -> String {
    json!({
        "@type": "searchRecentlyFoundChats",
        "@extra": extra.as_extra(),
        "query": query,
        "type_filter": Value::Null,
        "limit": limit,
    })
    .to_string()
}

/// Slice CL2: settings for automatic moving of chats to and from the
/// Archive chat list (TDLib 1.8.67, `schema/td_api.tl:3512`):
/// `archiveChatListSettings archive_and_mute_new_chats_from_unknown_users:Bool
/// keep_unmuted_chats_archived:Bool keep_chats_from_folders_archived:Bool =
/// ArchiveChatListSettings;`
/// The schema's field docs note `archive_and_mute_new_chats_from_unknown_users`
/// can only be set when the option
/// `can_archive_and_mute_new_chats_from_unknown_users` is true, and
/// `keep_chats_from_folders_archived` is ignored when
/// `keep_unmuted_chats_archived` is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArchiveChatListSettings {
    pub archive_and_mute_new_chats_from_unknown_users: bool,
    pub keep_unmuted_chats_archived: bool,
    pub keep_chats_from_folders_archived: bool,
}

/// Slice CL2: `setPinnedChats chat_list:ChatList chat_ids:vector<int53> = Ok;`
/// (schema 1.8.67, line 13681). `chat_ids` is the **full new order** of
/// pinned chats in the list (TGX `ChatsAdapter.movePinnedChat` sends the
/// reordered array the same way); `archived` selects `chatListArchive`,
/// otherwise `chatListMain`.
pub fn set_pinned_chats(extra: RequestId, archived: bool, chat_ids: &[i64]) -> String {
    json!({
        "@type": "setPinnedChats",
        "@extra": extra.as_extra(),
        "chat_list": { "@type": if archived { "chatListArchive" } else { "chatListMain" } },
        "chat_ids": chat_ids,
    })
    .to_string()
}

/// Slice CL2: `readChatList chat_list:ChatList = Ok;` (schema 1.8.67,
/// line 13684) — "Traverses all chats in a chat list and marks all
/// messages in the chats as read". `archived` selects `chatListArchive`,
/// otherwise `chatListMain`.
pub fn read_chat_list(extra: RequestId, archived: bool) -> String {
    json!({
        "@type": "readChatList",
        "@extra": extra.as_extra(),
        "chat_list": { "@type": if archived { "chatListArchive" } else { "chatListMain" } },
    })
    .to_string()
}

/// Slice CL2: `clearRecentlyFoundChats = Ok;` (schema 1.8.67, line
/// 11671). Clears the recently-found chats (the empty-search Recent
/// surface); the schema defines no update for this, so the client
/// clears its local copy optimistically (TGX `SearchManager` does the
/// same).
pub fn clear_recently_found_chats(extra: RequestId) -> String {
    json!({
        "@type": "clearRecentlyFoundChats",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice CL2: `getArchiveChatListSettings = ArchiveChatListSettings;`
/// (schema 1.8.67, line 13421).
pub fn get_archive_chat_list_settings(extra: RequestId) -> String {
    json!({
        "@type": "getArchiveChatListSettings",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice CL2: `setArchiveChatListSettings
/// settings:archiveChatListSettings = Ok;` (schema 1.8.67, line 13424).
pub fn set_archive_chat_list_settings(
    extra: RequestId,
    settings: ArchiveChatListSettings,
) -> String {
    json!({
        "@type": "setArchiveChatListSettings",
        "@extra": extra.as_extra(),
        "settings": {
            "@type": "archiveChatListSettings",
            "archive_and_mute_new_chats_from_unknown_users": settings.archive_and_mute_new_chats_from_unknown_users,
            "keep_unmuted_chats_archived": settings.keep_unmuted_chats_archived,
            "keep_chats_from_folders_archived": settings.keep_chats_from_folders_archived,
        },
    })
    .to_string()
}

/// Slice CL2: `createPrivateChat user_id:int53 force:Bool = Chat;`
/// (schema 1.8.67, line 13312). Saved Messages calls this with the own
/// user id (`getOption("my_id")`, schema line 9590); `force: false`
/// fetches the real chat.
pub fn create_private_chat(extra: RequestId, user_id: i64, force: bool) -> String {
    json!({
        "@type": "createPrivateChat",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "force": force,
    })
    .to_string()
}

/// `parity:platform-deep-links`: `getDeepLinkInfo link:string =
/// DeepLinkInfo;` (schema 1.8.67, line 16189).
pub fn get_deep_link_info(extra: RequestId, link: &str) -> String {
    json!({
        "@type": "getDeepLinkInfo",
        "@extra": extra.as_extra(),
        "link": link,
    })
    .to_string()
}

/// `getInternalLinkType link:string = InternalLinkType;` (schema 1.8.67,
/// line 13260): what a `t.me` / `tg://` link means, without opening it.
pub fn get_internal_link_type(extra: RequestId, link: &str) -> String {
    json!({
        "@type": "getInternalLinkType",
        "@extra": extra.as_extra(),
        "link": link,
    })
    .to_string()
}

/// `getMessageLinkInfo url:string = MessageLinkInfo;` (line 12073): the
/// chat, message, thread and `?t=` timestamp a message link points to.
pub fn get_message_link_info(extra: RequestId, url: &str) -> String {
    json!({
        "@type": "getMessageLinkInfo",
        "@extra": extra.as_extra(),
        "url": url,
    })
    .to_string()
}

/// `searchStickerSet name:string ignore_cache:Bool = StickerSet;` (line
/// 14681): a set by its short name, for `addstickers` / `addemoji` links.
pub fn search_sticker_set_by_name(extra: RequestId, name: &str) -> String {
    json!({
        "@type": "searchStickerSet",
        "@extra": extra.as_extra(),
        "name": name,
        "ignore_cache": false,
    })
    .to_string()
}

/// `searchUserByPhoneNumber phone_number:string only_local:Bool = User;`
/// (line 14580), for `+phone` links.
pub fn search_user_by_phone_number(extra: RequestId, phone: &str) -> String {
    json!({
        "@type": "searchUserByPhoneNumber",
        "@extra": extra.as_extra(),
        "phone_number": phone,
        "only_local": false,
    })
    .to_string()
}

/// Check an invite without joining (`schema/td_api.tl:14163`).
pub fn check_chat_invite_link(extra: RequestId, invite_link: &str) -> String {
    json!({
        "@type": "checkChatInviteLink",
        "@extra": extra.as_extra(),
        "invite_link": invite_link,
    })
    .to_string()
}

/// `parity:platform-deep-links`: `joinChatByInviteLink invite_link:string
/// = ChatJoinResult;` (schema 1.8.67, line 14166). `invite_link` is the
/// full `https://t.me/+<hash>` / `tg://join?invite=<hash>` link TDLib
/// requires.
pub fn join_chat_by_invite_link(extra: RequestId, invite_link: &str) -> String {
    json!({
        "@type": "joinChatByInviteLink",
        "@extra": extra.as_extra(),
        "invite_link": invite_link,
    })
    .to_string()
}

/// Typed `topic_id` JSON for TDLib 1.8.67 requests (schema: `MessageTopic`
/// constructors at `schema/td_api.tl:3001-3010`). `TopicId::None` encodes as
/// JSON null ("all topics"), matching the existing null-encoding convention.
pub fn topic_id_json(topic: &TopicId) -> Value {
    match topic {
        TopicId::None => Value::Null,
        TopicId::Forum { forum_topic_id } => json!({
            "@type": "messageTopicForum",
            "forum_topic_id": forum_topic_id,
        }),
        TopicId::DirectMessages {
            direct_messages_chat_topic_id,
        } => json!({
            "@type": "messageTopicDirectMessages",
            "direct_messages_chat_topic_id": direct_messages_chat_topic_id,
        }),
        TopicId::SavedMessages {
            saved_messages_topic_id,
        } => json!({
            "@type": "messageTopicSavedMessages",
            "saved_messages_topic_id": saved_messages_topic_id,
        }),
    }
}

/// `searchChatMessages` (TDLib 1.8.67). In-chat text search; returns
/// `foundChatMessages`. `topic` null-equivalent (`TopicId::None`) = all topics,
/// `sender_id` / `filter` null = any sender, all message types (tdesktop
/// ComposeSearch default). Passing `TopicId::Forum` with an empty `query`
/// lists that topic's messages — this is how per-topic history is fetched
/// (`getChatHistory` has no `topic_id` parameter, schema line 11829).
/// First page: `from_message_id` 0 (schema: last message), `offset` 0.
/// Slice media-shared-gallery: `filter` carries the tab's
/// `searchMessagesFilter*` constructor (schema/td_api.tl:11864); `None`
/// keeps the historical null-filter behavior for the existing callers.
#[allow(clippy::too_many_arguments)] // one arg per schema field, like the other request builders
pub fn search_chat_messages(
    extra: RequestId,
    chat_id: ChatId,
    topic: &TopicId,
    query: &str,
    from_message_id: MessageId,
    offset: i32,
    limit: i32,
    filter: Option<Value>,
) -> String {
    search_chat_messages_from(
        extra,
        chat_id,
        topic,
        query,
        None,
        from_message_id,
        offset,
        limit,
        filter,
    )
}

/// `searchChatMessages` restricted to one sender (`sender_id`, schema line
/// 11858) — tdesktop's "From: member" search. `None` = any sender.
#[allow(clippy::too_many_arguments)] // one arg per schema field
pub fn search_chat_messages_from(
    extra: RequestId,
    chat_id: ChatId,
    topic: &TopicId,
    query: &str,
    sender: Option<crate::telegram::envelope::MessageSender>,
    from_message_id: MessageId,
    offset: i32,
    limit: i32,
    filter: Option<Value>,
) -> String {
    json!({
        "@type": "searchChatMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": topic_id_json(topic),
        "query": query,
        "sender_id": sender.map_or(Value::Null, message_sender_json),
        "from_message_id": from_message_id.0,
        "offset": offset,
        "limit": limit,
        "filter": filter.unwrap_or(Value::Null),
    })
    .to_string()
}

/// `MessageSender` as TDLib JSON.
pub fn message_sender_json(sender: crate::telegram::envelope::MessageSender) -> Value {
    use crate::telegram::envelope::MessageSender;
    match sender {
        MessageSender::User { user_id } => {
            json!({ "@type": "messageSenderUser", "user_id": user_id })
        }
        MessageSender::Chat { chat_id } => {
            json!({ "@type": "messageSenderChat", "chat_id": chat_id })
        }
    }
}

/// `getChatMessageByDate` (schema line 11964): the last message sent no
/// later than `date`; a 404 error when there is none. Jump to date.
pub fn get_chat_message_by_date(extra: RequestId, chat_id: ChatId, date: i32) -> String {
    json!({
        "@type": "getChatMessageByDate",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "date": date,
    })
    .to_string()
}

/// `getChatMessageCalendar` (schema line 11982): messages of the filter's
/// type split by day, newest first, starting at `from_message_id` (0 = the
/// latest). TDLib rejects `searchMessagesFilterEmpty` here (400 "The
/// filter is not supported"), so a null `filter` is never sent.
pub fn get_chat_message_calendar(
    extra: RequestId,
    chat_id: ChatId,
    filter_constructor: &str,
    from_message_id: MessageId,
) -> String {
    json!({
        "@type": "getChatMessageCalendar",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "filter": search_messages_filter_json(filter_constructor),
        "from_message_id": from_message_id.0,
    })
    .to_string()
}

/// `searchMessagesFilter*` JSON for a `searchChatMessages` `filter` slot
/// (schema/td_api.tl lines 6275-6326 — every constructor takes no fields).
pub fn search_messages_filter_json(constructor: &str) -> Value {
    json!({ "@type": constructor })
}

pub fn add_chat_to_list(extra: RequestId, chat_id: ChatId, archive: bool) -> String {
    add_chat_to_list_value(
        extra,
        chat_id,
        json!({ "@type": if archive { "chatListArchive" } else { "chatListMain" } }),
    )
}

/// `addChatToList` (TDLib 1.8.67, `schema/td_api.tl:13352`) with an explicit
/// `ChatList` value. Parity slice: adding a chat to a folder uses
/// `{"@type":"chatListFolder","chat_folder_id":<id>}` — the schema doc says
/// "Use getChatListsToAddChat to get suitable chat lists".
pub fn add_chat_to_list_value(extra: RequestId, chat_id: ChatId, chat_list: Value) -> String {
    json!({
        "@type": "addChatToList",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "chat_list": chat_list
    })
    .to_string()
}

/// `getChatListsToAddChat` (TDLib 1.8.67, `schema/td_api.tl:13347`): "Returns
/// chat lists to which the chat can be added. This is an offline method".
/// Response is `chatLists`. Drives the per-chat folder picker (and the
/// archive/unarchive menu) as the schema intends.
pub fn get_chat_lists_to_add_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatListsToAddChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}
