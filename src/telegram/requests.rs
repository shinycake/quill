use crate::composer::{
    ComposerEntity, ComposerScheduling, FormatKind, PreviewMediaSize, SendOptions, find_urls,
    parse_format_markup,
};
use crate::ids::{ChatId, FileId, MessageId, RequestId, TopicId};
use crate::pins::{TDLIB_CMAKE_VERSION, TDLIB_GIT_COMMIT};
use crate::story_composer::StoryMediaKind;
use serde_json::{Value, json};

pub struct SetTdlibParameters {
    pub use_test_dc: bool,
    pub database_directory: String,
    pub files_directory: String,
    pub database_encryption_key_b64: String,
    pub api_id: i32,
    pub api_hash: String,
    pub device_model: String,
    pub system_version: String,
    pub application_version: String,
    pub system_language_code: String,
}

impl SetTdlibParameters {
    pub fn to_json(&self, extra: RequestId) -> String {
        json!({
            "@type": "setTdlibParameters",
            "@extra": extra.as_extra(),
            "use_test_dc": self.use_test_dc,
            "database_directory": self.database_directory,
            "files_directory": self.files_directory,
            "database_encryption_key": self.database_encryption_key_b64,
            "use_file_database": true,
            "use_chat_info_database": true,
            "use_message_database": true,
            "use_secret_chats": true,
            "api_id": self.api_id,
            "api_hash": self.api_hash,
            "system_language_code": self.system_language_code,
            "device_model": self.device_model,
            "system_version": self.system_version,
            "application_version": self.application_version,
        })
        .to_string()
    }
}

pub fn get_authorization_state(extra: RequestId) -> String {
    json!({
        "@type": "getAuthorizationState",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A3: `getActiveSessions = Sessions;` (TDLib 1.8.67,
/// `schema/td_api.tl:15102`): "Returns all active sessions of the current
/// user."
pub fn get_active_sessions(extra: RequestId) -> String {
    json!({
        "@type": "getActiveSessions",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A3: `terminateSession session_id:int64 = Ok;` (TDLib 1.8.67,
/// `schema/td_api.tl:15105`): "Terminates a session of the current user."
pub fn terminate_session(extra: RequestId, session_id: i64) -> String {
    json!({
        "@type": "terminateSession",
        "@extra": extra.as_extra(),
        "session_id": session_id,
    })
    .to_string()
}

/// Slice A3: `terminateAllOtherSessions = Ok;` (TDLib 1.8.67,
/// `schema/td_api.tl:15108`): "Terminates all other sessions of the
/// current user."
pub fn terminate_all_other_sessions(extra: RequestId) -> String {
    json!({
        "@type": "terminateAllOtherSessions",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `setAuthenticationPhoneNumber`. Callers must not log `phone_number`.
pub fn set_authentication_phone_number(extra: RequestId, phone_number: &str) -> String {
    json!({
        "@type": "setAuthenticationPhoneNumber",
        "@extra": extra.as_extra(),
        "phone_number": phone_number,
        "settings": {
            "@type": "phoneNumberAuthenticationSettings",
            "allow_flash_call": false,
            "allow_missed_call": false,
            "is_current_phone_number": false,
            "has_unknown_phone_number": false,
            "allow_sms_retriever_api": false,
            "firebase_authentication_settings": Value::Null,
            "authentication_tokens": []
        }
    })
    .to_string()
}

/// `checkAuthenticationCode`. Callers must not log `code`.
pub fn check_authentication_code(extra: RequestId, code: &str) -> String {
    json!({
        "@type": "checkAuthenticationCode",
        "@extra": extra.as_extra(),
        "code": code,
    })
    .to_string()
}

/// `checkAuthenticationPassword`. Callers must not log `password`.
pub fn check_authentication_password(extra: RequestId, password: &str) -> String {
    json!({
        "@type": "checkAuthenticationPassword",
        "@extra": extra.as_extra(),
        "password": password,
    })
    .to_string()
}

/// `resendAuthenticationCode` for the auth flow (NOT `resendPhoneNumberCode`,
/// which belongs to the phone-number-verification flow). Reason is the
/// user-initiated one; TDLib enforces its own server-side cooldown (429 on
/// too-early resend), so no local countdown is invented.
pub fn resend_authentication_code(extra: RequestId) -> String {
    json!({
        "@type": "resendAuthenticationCode",
        "@extra": extra.as_extra(),
        "reason": { "@type": "resendCodeReasonUserRequest" },
    })
    .to_string()
}

/// `requestQrCodeAuthentication`. `other_user_ids` is empty: this client has
/// no other logged-in user to hint at.
pub fn request_qr_code_authentication(extra: RequestId) -> String {
    json!({
        "@type": "requestQrCodeAuthentication",
        "@extra": extra.as_extra(),
        "other_user_ids": [],
    })
    .to_string()
}

/// Slice A2: `getPasswordState` (TDLib 1.8.67, `schema/td_api.tl:11426`):
/// "Returns the current state of 2-step verification".
pub fn get_password_state(extra: RequestId) -> String {
    json!({
        "@type": "getPasswordState",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A2: `setPassword` (TDLib 1.8.67, `schema/td_api.tl:11434`):
/// `setPassword old_password:string new_password:string new_hint:string
/// set_recovery_email_address:Bool new_recovery_email_address:string =
/// PasswordState;`
/// "Changes the 2-step verification password for the current user. If a
/// new recovery email address is specified, then the change will not be
/// applied until the new recovery email address is confirmed".
/// Empty `new_password` removes the password; `old_password` is empty
/// when enabling for the first time (TGX `PasswordController` sends null
/// in MODE_NEW). Callers must not log `old_password`/`new_password`.
pub fn set_password(
    extra: RequestId,
    old_password: &str,
    new_password: &str,
    new_hint: &str,
    new_recovery_email_address: Option<&str>,
) -> String {
    json!({
        "@type": "setPassword",
        "@extra": extra.as_extra(),
        "old_password": old_password,
        "new_password": new_password,
        "new_hint": new_hint,
        "set_recovery_email_address": new_recovery_email_address.is_some(),
        "new_recovery_email_address": new_recovery_email_address.unwrap_or(""),
    })
    .to_string()
}

/// Slice A2: `setRecoveryEmailAddress` (TDLib 1.8.67,
/// `schema/td_api.tl:11458`): "Changes the 2-step verification recovery
/// email address of the user. If a new recovery email address is
/// specified, then the change will not be applied until the new recovery
/// email address is confirmed." Callers must not log `password`.
pub fn set_recovery_email_address(
    extra: RequestId,
    password: &str,
    new_recovery_email_address: &str,
) -> String {
    json!({
        "@type": "setRecoveryEmailAddress",
        "@extra": extra.as_extra(),
        "password": password,
        "new_recovery_email_address": new_recovery_email_address,
    })
    .to_string()
}

/// Slice A2: `resendRecoveryEmailAddressCode` (TDLib 1.8.67,
/// `schema/td_api.tl:11464`): "Resends the 2-step verification recovery
/// email address verification code". TDLib enforces its own server-side
/// cooldown (429 on too-early resend), so no local countdown is invented.
pub fn resend_recovery_email_address_code(extra: RequestId) -> String {
    json!({
        "@type": "resendRecoveryEmailAddressCode",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A2: `cancelRecoveryEmailAddressVerification` (TDLib 1.8.67,
/// `schema/td_api.tl:11467`): "Cancels verification of the 2-step
/// verification recovery email address".
pub fn cancel_recovery_email_address_verification(extra: RequestId) -> String {
    json!({
        "@type": "cancelRecoveryEmailAddressVerification",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

pub fn close_request(extra: RequestId) -> String {
    json!({
        "@type": "close",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

pub fn log_out(extra: RequestId) -> String {
    json!({
        "@type": "logOut",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

pub fn load_chats(extra: RequestId, limit: i32) -> String {
    load_chats_list(extra, json!({ "@type": "chatListMain" }), limit)
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

/// `searchMessages` (TDLib 1.8.67). `chat_list` null = all lists (official
/// clients / Unigram); schema: only Main and Archive are searchable.
/// `filter` / `chat_type_filter` null = all messages / all chat types.
pub fn search_messages(extra: RequestId, query: &str, limit: i32) -> String {
    json!({
        "@type": "searchMessages",
        "@extra": extra.as_extra(),
        "chat_list": Value::Null,
        "query": query,
        "offset": "",
        "limit": limit,
        "filter": Value::Null,
        "chat_type_filter": Value::Null,
        "min_date": 0,
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
pub fn search_chat_messages(
    extra: RequestId,
    chat_id: ChatId,
    topic: &TopicId,
    query: &str,
    from_message_id: MessageId,
    offset: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "searchChatMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": topic_id_json(topic),
        "query": query,
        "sender_id": Value::Null,
        "from_message_id": from_message_id.0,
        "offset": offset,
        "limit": limit,
        "filter": Value::Null,
    })
    .to_string()
}

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

/// `addRecentlyFoundChat` (TDLib 1.8.67). Official clients send this on select.
pub fn add_recently_found_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "addRecentlyFoundChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

pub fn get_chat_history(
    extra: RequestId,
    chat_id: ChatId,
    from_message_id: MessageId,
    offset: i32,
    limit: i32,
    only_local: bool,
) -> String {
    json!({
        "@type": "getChatHistory",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "from_message_id": from_message_id.0,
        "offset": offset,
        "limit": limit,
        "only_local": only_local,
    })
    .to_string()
}

/// `openChat` — required before `viewMessages` can mark history as read.
pub fn open_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "openChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `closeChat` when leaving a conversation. Distinct from client `close`.
pub fn close_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "closeChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `getMe` (TDLib 1.8.67). Sent once so `getChatMember` can resolve the
/// current user; only the response id is kept.
pub fn get_me(extra: RequestId) -> String {
    json!({
        "@type": "getMe",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `getChatMember` for the current user in a channel (TDLib 1.8.67). Response
/// is `chatMember`.
pub fn get_chat_member(extra: RequestId, chat_id: ChatId, user_id: i64) -> String {
    json!({
        "@type": "getChatMember",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "member_id": { "@type": "messageSenderUser", "user_id": user_id },
    })
    .to_string()
}

/// `getUserFullInfo` for a bot user (TDLib 1.8.67,
/// `getUserFullInfo user_id:int53 = UserFullInfo`). Response is
/// `userFullInfo`; `bot_info` feeds the bot panel.
pub fn get_user_full_info(extra: RequestId, user_id: i64) -> String {
    json!({
        "@type": "getUserFullInfo",
        "@extra": extra.as_extra(),
        "user_id": user_id,
    })
    .to_string()
}

/// Phase 6: `getContacts` (TDLib 1.8.67, `schema/td_api.tl:14520`):
/// `getContacts = Users;` — no parameters. Response is `users`
/// (`total_count:int32 user_ids:vector<int53>`, line 2471); the user
/// objects themselves arrive via `updateUser`.
pub fn get_contacts(extra: RequestId) -> String {
    json!({
        "@type": "getContacts",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Phase 6: `addContact` (TDLib 1.8.67, `schema/td_api.tl:14513`):
/// `addContact user_id:int53 contact:importedContact
/// share_phone_number:Bool = Ok;`
/// with `importedContact phone_number:string first_name:string
/// last_name:string note:formattedText = ImportedContact` (line 7382).
/// The note is sent as an empty `formattedText` — Quill has no contact
/// notes UI.
pub fn add_contact(
    extra: RequestId,
    user_id: i64,
    phone_number: &str,
    first_name: &str,
    last_name: &str,
) -> String {
    json!({
        "@type": "addContact",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "contact": {
            "@type": "importedContact",
            "phone_number": phone_number,
            "first_name": first_name,
            "last_name": last_name,
            "note": { "@type": "formattedText", "text": "", "entities": [] },
        },
        "share_phone_number": false,
    })
    .to_string()
}

/// Phase 6: `getSupergroupFullInfo` (TDLib 1.8.67,
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

/// Slice G1: `setChatMemberTag` (TDLib 1.8.67, `schema/td_api.tl:13598`):
/// `setChatMemberTag chat_id:int53 user_id:int53 tag:string = Ok;`
/// "Changes the tag or custom title of a chat member" — this is the
/// admin custom-title setter (Telegram X `EditRightsController` sets
/// the "Custom title" field through it; 0-16 characters, no emoji).
/// Basic groups and supergroups only, not channels; requires
/// `can_manage_tags` to change another member's tag.
pub fn set_chat_member_tag(extra: RequestId, chat_id: ChatId, user_id: i64, tag: &str) -> String {
    json!({
        "@type": "setChatMemberTag",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "user_id": user_id,
        "tag": tag,
    })
    .to_string()
}

/// Slice G1: `deleteChat` (TDLib 1.8.67, `schema/td_api.tl:11850`):
/// `deleteChat chat_id:int53 = Ok;`
/// "Deletes a chat along with all messages in the corresponding chat for
/// all chat members. For group chats this will release the usernames and
/// remove all members. Use the field chat.can_be_deleted_for_all_users to
/// find whether the method can be applied to the chat".
pub fn delete_chat(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "deleteChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// Slice CL1: `toggleChatIsPinned` (TDLib 1.8.67, `schema/td_api.tl:13678`):
/// `toggleChatIsPinned chat_list:ChatList chat_id:int53 is_pinned:Bool = Ok;`
/// The pinned state is per list — `archived` selects `chatListArchive`,
/// otherwise `chatListMain` (pinning is only defined for main/archive per
/// the schema doc).
pub fn toggle_chat_is_pinned(
    extra: RequestId,
    chat_id: i64,
    archived: bool,
    is_pinned: bool,
) -> String {
    json!({
        "@type": "toggleChatIsPinned",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "chat_list": { "@type": if archived { "chatListArchive" } else { "chatListMain" } },
        "is_pinned": is_pinned,
    })
    .to_string()
}

/// Slice CL1: `toggleChatIsMarkedAsUnread` (TDLib 1.8.67,
/// `schema/td_api.tl:13519`):
/// `toggleChatIsMarkedAsUnread chat_id:int53 is_marked_as_unread:Bool = Ok;`
pub fn toggle_chat_is_marked_as_unread(
    extra: RequestId,
    chat_id: i64,
    is_marked_as_unread: bool,
) -> String {
    json!({
        "@type": "toggleChatIsMarkedAsUnread",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "is_marked_as_unread": is_marked_as_unread,
    })
    .to_string()
}

/// Slice CL1: `deleteChatHistory` (TDLib 1.8.67, `schema/td_api.tl:11845`):
/// `deleteChatHistory chat_id:int53 remove_from_chat_list:Bool revoke:Bool = Ok;`
/// The chat stays in the chat list (`remove_from_chat_list: false`, TGX
/// clear-history behavior); `revoke` clears for everyone when
/// `chat.can_be_deleted_for_all_users`.
pub fn delete_chat_history(
    extra: RequestId,
    chat_id: i64,
    remove_from_chat_list: bool,
    revoke: bool,
) -> String {
    json!({
        "@type": "deleteChatHistory",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "remove_from_chat_list": remove_from_chat_list,
        "revoke": revoke,
    })
    .to_string()
}

/// Slice G1: `inputTextQuote` JSON (TDLib 1.8.67, `schema/td_api.tl:3056`):
/// `inputTextQuote text:formattedText position:int32 = InputTextQuote;`
/// `position` is the offset of the quoted text in the original message in
/// UTF-16 code units.
pub fn input_text_quote_json(text: &str, position: i32) -> Value {
    json!({
        "@type": "inputTextQuote",
        "text": {
            "@type": "formattedText",
            "text": text,
            "entities": []
        },
        "position": position,
    })
}

/// Slice G1: reply target for the send builders — a message id plus an
/// optional validated partial quote (`inputTextQuote`, schema 1.8.67
/// line 3056). Replaces bare `Option<MessageId>` wherever a send can
/// carry a quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReply {
    pub message_id: MessageId,
    pub quote: Option<(String, i32)>,
}

impl SendReply {
    pub fn plain(message_id: MessageId) -> Self {
        Self {
            message_id,
            quote: None,
        }
    }
}

/// Slice G1: `reply_to` JSON for a send builder — delegates to
/// `input_message_reply_to_with_quote` so whole-message replies keep
/// the exact shape the old `input_message_reply_to` produced.
fn send_reply_value(reply_to: Option<&SendReply>) -> Value {
    input_message_reply_to_with_quote(
        reply_to.map(|reply| reply.message_id),
        reply_to.and_then(|reply| {
            reply
                .quote
                .as_ref()
                .map(|(text, position)| (text.as_str(), *position))
        }),
    )
}

/// Slice G1: same-chat reply with an optional quote (TDLib 1.8.67,
/// `schema/td_api.tl:3086`):
/// `inputMessageReplyToMessage message_id:int53 quote:inputTextQuote
/// checklist_task_id:int32 poll_option_id:string = InputMessageReplyTo;`
/// `quote` is null for a whole-message reply.
pub fn input_message_reply_to_with_quote(
    message_id: Option<MessageId>,
    quote: Option<(&str, i32)>,
) -> Value {
    match message_id {
        None => Value::Null,
        Some(id) => json!({
            "@type": "inputMessageReplyToMessage",
            "message_id": id.0,
            "quote": quote.map(|(text, position)| input_text_quote_json(text, position)).unwrap_or(Value::Null),
            "checklist_task_id": 0,
            "poll_option_id": ""
        }),
    }
}

/// Phase D3c: `chatEventLogFilters` (TDLib 1.8.67,
/// `schema/td_api.tl:7956`). Mirrors the schema field order exactly.
/// Slice G2: the event-log section's filter chips edit a per-chat set;
/// when no category is enabled the driver passes `null` (all event
/// types — the schema's "pass null to get chat events of all types",
/// line 15252).
#[derive(Debug, Clone, Copy, Default)]
pub struct ChatEventLogFilterSet {
    pub message_edits: bool,
    pub message_deletions: bool,
    pub message_pins: bool,
    pub member_joins: bool,
    pub member_leaves: bool,
    pub member_invites: bool,
    pub member_promotions: bool,
    pub member_restrictions: bool,
    pub member_tag_changes: bool,
    pub info_changes: bool,
    pub setting_changes: bool,
    pub invite_link_changes: bool,
    pub video_chat_changes: bool,
    pub forum_changes: bool,
    pub subscription_extensions: bool,
}

impl ChatEventLogFilterSet {
    /// Slice G2: true when at least one category is enabled. The driver
    /// sends `null` (all event types) when none are, mirroring the
    /// schema's "pass null to get chat events of all types" (1.8.67,
    /// line 15252).
    pub fn any_enabled(self) -> bool {
        self.message_edits
            || self.message_deletions
            || self.message_pins
            || self.member_joins
            || self.member_leaves
            || self.member_invites
            || self.member_promotions
            || self.member_restrictions
            || self.member_tag_changes
            || self.info_changes
            || self.setting_changes
            || self.invite_link_changes
            || self.video_chat_changes
            || self.forum_changes
            || self.subscription_extensions
    }

    fn to_json(self) -> Value {
        json!({
            "@type": "chatEventLogFilters",
            "message_edits": self.message_edits,
            "message_deletions": self.message_deletions,
            "message_pins": self.message_pins,
            "member_joins": self.member_joins,
            "member_leaves": self.member_leaves,
            "member_invites": self.member_invites,
            "member_promotions": self.member_promotions,
            "member_restrictions": self.member_restrictions,
            "member_tag_changes": self.member_tag_changes,
            "info_changes": self.info_changes,
            "setting_changes": self.setting_changes,
            "invite_link_changes": self.invite_link_changes,
            "video_chat_changes": self.video_chat_changes,
            "forum_changes": self.forum_changes,
            "subscription_extensions": self.subscription_extensions,
        })
    }
}

/// Phase D3c: `getChatEventLog` (TDLib 1.8.67, `schema/td_api.tl:15252`):
/// `getChatEventLog chat_id:int53 query:string from_event_id:int64 limit:int32 filters:chatEventLogFilters user_ids:vector<int53> = ChatEvents;`
/// "Returns a list of service actions taken by chat members and
/// administrators in the last 48 hours. Available only in supergroups and
/// channels. Requires administrator rights. Returns results in reverse
/// chronological order (i.e., in order of decreasing event_id)".
/// `from_event_id` 0 starts from the latest events; `filters` `None` is
/// the schema's `null` = all event types.
pub fn get_chat_event_log(
    extra: RequestId,
    chat_id: i64,
    query: &str,
    from_event_id: i64,
    limit: i32,
    filters: Option<ChatEventLogFilterSet>,
    user_ids: &[i64],
) -> String {
    json!({
        "@type": "getChatEventLog",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "query": query,
        "from_event_id": from_event_id,
        "limit": limit,
        "filters": filters.map(|set| set.to_json()).unwrap_or(Value::Null),
        "user_ids": user_ids,
    })
    .to_string()
}

/// Phase B1: `createNewSecretChat` (TDLib 1.8.67, `schema/td_api.tl:13340`):
/// `createNewSecretChat user_id:int53 = Chat;`
/// "Creates a new secret chat. Returns the newly created chat". The new
/// chat also arrives as `updateNewChat` with `chatTypeSecret`.
pub fn create_new_secret_chat(extra: RequestId, user_id: i64) -> String {
    json!({
        "@type": "createNewSecretChat",
        "@extra": extra.as_extra(),
        "user_id": user_id,
    })
    .to_string()
}

/// Phase B1: `getSecretChat` (TDLib 1.8.67, `schema/td_api.tl:11516`):
/// `getSecretChat secret_chat_id:int32 = SecretChat;`
/// "Returns information about a secret chat by its identifier. This is an
/// offline method" — used to learn the initial state of a secret chat
/// whose `updateSecretChat` was never seen (e.g. loaded from the local DB).
pub fn get_secret_chat(extra: RequestId, secret_chat_id: i32) -> String {
    json!({
        "@type": "getSecretChat",
        "@extra": extra.as_extra(),
        "secret_chat_id": secret_chat_id,
    })
    .to_string()
}

/// Phase B1: `closeSecretChat` (TDLib 1.8.67, `schema/td_api.tl:15242`):
/// `closeSecretChat secret_chat_id:int32 = Ok;`
/// "Closes a secret chat, effectively transferring its state to
/// secretChatStateClosed". The state change itself arrives as
/// `updateSecretChat`.
pub fn close_secret_chat(extra: RequestId, secret_chat_id: i32) -> String {
    json!({
        "@type": "closeSecretChat",
        "@extra": extra.as_extra(),
        "secret_chat_id": secret_chat_id,
    })
    .to_string()
}

/// Phase S1: `toggleSessionCanAcceptSecretChats` (TDLib 1.8.67,
/// `schema/td_api.tl:15117`):
/// `toggleSessionCanAcceptSecretChats session_id:int64 can_accept_secret_chats:Bool = Ok;`
/// Per-session toggle — the session accepts (or rejects) new secret chats.
/// TGX surfaces it in the session editor ("Secret Chats" Accept/Reject,
/// `EditSessionController`); Quill has no sessions screen yet, so this is
/// request-layer only until one lands.
pub fn toggle_session_can_accept_secret_chats(
    extra: RequestId,
    session_id: i64,
    can_accept_secret_chats: bool,
) -> String {
    json!({
        "@type": "toggleSessionCanAcceptSecretChats",
        "@extra": extra.as_extra(),
        "session_id": session_id.to_string(),
        "can_accept_secret_chats": can_accept_secret_chats,
    })
    .to_string()
}

/// Phase S2: `getStorageStatistics` (TDLib 1.8.67,
/// `schema/td_api.tl:15781`):
/// `getStorageStatistics chat_limit:int32 = StorageStatistics;`
/// Drives the storage-usage overlay, including the "Secret media and
/// files" category (`fileTypeSecret`, td_api.tl:9728 — "The file was
/// sent to a secret chat (the file type is not known to the server)").
/// `chat_limit` 0 is honest here: the overlay aggregates by file type
/// across chats, so per-chat splits are not needed.
pub fn get_storage_statistics(extra: RequestId, chat_limit: i32) -> String {
    json!({
        "@type": "getStorageStatistics",
        "@extra": extra.as_extra(),
        "chat_limit": chat_limit,
    })
    .to_string()
}

/// Phase C1: the `callProtocol` Quill advertises for signaling-only
/// calls (TDLib 1.8.67, `schema/td_api.tl:7008`):
/// `callProtocol udp_p2p:Bool udp_reflector:Bool min_layer:int32
/// max_layer:int32 library_versions:vector<string> = CallProtocol;`
/// The schema pins `min_layer = 65` / `max_layer = 92`. Quill claims
/// **no media capability** (`udp_p2p: false`, `udp_reflector: false`,
/// no tgcalls `library_versions`) — honest, because this slice has no
/// VoIP transport (TDLib does not move audio/video; official clients
/// use libtgvoip, the C2 spike). The remote side will see us as
/// "connecting" until it gives up; the UI says so explicitly.
pub fn call_protocol() -> Value {
    json!({
        "@type": "callProtocol",
        "udp_p2p": false,
        "udp_reflector": false,
        "min_layer": 65,
        "max_layer": 92,
        "library_versions": []
    })
}

/// Phase C1: `createCall` (TDLib 1.8.67, `schema/td_api.tl:14212`):
/// `createCall user_id:int53 protocol:callProtocol is_video:Bool =
/// CallId;` "Creates a new call". Phase C1b: `is_video: true` is
/// allowed — it starts video-call *signaling*; media transport is
/// still Phase C2, so the call carries no audio or video.
pub fn create_call(extra: RequestId, user_id: i64, is_video: bool) -> String {
    create_call_with_protocol(extra, user_id, is_video, &call_protocol())
}

/// Phase C2b: `createCall` with the protocol reported by the loaded engine.
pub fn create_call_with_protocol(
    extra: RequestId,
    user_id: i64,
    is_video: bool,
    protocol: &Value,
) -> String {
    json!({
        "@type": "createCall",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "protocol": protocol,
        "is_video": is_video,
    })
    .to_string()
}

/// Phase C1: `acceptCall` (TDLib 1.8.67, `schema/td_api.tl:14215`):
/// `acceptCall call_id:int32 protocol:callProtocol = Ok;`
/// "Accepts an incoming call".
pub fn accept_call(extra: RequestId, call_id: i32) -> String {
    accept_call_with_protocol(extra, call_id, &call_protocol())
}

/// Phase C2b: `acceptCall` with the protocol reported by the loaded engine.
pub fn accept_call_with_protocol(extra: RequestId, call_id: i32, protocol: &Value) -> String {
    json!({
        "@type": "acceptCall",
        "@extra": extra.as_extra(),
        "call_id": call_id,
        "protocol": protocol,
    })
    .to_string()
}

/// Phase C2b: forward engine-emitted signaling through TDLib
/// (`sendCallSignalingData`, `schema/td_api.tl:14218`). TDLib JSON `bytes`
/// fields use standard base64.
pub fn send_call_signaling_data(extra: RequestId, call_id: i32, data: &[u8]) -> String {
    use base64::Engine;
    json!({
        "@type": "sendCallSignalingData",
        "@extra": extra.as_extra(),
        "call_id": call_id,
        "data": base64::engine::general_purpose::STANDARD.encode(data),
    })
    .to_string()
}

/// Phase C1: `discardCall` (TDLib 1.8.67, `schema/td_api.tl:14227`):
/// `discardCall call_id:int32 is_disconnected:Bool invite_link:string
/// duration:int32 is_video:Bool connection_id:int64 = Ok;`
/// `duration` is the connected time in seconds (0 when the call never
/// reached `callStateReady`); `invite_link` is empty and
/// `connection_id` is 0 because there is no media connection yet (C2).
pub fn discard_call(
    extra: RequestId,
    call_id: i32,
    is_disconnected: bool,
    duration_secs: i32,
    is_video: bool,
) -> String {
    json!({
        "@type": "discardCall",
        "@extra": extra.as_extra(),
        "call_id": call_id,
        "is_disconnected": is_disconnected,
        "invite_link": "",
        "duration": duration_secs,
        "is_video": is_video,
        "connection_id": 0,
    })
    .to_string()
}

/// Phase C1: `sendCallRating` (TDLib 1.8.67, `schema/td_api.tl:14234`):
/// `sendCallRating call_id:InputCall rating:int32 comment:string
/// problems:vector<CallProblem> = Ok;` "Sends a call rating". The call
/// has ended, so the call is identified with `inputCallDiscarded`.
/// The star tap opens the rating detail editor (C2i:
/// `open_rating_detail` / `send_call_rating_detail`) instead of
/// calling this directly; this kept helper covers the
/// no-problems no-comment shape used in tests.
pub fn send_call_rating(extra: RequestId, call_id: i32, rating: i32) -> String {
    send_call_rating_detail(extra, call_id, rating, "", &[])
}

/// Phase C2d: `sendCallDebugInformation` (TDLib 1.8.67,
/// `schema/td_api.tl:14237`) identifies the ended call with
/// `inputCallDiscarded` (`schema/td_api.tl:7043`).
pub fn send_call_debug_information(
    extra: RequestId,
    call_id: i32,
    debug_information: &str,
) -> String {
    json!({
        "@type": "sendCallDebugInformation",
        "@extra": extra.as_extra(),
        "call_id": {
            "@type": "inputCallDiscarded",
            "call_id": call_id,
        },
        "debug_information": debug_information,
    })
    .to_string()
}

/// Phase C2i: `sendCallRating` with the full detail (TDLib 1.8.67,
/// `schema/td_api.tl:14234`):
/// `sendCallRating call_id:InputCall rating:int32 comment:string
/// problems:vector<CallProblem> = Ok;`
/// "comment: An optional user comment if the rating is less than 5;
/// problems: List of the exact types of problems with the call,
/// specified by the user". `problems` are `CallProblem` constructor
/// names (`callProblemEcho`, …, schema `:7253`-`:7277`); the call has
/// ended so it is identified with `inputCallDiscarded` (:7043).
pub fn send_call_rating_detail(
    extra: RequestId,
    call_id: i32,
    rating: i32,
    comment: &str,
    problems: &[&str],
) -> String {
    json!({
        "@type": "sendCallRating",
        "@extra": extra.as_extra(),
        "call_id": {
            "@type": "inputCallDiscarded",
            "call_id": call_id,
        },
        "rating": rating.clamp(1, 5),
        "comment": comment,
        "problems": problems.iter().map(|name| json!({"@type": name})).collect::<Vec<_>>(),
    })
    .to_string()
}

/// Phase C2i: `sendCallLog` (TDLib 1.8.67, `schema/td_api.tl:14240`):
/// `sendCallLog call_id:InputCall log_file:InputFile = Ok;`
/// "Only inputFileLocal and inputFileGenerated are supported".
pub fn send_call_log(extra: RequestId, call_id: i32, log_path: &str) -> String {
    json!({
        "@type": "sendCallLog",
        "@extra": extra.as_extra(),
        "call_id": {
            "@type": "inputCallDiscarded",
            "call_id": call_id,
        },
        "log_file": {
            "@type": "inputFileLocal",
            "path": log_path,
        },
    })
    .to_string()
}

/// Phase C2i: `searchCallMessages` (TDLib 1.8.67,
/// `schema/td_api.tl:11903`): "Searches for call and group call
/// messages. Returns the results in reverse chronological order".
/// `searchCallMessages offset:string limit:int32 only_missed:Bool =
/// FoundMessages;`
pub fn search_call_messages(extra: RequestId, offset: &str, limit: i32) -> String {
    json!({
        "@type": "searchCallMessages",
        "@extra": extra.as_extra(),
        "offset": offset,
        "limit": limit,
        "only_missed": false,
    })
    .to_string()
}

/// Phase C2i: which call privacy setting a request targets —
/// `userPrivacySettingAllowCalls` (who can call me, schema 1.8.67
/// `:9006`) or `userPrivacySettingAllowPeerToPeerCalls` (P2P relay,
/// `:9009`). Both are standard privacy settings (Telegram X lists
/// both in its privacy screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPrivacySetting {
    AllowCalls,
    PeerToPeer,
}

impl CallPrivacySetting {
    pub fn td_type(self) -> &'static str {
        match self {
            CallPrivacySetting::AllowCalls => "userPrivacySettingAllowCalls",
            CallPrivacySetting::PeerToPeer => "userPrivacySettingAllowPeerToPeerCalls",
        }
    }
}

/// Phase C2i: Everybody / Contacts / Nobody mapping for the two call
/// privacy settings. Telegram clients expose these as rule lists;
/// the three simple cases are `[AllowAll]`, `[AllowContacts]`,
/// `[RestrictAll]` (schema 1.8.67, `:8943`-`:8964`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyWho {
    Everybody,
    Contacts,
    Nobody,
}

impl PrivacyWho {
    /// The `userPrivacySettingRules` JSON for this choice.
    pub fn rules(self) -> Vec<Value> {
        let name = match self {
            PrivacyWho::Everybody => "userPrivacySettingRuleAllowAll",
            PrivacyWho::Contacts => "userPrivacySettingRuleAllowContacts",
            PrivacyWho::Nobody => "userPrivacySettingRuleRestrictAll",
        };
        vec![json!({"@type": name})]
    }

    /// Map server-returned rule constructor names back to the simple
    /// choice. Exception rules (`AllowUsers` / `RestrictUsers` / ...)
    /// are ignored: the list collapses to the first recognizable base
    /// rule in priority order (AllowAll > RestrictAll > AllowContacts);
    /// `None` only when no base rule is present (empty or fully custom
    /// lists the three-option UI cannot represent).
    pub fn from_rule_names(names: &[String]) -> Option<Self> {
        if names.iter().any(|n| n == "userPrivacySettingRuleAllowAll") {
            Some(PrivacyWho::Everybody)
        } else if names
            .iter()
            .any(|n| n == "userPrivacySettingRuleRestrictAll")
        {
            Some(PrivacyWho::Nobody)
        } else if names
            .iter()
            .any(|n| n == "userPrivacySettingRuleAllowContacts")
        {
            Some(PrivacyWho::Contacts)
        } else {
            None
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PrivacyWho::Everybody => "Everybody",
            PrivacyWho::Contacts => "My contacts",
            PrivacyWho::Nobody => "Nobody",
        }
    }
}

/// Phase C2i: `getUserPrivacySettingRules` (TDLib 1.8.67,
/// `schema/td_api.tl:15620`): "Returns the current privacy settings".
/// `getUserPrivacySettingRules setting:UserPrivacySetting =
/// UserPrivacySettingRules;`
pub fn get_user_privacy_setting_rules(extra: RequestId, setting: CallPrivacySetting) -> String {
    json!({
        "@type": "getUserPrivacySettingRules",
        "@extra": extra.as_extra(),
        "setting": {"@type": setting.td_type()},
    })
    .to_string()
}

/// Phase C2i: `setUserPrivacySettingRules` (TDLib 1.8.67,
/// `schema/td_api.tl:15617`): "Changes user privacy settings".
/// `setUserPrivacySettingRules setting:UserPrivacySetting
/// rules:userPrivacySettingRules = Ok;`
pub fn set_user_privacy_setting_rules(
    extra: RequestId,
    setting: CallPrivacySetting,
    who: PrivacyWho,
) -> String {
    json!({
        "@type": "setUserPrivacySettingRules",
        "@extra": extra.as_extra(),
        "setting": {"@type": setting.td_type()},
        "rules": {"@type": "userPrivacySettingRules", "rules": who.rules()},
    })
    .to_string()
}

/// Phase C3a: a `MessageSender` reference for group-call request
/// fields (e.g. `toggleGroupCallParticipantIsMuted participant_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSenderRef {
    User(i64),
    Chat(i64),
}

impl MessageSenderRef {
    /// Renders `messageSenderUser` (TDLib 1.8.67,
    /// `schema/td_api.tl:2831`) / `messageSenderChat`
    /// (`schema/td_api.tl:2834`), matching the inline convention
    /// used elsewhere in this file.
    pub fn to_value(&self) -> Value {
        match *self {
            MessageSenderRef::User(user_id) => {
                json!({ "@type": "messageSenderUser", "user_id": user_id })
            }
            MessageSenderRef::Chat(chat_id) => {
                json!({ "@type": "messageSenderChat", "chat_id": chat_id })
            }
        }
    }
}

/// Phase C3a: an `InputGroupCall` reference for non-chat-bound group
/// calls (TDLib 1.8.67, `schema/td_api.tl:7242` /
/// `schema/td_api.tl:7247`):
/// `inputGroupCallLink link:string = InputGroupCall;`
/// `inputGroupCallMessage chat_id:int53 message_id:int53 =
/// InputGroupCall;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputGroupCallRef {
    Link(String),
    Message { chat_id: i64, message_id: i64 },
}

impl InputGroupCallRef {
    pub fn to_value(&self) -> Value {
        match self {
            InputGroupCallRef::Link(link) => {
                json!({ "@type": "inputGroupCallLink", "link": link })
            }
            InputGroupCallRef::Message {
                chat_id,
                message_id,
            } => {
                json!({
                    "@type": "inputGroupCallMessage",
                    "chat_id": chat_id,
                    "message_id": message_id,
                })
            }
        }
    }
}

/// Phase C3a: `groupCallJoinParameters` (TDLib 1.8.67,
/// `schema/td_api.tl:7089`):
/// `groupCallJoinParameters audio_source_id:int32 payload:string
/// is_muted:Bool is_my_video_enabled:Bool = GroupCallJoinParameters;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCallJoinParams {
    pub audio_source_id: i32,
    pub payload: String,
    pub is_muted: bool,
    pub is_my_video_enabled: bool,
}

impl GroupCallJoinParams {
    /// The honest no-device join: `audio_source_id` is 0 and `payload`
    /// is empty. Used when no call engine is available or the native
    /// offer fails — TDLib accepts these and the join still goes out;
    /// Phase C2g normally replaces this with the real tgcalls offer.
    pub fn honest_no_device() -> Self {
        GroupCallJoinParams {
            audio_source_id: 0,
            payload: String::new(),
            is_muted: false,
            is_my_video_enabled: false,
        }
    }

    pub fn to_value(&self) -> Value {
        json!({
            "@type": "groupCallJoinParameters",
            "audio_source_id": self.audio_source_id,
            "payload": self.payload,
            "is_muted": self.is_muted,
            "is_my_video_enabled": self.is_my_video_enabled,
        })
    }
}

/// Phase C3a: `createVideoChat` (TDLib 1.8.67,
/// `schema/td_api.tl:14256`):
/// `createVideoChat chat_id:int53 title:string start_date:int32
/// is_rtmp_stream:Bool = GroupCallId;`
/// This is the chat-bound voice/video-chat creation path (groups and
/// channels). An immediate voice chat passes `start_date: 0` and
/// `is_rtmp_stream: false`. (`createGroupCall` is for group calls that
/// *aren't* bound to a chat.)
pub fn create_video_chat(
    extra: RequestId,
    chat_id: i64,
    title: &str,
    start_date: i32,
    is_rtmp_stream: bool,
) -> String {
    json!({
        "@type": "createVideoChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "title": title,
        "start_date": start_date,
        "is_rtmp_stream": is_rtmp_stream,
    })
    .to_string()
}

/// Phase C3a: `createGroupCall` (TDLib 1.8.67,
/// `schema/td_api.tl:14259`):
/// `createGroupCall join_parameters:groupCallJoinParameters =
/// GroupCallInfo;`
/// Creates a group call that isn't bound to a chat. Per the schema
/// docs, pass null `join_parameters` to only create the call link
/// without joining the call.
pub fn create_group_call(extra: RequestId, join_params: Option<&GroupCallJoinParams>) -> String {
    json!({
        "@type": "createGroupCall",
        "@extra": extra.as_extra(),
        "join_parameters": join_params.map(|p| p.to_value()).unwrap_or(Value::Null),
    })
    .to_string()
}

/// Phase C3a: `joinVideoChat` (TDLib 1.8.67,
/// `schema/td_api.tl:14292`):
/// `joinVideoChat group_call_id:int32 participant_id:MessageSender
/// join_parameters:groupCallJoinParameters invite_hash:string = Text;`
/// "Joins an active video chat. Returns join response payload for
/// tgcalls". `participant_id: None` serializes null (join as self).
/// The returned payload is stored by the driver and never consumed —
/// there is no media transport until Phase C2.
pub fn join_video_chat(
    extra: RequestId,
    group_call_id: i32,
    participant_id: Option<&MessageSenderRef>,
    join_params: &GroupCallJoinParams,
    invite_hash: &str,
) -> String {
    json!({
        "@type": "joinVideoChat",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "participant_id": participant_id.map(|p| p.to_value()).unwrap_or(Value::Null),
        "join_parameters": join_params.to_value(),
        "invite_hash": invite_hash,
    })
    .to_string()
}

/// Phase C3a: `joinGroupCall` (TDLib 1.8.67,
/// `schema/td_api.tl:14285`):
/// `joinGroupCall input_group_call:InputGroupCall
/// join_parameters:groupCallJoinParameters = GroupCallInfo;`
/// Joins a regular group call that is not bound to a chat.
pub fn join_group_call(
    extra: RequestId,
    input_group_call: &InputGroupCallRef,
    join_params: &GroupCallJoinParams,
) -> String {
    json!({
        "@type": "joinGroupCall",
        "@extra": extra.as_extra(),
        "input_group_call": input_group_call.to_value(),
        "join_parameters": join_params.to_value(),
    })
    .to_string()
}

/// Phase C3a: `getGroupCall` (TDLib 1.8.67, `schema/td_api.tl:14274`):
/// `getGroupCall group_call_id:int32 = GroupCall;`
pub fn get_group_call(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "getGroupCall",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C3a: `getGroupCallParticipants` (TDLib 1.8.67,
/// `schema/td_api.tl:14449`):
/// `getGroupCallParticipants input_group_call:InputGroupCall
/// limit:int32 = GroupCallParticipants;`
/// "Returns information about participants of a non-joined group call
/// that is not bound to a chat".
pub fn get_group_call_participants(
    extra: RequestId,
    input_group_call: &InputGroupCallRef,
    limit: i32,
) -> String {
    json!({
        "@type": "getGroupCallParticipants",
        "@extra": extra.as_extra(),
        "input_group_call": input_group_call.to_value(),
        "limit": limit,
    })
    .to_string()
}

/// Phase C3a: `loadGroupCallParticipants` (TDLib 1.8.67,
/// `schema/td_api.tl:14455`):
/// `loadGroupCallParticipants group_call_id:int32 limit:int32 = Ok;`
/// "Loads more participants of a group call … The group call must be
/// previously received through getGroupCall and must be joined or
/// being joined". Limit up to 100.
pub fn load_group_call_participants(extra: RequestId, group_call_id: i32, limit: i32) -> String {
    json!({
        "@type": "loadGroupCallParticipants",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "limit": limit,
    })
    .to_string()
}

/// Phase C3a: `leaveGroupCall` (TDLib 1.8.67,
/// `schema/td_api.tl:14458`): `leaveGroupCall group_call_id:int32 =
/// Ok;` "Leaves a group call".
pub fn leave_group_call(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "leaveGroupCall",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C3a: `endGroupCall` (TDLib 1.8.67, `schema/td_api.tl:14461`):
/// `endGroupCall group_call_id:int32 = Ok;` "Ends a group call.
/// Requires groupCall.can_be_managed right for video chats and live
/// stories or groupCall.is_owned otherwise".
pub fn end_group_call(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "endGroupCall",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C2g: `startGroupCallScreenSharing` (TDLib 1.8.67,
/// `schema/td_api.tl:14303`):
/// `startGroupCallScreenSharing group_call_id:int32 audio_source_id:int32
/// payload:string = Text;`
/// "Starts screen sharing in a group call". The `payload` is the
/// presentation offer from `ntg_init_presentation`; the returned `Text`
/// is the answer for `ntg_connect(..., is_presentation=true)`. No
/// separate screen audio source exists in this slice (`audio_source_id`
/// is 0).
pub fn start_group_call_screen_sharing(
    extra: RequestId,
    group_call_id: i32,
    payload: &str,
) -> String {
    json!({
        "@type": "startGroupCallScreenSharing",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "audio_source_id": 0,
        "payload": payload,
    })
    .to_string()
}

/// Phase C2g: `endGroupCallScreenSharing` (TDLib 1.8.67,
/// `schema/td_api.tl:14309`): `endGroupCallScreenSharing
/// group_call_id:int32 = Ok;` "Ends screen sharing in a group call".
pub fn end_group_call_screen_sharing(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "endGroupCallScreenSharing",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C3a: `toggleGroupCallIsMyVideoEnabled` (TDLib 1.8.67,
/// `schema/td_api.tl:14414`):
/// `toggleGroupCallIsMyVideoEnabled group_call_id:int32
/// is_my_video_enabled:Bool = Ok;`
/// Signaling-only in this slice: tracks state, no camera (Phase C2).
pub fn toggle_group_call_is_my_video_enabled(
    extra: RequestId,
    group_call_id: i32,
    is_enabled: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallIsMyVideoEnabled",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "is_my_video_enabled": is_enabled,
    })
    .to_string()
}

/// Phase C3a: `toggleGroupCallIsMyVideoPaused` (TDLib 1.8.67,
/// `schema/td_api.tl:14411`):
/// `toggleGroupCallIsMyVideoPaused group_call_id:int32
/// is_my_video_paused:Bool = Ok;`
/// Signaling-only in this slice: tracks state, no camera (Phase C2).
pub fn toggle_group_call_is_my_video_paused(
    extra: RequestId,
    group_call_id: i32,
    is_paused: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallIsMyVideoPaused",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "is_my_video_paused": is_paused,
    })
    .to_string()
}

/// Phase C3a: `toggleGroupCallParticipantIsMuted` (TDLib 1.8.67,
/// `schema/td_api.tl:14431`):
/// `toggleGroupCallParticipantIsMuted group_call_id:int32
/// participant_id:MessageSender is_muted:Bool = Ok;`
/// "Toggles whether a participant of an active group call is muted,
/// unmuted, or allowed to unmute themselves; not supported for live
/// stories". Gate the UI on the participant's `can_be_muted_for_all_users`
/// / `can_be_unmuted_for_all_users` flags.
pub fn toggle_group_call_participant_is_muted(
    extra: RequestId,
    group_call_id: i32,
    participant_id: &MessageSenderRef,
    is_muted: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallParticipantIsMuted",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "participant_id": participant_id.to_value(),
        "is_muted": is_muted,
    })
    .to_string()
}

/// Phase C3a: `toggleGroupCallParticipantIsHandRaised` (TDLib 1.8.67,
/// `schema/td_api.tl:14444`):
/// `toggleGroupCallParticipantIsHandRaised group_call_id:int32
/// participant_id:MessageSender is_hand_raised:Bool = Ok;`
/// "for video chats only … Only self hand can be raised. Requires
/// groupCall.can_be_managed right to lower other's hand".
pub fn toggle_group_call_participant_is_hand_raised(
    extra: RequestId,
    group_call_id: i32,
    participant_id: &MessageSenderRef,
    is_hand_raised: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallParticipantIsHandRaised",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "participant_id": participant_id.to_value(),
        "is_hand_raised": is_hand_raised,
    })
    .to_string()
}

/// Phase C3a: `toggleVideoChatMuteNewParticipants` (TDLib 1.8.67,
/// `schema/td_api.tl:14317`):
/// `toggleVideoChatMuteNewParticipants group_call_id:int32
/// mute_new_participants:Bool = Ok;`
/// "Toggles whether new participants of a video chat can be unmuted
/// only by administrators of the video chat. Requires
/// groupCall.can_toggle_mute_new_participants right".
pub fn toggle_video_chat_mute_new_participants(
    extra: RequestId,
    group_call_id: i32,
    mute_new_participants: bool,
) -> String {
    json!({
        "@type": "toggleVideoChatMuteNewParticipants",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "mute_new_participants": mute_new_participants,
    })
    .to_string()
}

/// Phase C3a: `setVideoChatTitle` (TDLib 1.8.67,
/// `schema/td_api.tl:14312`):
/// `setVideoChatTitle group_call_id:int32 title:string = Ok;`
/// "Sets title of a video chat; requires groupCall.can_be_managed
/// right". Title is 1-64 characters.
pub fn set_video_chat_title(extra: RequestId, group_call_id: i32, title: &str) -> String {
    json!({
        "@type": "setVideoChatTitle",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "title": title,
    })
    .to_string()
}

/// Phase C3a: `getVideoChatInviteLink` (TDLib 1.8.67,
/// `schema/td_api.tl:14395`):
/// `getVideoChatInviteLink group_call_id:int32 can_self_unmute:Bool =
/// HttpUrl;`
/// "Returns invite link to a video chat in a public chat".
/// `can_self_unmute: true` requires `groupCall.can_be_managed`.
pub fn get_video_chat_invite_link(
    extra: RequestId,
    group_call_id: i32,
    can_self_unmute: bool,
) -> String {
    json!({
        "@type": "getVideoChatInviteLink",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "can_self_unmute": can_self_unmute,
    })
    .to_string()
}

/// Phase C2h: `revokeGroupCallInviteLink` (TDLib 1.8.67,
/// `schema/td_api.tl:14398`):
/// `revokeGroupCallInviteLink group_call_id:int32 = Ok;`
/// "Revokes invite link for a group call. Requires
/// groupCall.can_be_managed right for video chats or
/// groupCall.is_owned otherwise".
pub fn revoke_group_call_invite_link(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "revokeGroupCallInviteLink",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C2h: `startGroupCallRecording` (TDLib 1.8.67,
/// `schema/td_api.tl:14405`):
/// `startGroupCallRecording group_call_id:int32 title:string
/// record_video:Bool use_portrait_orientation:Bool = Ok;`
/// "Starts recording of an active group call; for video chats only.
/// Requires groupCall.can_be_managed right". Title is 0-64
/// characters; ongoing state arrives as `groupCall.record_duration`
/// / `is_video_recorded` (schema 1.8.67, lines 7151-7152).
pub fn start_group_call_recording(
    extra: RequestId,
    group_call_id: i32,
    title: &str,
    record_video: bool,
    use_portrait_orientation: bool,
) -> String {
    json!({
        "@type": "startGroupCallRecording",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "title": title,
        "record_video": record_video,
        "use_portrait_orientation": use_portrait_orientation,
    })
    .to_string()
}

/// Phase C2h: `endGroupCallRecording` (TDLib 1.8.67,
/// `schema/td_api.tl:14408`):
/// `endGroupCallRecording group_call_id:int32 = Ok;`
/// "Ends recording of an active group call; for video chats only.
/// Requires groupCall.can_be_managed right".
pub fn end_group_call_recording(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "endGroupCallRecording",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// Phase C2h: `startScheduledVideoChat` (TDLib 1.8.67,
/// `schema/td_api.tl:14277`):
/// `startScheduledVideoChat group_call_id:int32 = Ok;`
/// "Starts a scheduled video chat". The schema names no explicit
/// right for this constructor; the driver gates it on
/// `groupCall.can_be_managed` (the tracked proxy for the
/// `can_manage_video_chats` admin right), same as the other
/// video-chat admin actions.
pub fn start_scheduled_video_chat(extra: RequestId, group_call_id: i32) -> String {
    json!({
        "@type": "startScheduledVideoChat",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
    })
    .to_string()
}

/// `toggleVideoChatEnabledStartNotification` (TDLib 1.8.67,
/// `schema/td_api.tl:14282`):
/// `toggleVideoChatEnabledStartNotification group_call_id:int32
/// enabled_start_notification:Bool = Ok;`
/// "Toggles whether the current user will receive a notification
/// when the group call starts; for video chats only". The new flag
/// arrives back as `updateGroupCall` (`groupCall.enabled_start_notification`, :7154).
pub fn toggle_video_chat_enabled_start_notification(
    extra: RequestId,
    group_call_id: i32,
    enabled_start_notification: bool,
) -> String {
    json!({
        "@type": "toggleVideoChatEnabledStartNotification",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "enabled_start_notification": enabled_start_notification,
    })
    .to_string()
}

/// Phase C2h: `getVideoChatRtmpUrl` (TDLib 1.8.67,
/// `schema/td_api.tl:14261`):
/// `getVideoChatRtmpUrl chat_id:int53 = RtmpUrl;`
/// "Returns RTMP URL for streaming to the video chat of a chat;
/// requires can_manage_video_chats administrator right".
pub fn get_video_chat_rtmp_url(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "getVideoChatRtmpUrl",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// Phase C2h: `replaceVideoChatRtmpUrl` (TDLib 1.8.67,
/// `schema/td_api.tl:14264`):
/// `replaceVideoChatRtmpUrl chat_id:int53 = RtmpUrl;`
/// "Replaces the current RTMP URL for streaming to the video chat of
/// a chat; requires owner privileges in the chat".
pub fn replace_video_chat_rtmp_url(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "replaceVideoChatRtmpUrl",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// Phase C2h: `sendGroupCallMessage` (TDLib 1.8.67,
/// `schema/td_api.tl:14341`):
/// `sendGroupCallMessage group_call_id:int32 text:formattedText
/// paid_message_star_count:int53 = Ok;`
/// "Sends a message to other participants of a group call. Requires
/// groupCall.can_send_messages right". Plain text only (empty
/// entities); `paid_message_star_count` is 0 — paid messages are a
/// live-story-only feature Quill doesn't surface.
pub fn send_group_call_message(extra: RequestId, group_call_id: i32, text: &str) -> String {
    json!({
        "@type": "sendGroupCallMessage",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "text": { "@type": "formattedText", "text": text, "entities": [] },
        "paid_message_star_count": 0,
    })
    .to_string()
}

/// Phase C2h: `toggleGroupCallAreMessagesAllowed` (TDLib 1.8.67,
/// `schema/td_api.tl:14322`):
/// `toggleGroupCallAreMessagesAllowed group_call_id:int32
/// are_messages_allowed:Bool = Ok;`
/// "Toggles whether participants of a group call can send messages
/// there. Requires groupCall.can_toggle_are_messages_allowed right".
pub fn toggle_group_call_are_messages_allowed(
    extra: RequestId,
    group_call_id: i32,
    are_messages_allowed: bool,
) -> String {
    json!({
        "@type": "toggleGroupCallAreMessagesAllowed",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "are_messages_allowed": are_messages_allowed,
    })
    .to_string()
}

/// Phase C3a: `declineGroupCallInvitation` (TDLib 1.8.67,
/// `schema/td_api.tl:14380`):
/// `declineGroupCallInvitation chat_id:int53 message_id:int53 = Ok;`
/// Declines a `messageGroupCall` invitation (`schema/td_api.tl:5288`
/// flow: `joinGroupCall` to accept, `declineGroupCallInvitation` to
/// decline).
pub fn decline_group_call_invitation(extra: RequestId, chat_id: i64, message_id: i64) -> String {
    json!({
        "@type": "declineGroupCallInvitation",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "message_id": message_id,
    })
    .to_string()
}

/// Phase C2f: `inviteGroupCallParticipant` (TDLib 1.8.67,
/// `schema/td_api.tl:14375`):
/// `inviteGroupCallParticipant group_call_id:int32 user_id:int53
/// is_video:Bool = InviteGroupCallParticipantResult;`
/// "Invites a user to an active group call". The answer is one of
/// the `inviteGroupCallParticipantResult*` variants (schema 1.8.67,
/// lines 7216-7227), parsed by the envelope.
pub fn invite_group_call_participant(
    extra: RequestId,
    group_call_id: i32,
    user_id: i64,
    is_video: bool,
) -> String {
    json!({
        "@type": "inviteGroupCallParticipant",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "user_id": user_id,
        "is_video": is_video,
    })
    .to_string()
}

/// Phase C2f: `banGroupCallParticipants` (TDLib 1.8.67,
/// `schema/td_api.tl:14385`):
/// `banGroupCallParticipants group_call_id:int32 user_ids:vector<int64>
/// = Ok;` "Identifiers of group call participants to ban".
pub fn ban_group_call_participants(
    extra: RequestId,
    group_call_id: i32,
    user_ids: &[i64],
) -> String {
    json!({
        "@type": "banGroupCallParticipants",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "user_ids": user_ids,
    })
    .to_string()
}

/// Phase C2f: `setGroupCallParticipantVolumeLevel` (TDLib 1.8.67,
/// `schema/td_api.tl:14438`):
/// `setGroupCallParticipantVolumeLevel group_call_id:int32
/// participant_id:MessageSender volume_level:int32 = Ok;`
/// "New participant's volume level; 1-20000 in hundreds of percents"
/// — the driver clamps before sending.
pub fn set_group_call_participant_volume_level(
    extra: RequestId,
    group_call_id: i32,
    participant_id: &MessageSenderRef,
    volume_level: i32,
) -> String {
    json!({
        "@type": "setGroupCallParticipantVolumeLevel",
        "@extra": extra.as_extra(),
        "group_call_id": group_call_id,
        "participant_id": participant_id.to_value(),
        "volume_level": volume_level,
    })
    .to_string()
}

/// Phase B4: `setChatMessageAutoDeleteTime` (TDLib 1.8.67,
/// `schema/td_api.tl:13454`):
/// `setChatMessageAutoDeleteTime chat_id:int53
/// message_auto_delete_time:int32 = Ok;`
/// "Changes the message auto-delete **or self-destruct (for secret
/// chats)** time in a chat. Requires change_info administrator right in
/// basic groups, supergroups and channels."
/// Value rule (from the schema comment): unless the chat is secret, the
/// time must be 0 or a multiple of 86400, up to 365 * 86400; secret chats
/// accept arbitrary second values. 0 disables the timer. The driver
/// enforces the rule before sending (defense in depth); the new value
/// arrives back as `updateChatMessageAutoDeleteTime`.
pub fn set_chat_message_auto_delete_time(
    extra: RequestId,
    chat_id: i64,
    message_auto_delete_time: i32,
) -> String {
    json!({
        "@type": "setChatMessageAutoDeleteTime",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "message_auto_delete_time": message_auto_delete_time,
    })
    .to_string()
}

/// Phase 3.3: `getCommands` for a bot's global (default) command scope
/// (TDLib 1.8.67, `schema/td_api.tl:14953`):
/// `getCommands scope:BotCommandScope language_code:string = BotCommands;`
/// The schema annotates the method "for bots only", so a user session
/// gets an `error` answer instead of `botCommands`; the driver absorbs it
/// and the `/` menu falls back to the `botInfo` commands. The scope is
/// `botCommandScopeDefault` (line 10360, "a scope covering all users");
/// the chat-specific commands already arrive via `botInfo`.
pub fn get_commands(extra: RequestId) -> String {
    json!({
        "@type": "getCommands",
        "@extra": extra.as_extra(),
        "scope": null,
        "language_code": "",
    })
    .to_string()
}

/// `joinChat` for a public channel (TDLib 1.8.67). Response is
/// `ChatJoinResult`.
pub fn join_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "joinChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `leaveChat` for a channel (TDLib 1.8.67). Response is `ok`.
pub fn leave_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "leaveChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `getChatSponsoredMessages` (TDLib 1.8.67). For channel chats (and chats
/// with bots); rows render Sponsored / Recommended per `is_recommended`.
pub fn get_chat_sponsored_messages(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatSponsoredMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `reportChatSponsoredMessage` (TDLib 1.8.67). `option_id` is the base64
/// `reportOption.id`; empty for the initial request.
pub fn report_chat_sponsored_message(
    extra: RequestId,
    chat_id: ChatId,
    message_id: i64,
    option_id: &str,
) -> String {
    json!({
        "@type": "reportChatSponsoredMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id,
        "option_id": option_id,
    })
    .to_string()
}

/// Slice CL3: `reportChat` (TDLib 1.8.67, `schema/td_api.tl:15693`):
/// `reportChat chat_id:int53 option_id:bytes message_ids:vector<int53>
/// text:string = ReportChatResult;`
/// The simple spam-report flow uses empty option_id/message_ids/text
/// (schema:3667: "The chat can be reported as spam using the method
/// reportChat with an empty option_id and message_ids").
pub fn report_chat(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "reportChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "option_id": "",
        "message_ids": [],
        "text": "",
    })
    .to_string()
}

/// Slice CL3: `setMessageSenderBlockList` (TDLib 1.8.67,
/// `schema/td_api.tl:14492`):
/// `setMessageSenderBlockList sender_id:MessageSender
/// block_list:BlockList = Ok;`
/// `block = false` passes null `block_list` to unblock the sender (TGX
/// `Tdlib.unblockSender`).
pub fn set_message_sender_block_list(extra: RequestId, user_id: i64, block: bool) -> String {
    json!({
        "@type": "setMessageSenderBlockList",
        "@extra": extra.as_extra(),
        "sender_id": { "@type": "messageSenderUser", "user_id": user_id },
        "block_list": if block { json!({ "@type": "blockListMain" }) } else { Value::Null },
    })
    .to_string()
}

/// Slice B2: `sendBotStartMessage bot_user_id:int53 chat_id:int53
/// parameter:string = Message;` (TDLib 1.8.67, `schema/td_api.tl:12216`) —
/// what the START button and "Restart bot" send. `parameter` is the
/// `internalLinkTypeBotStart.start_parameter` (line 9399); empty for a
/// plain restart. Telegram X `Tdlib.sendBotStartMessage`.
pub fn send_bot_start_message(
    extra: RequestId,
    bot_user_id: i64,
    chat_id: i64,
    parameter: &str,
) -> String {
    json!({
        "@type": "sendBotStartMessage",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
        "chat_id": chat_id,
        "parameter": parameter,
    })
    .to_string()
}

/// Slice B2: `getBotSimilarBots bot_user_id:int53 = Users;` (TDLib 1.8.67,
/// `schema/td_api.tl:11640`). Powers the similar-bots section of the bot
/// profile (Telegram X `SharedChatsController.Mode.SIMILAR_BOTS`).
pub fn get_bot_similar_bots(extra: RequestId, bot_user_id: i64) -> String {
    json!({
        "@type": "getBotSimilarBots",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
    })
    .to_string()
}

/// `viewSponsoredChat` (TDLib 1.8.67). `unique_id` is the `sponsoredChat`
/// unique id (from sponsored search results).
pub fn view_sponsored_chat(extra: RequestId, sponsored_chat_unique_id: i64) -> String {
    json!({
        "@type": "viewSponsoredChat",
        "@extra": extra.as_extra(),
        "sponsored_chat_unique_id": sponsored_chat_unique_id,
    })
    .to_string()
}

/// `clickChatSponsoredMessage` (TDLib 1.8.67). Sent when the user opens a
/// sponsored message's sponsor link/button (`is_media_click = false`) or its
/// media (`is_media_click = true`). `from_fullscreen` is true when the media
/// was opened from the fullscreen viewer.
pub fn click_chat_sponsored_message(
    extra: RequestId,
    chat_id: ChatId,
    message_id: i64,
    is_media_click: bool,
    from_fullscreen: bool,
) -> String {
    json!({
        "@type": "clickChatSponsoredMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id,
        "is_media_click": is_media_click,
        "from_fullscreen": from_fullscreen,
    })
    .to_string()
}

/// `viewMessages` (TDLib 1.8.67). `source` is `messageSourceChatHistory`.
/// `force_read` marks the ids read even if `openChat` has not completed.
pub fn view_messages(
    extra: RequestId,
    chat_id: ChatId,
    message_ids: &[MessageId],
    source: &str,
    force_read: bool,
) -> String {
    json!({
        "@type": "viewMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "source": { "@type": source },
        "force_read": force_read,
    })
    .to_string()
}

/// `downloadFile` (TDLib 1.8.67). `synchronous: false` returns the current
/// `file` immediately; progress continues on `updateFile`.
pub fn download_file(extra: RequestId, file_id: FileId, priority: i32) -> String {
    json!({
        "@type": "downloadFile",
        "@extra": extra.as_extra(),
        "file_id": file_id.0,
        "priority": priority,
        "offset": 0,
        "limit": 0,
        "synchronous": false,
    })
    .to_string()
}

/// `cancelDownloadFile` (TDLib 1.8.67, schema :13990-13991): "Stops the
/// downloading of a file. If a file has already been downloaded, does
/// nothing." `only_if_pending: false` cancels an in-flight download (TGX
/// `cancelDownloadOrUploadFile`); `true` only stops one that hasn't started.
pub fn cancel_download_file(extra: RequestId, file_id: FileId, only_if_pending: bool) -> String {
    json!({
        "@type": "cancelDownloadFile",
        "@extra": extra.as_extra(),
        "file_id": file_id.0,
        "only_if_pending": only_if_pending,
    })
    .to_string()
}

/// `setChatDraftMessage` (TDLib 1.8.67). `topic_id` null updates the chat itself.
/// `draft_message` null removes the draft. Text uses `draftMessageContentText`.
/// `date` 0 and `effect_id` `"0"` match Unigram's `DraftMessage` constructor
/// (`int64` is a JSON string). `link_preview_options` null keeps the default.
pub fn set_chat_draft_message(
    extra: RequestId,
    chat_id: ChatId,
    text: Option<&str>,
    reply_to: Option<&SendReply>,
) -> String {
    let draft_message = match text {
        None if reply_to.is_none() => Value::Null,
        text => {
            let body = text.unwrap_or("");
            json!({
                "@type": "draftMessage",
                "reply_to": send_reply_value(reply_to),
                "date": 0,
                "content": {
                    "@type": "draftMessageContentText",
                    "text": {
                        "@type": "formattedText",
                        "text": body,
                        "entities": []
                    },
                    "link_preview_options": Value::Null
                },
                "effect_id": "0",
                "suggested_post_info": Value::Null
            })
        }
    };
    json!({
        "@type": "setChatDraftMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "draft_message": draft_message,
    })
    .to_string()
}

/// M1: `textEntity` JSON for a parsed composer entity (TDLib 1.8.67,
/// `schema/td_api.tl:5743`–`:5773` — `textEntityTypeBold`,
/// `textEntityTypeItalic`, `textEntityTypeUnderline`,
/// `textEntityTypeStrikethrough`, `textEntityTypeSpoiler`,
/// `textEntityTypeCode`, `textEntityTypePre` / `textEntityTypePreCode
/// language`, `textEntityTypeBlockQuote`, `textEntityTypeTextUrl url`).
/// Offsets/lengths are UTF-16 code units, per the schema.
pub fn format_entity_json(entity: &ComposerEntity) -> Value {
    let entity_type = match entity.kind {
        FormatKind::Bold => json!({"@type": "textEntityTypeBold"}),
        FormatKind::Italic => json!({"@type": "textEntityTypeItalic"}),
        FormatKind::Underline => json!({"@type": "textEntityTypeUnderline"}),
        FormatKind::Strikethrough => json!({"@type": "textEntityTypeStrikethrough"}),
        FormatKind::Code => json!({"@type": "textEntityTypeCode"}),
        FormatKind::Pre if entity.language.is_empty() => {
            json!({"@type": "textEntityTypePre"})
        }
        FormatKind::Pre => json!({
            "@type": "textEntityTypePreCode",
            "language": entity.language,
        }),
        FormatKind::Spoiler => json!({"@type": "textEntityTypeSpoiler"}),
        FormatKind::BlockQuote => json!({"@type": "textEntityTypeBlockQuote"}),
        FormatKind::TextUrl => json!({
            "@type": "textEntityTypeTextUrl",
            "url": entity.url,
        }),
    };
    json!({
        "@type": "textEntity",
        "offset": entity.offset,
        "length": entity.length,
        "type": entity_type,
    })
}

/// M1: `messageSendOptions` JSON (TDLib 1.8.67, `schema/td_api.tl:5934`).
/// `scheduling_state` is `messageSchedulingStateSendAtDate` (`:5902`) or
/// `messageSchedulingStateSendWhenOnline` (`:5905`); null otherwise.
pub fn message_send_options(options: &SendOptions) -> Value {
    let scheduling_state = match options.scheduling {
        ComposerScheduling::None => Value::Null,
        ComposerScheduling::SendAtDate(send_date) => json!({
            "@type": "messageSchedulingStateSendAtDate",
            "send_date": send_date as i32,
            "repeat_period": 0,
        }),
        ComposerScheduling::SendWhenOnline => {
            json!({"@type": "messageSchedulingStateSendWhenOnline"})
        }
    };
    json!({
        "@type": "messageSendOptions",
        "suggested_post_info": Value::Null,
        "disable_notification": options.disable_notification,
        "from_background": false,
        "protect_content": false,
        "allow_paid_broadcast": false,
        "paid_message_star_count": 0,
        "update_order_of_installed_sticker_sets": false,
        "scheduling_state": scheduling_state,
        "effect_id": 0,
        "sending_id": 0,
        "only_preview": false,
    })
}

/// `sendMessage` for the pinned 1.8.67 schema: typed `topic_id`, not `message_thread_id`.
/// Parity slice 4: posting into a forum topic passes
/// `topic_id = messageTopicForum{forum_topic_id}` (schema 1.8.67, lines
/// 12200 and 3004); `None` sends null (no topic).
fn message_topic_value(topic_id: Option<i32>) -> Value {
    match topic_id {
        Some(forum_topic_id) => json!({
            "@type": "messageTopicForum",
            "forum_topic_id": forum_topic_id,
        }),
        None => Value::Null,
    }
}

pub fn send_text(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    text: &str,
    reply_to: Option<SendReply>,
    options: &SendOptions,
) -> String {
    // M1: composer markup (`**bold**` etc.) becomes `textEntities` here,
    // so formatting genuinely reaches the wire on every text send.
    // M1 fix-up: secret chats strip `textEntityTypeBlockQuote` (schema:
    // unsupported there) — see `secret_chat_entities`.
    let (clean_text, entities) = parse_format_markup(text);
    let entities = secret_chat_entities(entities, options.is_secret);
    let entities_json: Vec<Value> = entities.iter().map(format_entity_json).collect();
    // Phase S1: secret chats never get link previews (TGX default-off —
    // previews are generated on Telegram servers, which can't see E2E
    // content). `is_disabled: true` makes the default-off explicit on the
    // wire instead of relying on TDLib to skip it.
    let link_preview_options = if options.link_preview_disabled {
        json!({
            "@type": "linkPreviewOptions",
            "is_disabled": true,
            "url": "",
            "force_small_media": false,
            "force_large_media": false,
            "show_above_text": false,
        })
    } else if options.link_preview_above_text
        || !matches!(options.link_preview_media, PreviewMediaSize::Auto)
    {
        // MED4b: full `linkPreviewOptions` (schema:2237). The force flags
        // are ignored unless the URL is explicitly specified, so the
        // detected first URL rides along (TGX sets `options.url` when
        // forcing — `MessagesController.takeOutputLinkPreviewOptions`).
        let first_url = find_urls(text).into_iter().next().unwrap_or_default();
        json!({
            "@type": "linkPreviewOptions",
            "is_disabled": false,
            "url": first_url,
            "force_small_media": matches!(options.link_preview_media, PreviewMediaSize::ForceSmall),
            "force_large_media": matches!(options.link_preview_media, PreviewMediaSize::ForceLarge),
            "show_above_text": options.link_preview_above_text,
        })
    } else {
        Value::Null
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": message_send_options(options),
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageText",
            "text": {
                "@type": "formattedText",
                "text": clean_text,
                "entities": entities_json
            },
            "link_preview_options": link_preview_options,
            "clear_draft": true
        }
    })
    .to_string()
}

/// M2: `sendMessage` + `inputMessageRichMessage` (TDLib 1.8.67, line 6084).
/// `rich` is the `inputRichMessage` object built by
/// `quill::rich::input_rich_message`. Reply / scheduling / silent options
/// ride the same `sendMessage` envelope as text sends.
pub fn send_rich_message(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    rich: &Value,
    reply_to: Option<SendReply>,
    options: &SendOptions,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": message_send_options(options),
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageRichMessage",
            "message": rich,
            "clear_draft": true
        }
    })
    .to_string()
}

/// MED4: `getWebPageInstantView` (TDLib 1.8.67, `schema/td_api.tl:14794`).
/// `only_local: false` — a network fetch is exactly what opening IV is
/// for. TDLib answers `webPageInstantView` or a 404 error when the page
/// has no Instant View (the caller falls back to the browser).
pub fn get_web_page_instant_view(extra: RequestId, url: &str) -> String {
    json!({
        "@type": "getWebPageInstantView",
        "@extra": extra.as_extra(),
        "url": url,
        "only_local": false,
    })
    .to_string()
}

/// MED4b: `getLinkPreview` (TDLib 1.8.67, `schema/td_api.tl:14792`) —
/// "Returns a link preview by the text of a message. Do not call this
/// function too often. Returns a 404 error if the text has no link
/// preview". TGX (`LinkPreview.loadLinkPreview`) passes the URL as the
/// text with null options; Quill does the same and debounces at the UI.
pub fn get_link_preview(extra: RequestId, url: &str) -> String {
    json!({
        "@type": "getLinkPreview",
        "@extra": extra.as_extra(),
        "text": {
            "@type": "formattedText",
            "text": url,
            "entities": []
        },
        "link_preview_options": Value::Null,
    })
    .to_string()
}

/// M2: `getFullRichMessage` (TDLib 1.8.67, line 11554) — fetch the full
/// blocks of a partially received `richMessage` (`is_full == false`).
pub fn get_full_rich_message(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getFullRichMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

fn formatted_caption(caption: &str, strip_blockquote: bool) -> Value {
    // M1 fix-up: captions get the same markup→entities treatment as
    // message text (the toolbar is always visible above the composer,
    // including with attachments pending, so `**bold**` in a caption
    // must not go out literal).
    let (clean_text, entities) = parse_format_markup(caption);
    let entities = secret_chat_entities(entities, strip_blockquote);
    let entities_json: Vec<Value> = entities.iter().map(format_entity_json).collect();
    json!({
        "@type": "formattedText",
        "text": clean_text,
        "entities": entities_json
    })
}

/// M1 fix-up: `textEntityTypeBlockQuote` is not supported in secret chats
/// (schema 1.8.67) — strip it driver-side instead of letting TDLib drop
/// it (S1's layer-based gating philosophy).
fn secret_chat_entities(
    entities: Vec<ComposerEntity>,
    strip_blockquote: bool,
) -> Vec<ComposerEntity> {
    if strip_blockquote {
        entities
            .into_iter()
            .filter(|e| e.kind != FormatKind::BlockQuote)
            .collect()
    } else {
        entities
    }
}

/// Phase B3: self-destruct choice for `inputMessagePhoto` /
/// `inputMessageVideo` (TDLib 1.8.67, `schema/td_api.tl:6117` /
/// `:6128` — "private chats only"). TDLib validates the choice at runtime
/// (`MessageSelfDestructType::get_message_self_destruct_type`): the timer
/// must be 1–60 seconds (`MAX_PRIVATE_MESSAGE_TTL = 60`), and any non-empty
/// choice in a non-`DialogType::User` chat fails with 400 "Messages can
/// self-destruct only in private chats" — secret chats included. The driver
/// therefore strips the choice for every non-`chatTypePrivate` chat
/// (defense in depth; the composer picker is gated the same way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfDestructSend {
    /// `messageSelfDestructTypeTimer` (schema 1.8.67 line 5915).
    Timer(i32),
    /// `messageSelfDestructTypeImmediately` (schema 1.8.67 line 5918) —
    /// view once, destroyed after being closed.
    Immediately,
}

/// `MessageSelfDestructType` JSON for an `inputMessage*` `self_destruct_type`
/// field. `None` is JSON null (schema: "pass null if none").
fn self_destruct_type_value(choice: Option<SelfDestructSend>) -> Value {
    match choice {
        None => Value::Null,
        Some(SelfDestructSend::Timer(secs)) => json!({
            "@type": "messageSelfDestructTypeTimer",
            "self_destruct_time": secs
        }),
        Some(SelfDestructSend::Immediately) => json!({
            "@type": "messageSelfDestructTypeImmediately"
        }),
    }
}

/// `inputMessagePhoto` body (TDLib 1.8.67). Shared by `sendMessage` and `sendMessageAlbum`.
pub fn input_message_photo(
    path: &str,
    caption: &str,
    caption_above: bool,
    self_destruct: Option<SelfDestructSend>,
    strip_blockquote: bool,
) -> Value {
    json!({
        "@type": "inputMessagePhoto",
        "photo": {
            "@type": "inputPhoto",
            "photo": {
                "@type": "inputFileLocal",
                "path": path
            },
            "thumbnail": Value::Null,
            "video": Value::Null,
            "added_sticker_file_ids": [],
            "width": 0,
            "height": 0
        },
        "caption": formatted_caption(caption, strip_blockquote),
        "show_caption_above_media": caption_above,
        "self_destruct_type": self_destruct_type_value(self_destruct),
        "has_spoiler": false
    })
}

/// `sendMessage` + `inputMessagePhoto` / `inputPhoto` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
#[allow(clippy::too_many_arguments)] // `strip_blockquote` is pure pass-through to `input_message_photo`
pub fn send_photo(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    caption: &str,
    caption_above: bool,
    reply_to: Option<SendReply>,
    self_destruct: Option<SelfDestructSend>,
    strip_blockquote: bool,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": input_message_photo(path, caption, caption_above, self_destruct, strip_blockquote)
    })
    .to_string()
}

/// `getInstalledStickerSets` for regular stickers (Unigram `StickerTypeRegular`).
pub fn get_installed_sticker_sets(extra: RequestId) -> String {
    json!({
        "@type": "getInstalledStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeRegular" },
    })
    .to_string()
}

/// `getStickerSet`. `set_id` is int64 — JSON string, not a float.
pub fn get_sticker_set(extra: RequestId, set_id: i64) -> String {
    json!({
        "@type": "getStickerSet",
        "@extra": extra.as_extra(),
        "set_id": set_id.to_string(),
    })
    .to_string()
}

/// Fields for `inputMessageSticker` (TDLib 1.8.67). Thumbnail matches Unigram `Thumbnail.ToInput`.
pub struct StickerSend<'a> {
    pub file_id: FileId,
    pub emoji: &'a str,
    pub width: i32,
    pub height: i32,
    pub thumb: Option<(FileId, i32, i32)>,
    pub reply_to: Option<SendReply>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `sendMessage` + `inputMessageSticker` / `inputSticker` / `inputFileId` (1.8.67).
/// Thumbnail is `inputThumbnail` + `inputFileId` when the sticker has one (Unigram `Thumbnail.ToInput`).
pub fn send_sticker(extra: RequestId, chat_id: ChatId, sticker: StickerSend<'_>) -> String {
    let thumbnail = match sticker.thumb {
        Some((id, thumb_width, thumb_height)) if id.0 != 0 => json!({
            "@type": "inputThumbnail",
            "thumbnail": { "@type": "inputFileId", "id": id.0 },
            "width": thumb_width,
            "height": thumb_height,
        }),
        _ => Value::Null,
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(sticker.topic_id),
        "reply_to": send_reply_value(sticker.reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageSticker",
            "sticker": {
                "@type": "inputSticker",
                "sticker": { "@type": "inputFileId", "id": sticker.file_id.0 },
                "thumbnail": thumbnail,
                "width": sticker.width,
                "height": sticker.height,
            },
            "emoji": sticker.emoji,
        }
    })
    .to_string()
}

/// Fields for `inputMessageAnimation` (TDLib 1.8.67). Saved GIFs use `inputFileId`.
/// Thumbnail stays null: `inputThumbnail` says file_id upload is not supported, and a
/// saved animation is already known to the server (Unigram sends the file id only).
pub struct AnimationSend {
    pub file_id: FileId,
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub reply_to: Option<SendReply>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `getSavedAnimations` — saved GIFs, no query and no third-party key.
pub fn get_saved_animations(extra: RequestId) -> String {
    json!({
        "@type": "getSavedAnimations",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `sendMessage` + `inputMessageAnimation` / `inputAnimation` / `inputFileId` (1.8.67).
pub fn send_animation(extra: RequestId, chat_id: ChatId, animation: AnimationSend) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(animation.topic_id),
        "reply_to": send_reply_value(animation.reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageAnimation",
            "animation": {
                "@type": "inputAnimation",
                "animation": { "@type": "inputFileId", "id": animation.file_id.0 },
                "thumbnail": Value::Null,
                "added_sticker_file_ids": [],
                "duration": animation.duration,
                "width": animation.width,
                "height": animation.height,
            },
            "caption": Value::Null,
            "show_caption_above_media": false,
            "has_spoiler": false,
        }
    })
    .to_string()
}

/// Fields for `inputVideo` (TDLib 1.8.67). Thumbnail stays null: the schema says
/// pass null to skip thumbnail uploading, and TDLib fills one for small files.
pub struct VideoSend {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub supports_streaming: bool,
    /// Phase B3: `inputMessageVideo.self_destruct_type` (schema 1.8.67
    /// line 6128 — private chats only).
    pub self_destruct: Option<SelfDestructSend>,
}

/// `inputMessageVideo` body (TDLib 1.8.67). Shared by `sendMessage` and `sendMessageAlbum`.
pub fn input_message_video(
    path: &str,
    video: &VideoSend,
    caption: &str,
    caption_above: bool,
    strip_blockquote: bool,
) -> Value {
    json!({
        "@type": "inputMessageVideo",
        "video": {
            "@type": "inputVideo",
            "video": {
                "@type": "inputFileLocal",
                "path": path
            },
            "thumbnail": Value::Null,
            "cover": Value::Null,
            "start_timestamp": 0,
            "added_sticker_file_ids": [],
            "duration": video.duration,
            "width": video.width,
            "height": video.height,
            "supports_streaming": video.supports_streaming
        },
        "caption": formatted_caption(caption, strip_blockquote),
        "show_caption_above_media": caption_above,
        "self_destruct_type": self_destruct_type_value(video.self_destruct),
        "has_spoiler": false
    })
}

/// `inputVideoNote.thumbnail` when a JPEG was written locally. `None` is JSON null
/// (schema: pass null to skip thumbnail uploading).
pub struct VideoNoteThumbnailSend {
    pub path: String,
    pub width: i32,
    pub height: i32,
}

/// Fields for `inputVideoNote` (TDLib 1.8.67). `duration` is 0–60. `length` is
/// the square side, positive and at most 640.
pub struct VideoNoteSend {
    pub duration: i32,
    pub length: i32,
    pub thumbnail: Option<VideoNoteThumbnailSend>,
}

fn input_video_note_thumbnail(thumb: Option<&VideoNoteThumbnailSend>) -> Value {
    match thumb {
        Some(thumb) => json!({
            "@type": "inputThumbnail",
            "thumbnail": {
                "@type": "inputFileLocal",
                "path": thumb.path
            },
            "width": thumb.width,
            "height": thumb.height
        }),
        None => Value::Null,
    }
}

/// `sendMessage` + `inputMessageVideoNote` / `inputVideoNote` / `inputFileLocal` (1.8.67).
/// No caption: the constructor is `video_note` and `self_destruct_type` only.
/// `path` must already be an explicitly picked local file.
pub fn send_video_note(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    note: &VideoNoteSend,
    reply_to: Option<SendReply>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageVideoNote",
            "video_note": {
                "@type": "inputVideoNote",
                "video_note": {
                    "@type": "inputFileLocal",
                    "path": path
                },
                "thumbnail": input_video_note_thumbnail(note.thumbnail.as_ref()),
                "duration": note.duration,
                "length": note.length
            },
            "self_destruct_type": Value::Null
        }
    })
    .to_string()
}

/// `sendMessage` + `inputMessageVideo` / `inputVideo` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
#[allow(clippy::too_many_arguments)] // `strip_blockquote` is pure pass-through to `input_message_video`
pub fn send_video(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    video: &VideoSend,
    caption: &str,
    caption_above: bool,
    reply_to: Option<SendReply>,
    strip_blockquote: bool,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": input_message_video(path, video, caption, caption_above, strip_blockquote)
    })
    .to_string()
}

/// `sendMessageAlbum` (TDLib 1.8.67). 2–10 contents, same `show_caption_above_media`.
/// Caption sits on the last item (`show_caption_above_media` is false).
pub fn send_message_album(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    reply_to: Option<SendReply>,
    input_message_contents: Vec<Value>,
) -> String {
    json!({
        "@type": "sendMessageAlbum",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "input_message_contents": input_message_contents
    })
    .to_string()
}

/// `sendMessage` + `inputMessageDocument` / `inputDocument` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
pub fn send_document(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    caption: &str,
    reply_to: Option<SendReply>,
    strip_blockquote: bool,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageDocument",
            "document": {
                "@type": "inputDocument",
                "document": {
                    "@type": "inputFileLocal",
                    "path": path
                },
                "thumbnail": Value::Null,
                "disable_content_type_detection": false
            },
            "caption": formatted_caption(caption, strip_blockquote)
        }
    })
    .to_string()
}

/// `setPollAnswer` (TDLib 1.8.67, `schema/td_api.tl:12932`): `option_ids` are
/// 0-based indexes into the poll's option list (not the `pollOption.id`
/// strings). Response is `ok`; the new counts arrive via `updatePoll`.
pub fn set_poll_answer(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    option_ids: &[i32],
) -> String {
    json!({
        "@type": "setPollAnswer",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "option_ids": option_ids,
    })
    .to_string()
}

/// Fields for `inputMessagePoll` (TDLib 1.8.67, `schema/td_api.tl:6193`).
/// Quiz creation stays out of this slice (Phase 4.2) — polls are created as
/// `inputPollTypeRegular` (schema line 481) even for quiz-flagged drafts.
pub struct PollSend<'a> {
    pub question: &'a str,
    pub options: &'a [&'a str],
    pub is_anonymous: bool,
    pub allows_multiple_answers: bool,
    pub reply_to: Option<SendReply>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `sendMessage` + `inputMessagePoll` / `inputPollOption` / `inputPollTypeRegular`
/// (TDLib 1.8.67). Options must already be trimmed and non-empty (2–10);
/// the question 1–255 chars — validated by `PollDraft::validate` before this
/// is called. `allows_revoting` is true for regular polls (official clients
/// let the user change their vote); `members_only`, `country_codes`,
/// `shuffle_options`, `hide_results_until_closes`, `open_period`,
/// `close_date` all stay at the zero value.
pub fn send_poll(extra: RequestId, chat_id: ChatId, poll: PollSend<'_>) -> String {
    let options: Vec<Value> = poll
        .options
        .iter()
        .map(|text| {
            json!({
                "@type": "inputPollOption",
                "text": {
                    "@type": "formattedText",
                    "text": text,
                    "entities": []
                },
                "media": Value::Null
            })
        })
        .collect();
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(poll.topic_id),
        "reply_to": send_reply_value(poll.reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessagePoll",
            "question": {
                "@type": "formattedText",
                "text": poll.question,
                "entities": []
            },
            "options": options,
            "description": Value::Null,
            "media": Value::Null,
            "is_anonymous": poll.is_anonymous,
            "allows_multiple_answers": poll.allows_multiple_answers,
            "allows_revoting": true,
            "members_only": false,
            "country_codes": [],
            "shuffle_options": false,
            "hide_results_until_closes": false,
            "type": {
                "@type": "inputPollTypeRegular",
                "allow_adding_options": false
            },
            "open_period": 0,
            "close_date": 0,
            "is_closed": false
        }
    })
    .to_string()
}

/// `editMessageText` (TDLib 1.8.67). `reply_markup` null — bots only.
/// `input_message_content` must be `inputMessageText` (or `inputMessageRichMessage`).
pub fn edit_message_text(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    text: &str,
    strip_blockquote: bool,
) -> String {
    // M1: edits carry the same markup→entities conversion as sends.
    // M1 fix-up: secret chats strip `textEntityTypeBlockQuote`
    // (unsupported there), same as sends.
    let (clean_text, entities) = parse_format_markup(text);
    let entities = secret_chat_entities(entities, strip_blockquote);
    let entities_json: Vec<Value> = entities.iter().map(format_entity_json).collect();
    json!({
        "@type": "editMessageText",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageText",
            "text": {
                "@type": "formattedText",
                "text": clean_text,
                "entities": entities_json
            },
            "link_preview_options": Value::Null,
            "clear_draft": false
        }
    })
    .to_string()
}

/// `editMessageCaption` (TDLib 1.8.67). Caption-only media edit.
/// `show_caption_above_media` is false unless the original already inverted it.
/// M1 fix-up: the caption gets the same markup→entities treatment as
/// sends (blocker: `**bold**` went out literal).
pub fn edit_message_caption(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    caption: &str,
    show_caption_above_media: bool,
    strip_blockquote: bool,
) -> String {
    json!({
        "@type": "editMessageCaption",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reply_markup": Value::Null,
        "caption": formatted_caption(caption, strip_blockquote),
        "show_caption_above_media": show_caption_above_media
    })
    .to_string()
}

/// `deleteMessages` (TDLib 1.8.67). `revoke` true = delete for all members
/// (tdesktop `DeleteMessagesBox` / Unigram `DeleteMessagesPopup` default for
/// own outgoing that `can_be_deleted_for_all_users`). Always true in
/// supergroups, channels, and secret chats per schema.
pub fn delete_messages(
    extra: RequestId,
    chat_id: ChatId,
    message_ids: &[MessageId],
    revoke: bool,
) -> String {
    json!({
        "@type": "deleteMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "revoke": revoke
    })
    .to_string()
}

/// `forwardMessages` (TDLib 1.8.67, `schema/td_api.tl:12237` —
/// `forwardMessages chat_id topic_id from_chat_id message_ids options
/// send_copy remove_caption`). `send_copy` true drops the "Forwarded from"
/// attribution (TGX "Hide sender name"); `remove_caption` strips captions
/// on the copies (ignored unless `send_copy` is true). `topic_id` /
/// `options` null. Ids must already be strictly increasing (≤ 100).
pub fn forward_messages(
    extra: RequestId,
    chat_id: ChatId,
    from_chat_id: ChatId,
    message_ids: &[MessageId],
    send_copy: bool,
    remove_caption: bool,
) -> String {
    json!({
        "@type": "forwardMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "from_chat_id": from_chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "options": Value::Null,
        "send_copy": send_copy,
        "remove_caption": remove_caption && send_copy
    })
    .to_string()
}

/// M1: `unpinAllChatMessages` (TDLib 1.8.67, `schema/td_api.tl:13565` —
/// `unpinAllChatMessages chat_id:int53 = Ok;`).
pub fn unpin_all_chat_messages(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "unpinAllChatMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// M1: `getMessageLink` (TDLib 1.8.67, `schema/td_api.tl:12064` —
/// `getMessageLink chat_id message_id media_timestamp checklist_task_id
/// poll_option_id for_album in_message_thread = MessageLink`). Plain
/// message link: no timestamp / album / thread.
pub fn get_message_link(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getMessageLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "media_timestamp": 0,
        "checklist_task_id": 0,
        "poll_option_id": "",
        "for_album": false,
        "in_message_thread": false,
    })
    .to_string()
}

/// M1 fix-up: `getMessageProperties` (TDLib 1.8.67,
/// `schema/td_api.tl:11557`). "Share link" sends this first so the
/// driver can gate `getMessageLink` on `messageProperties.can_get_link`
/// (schema line 12056) instead of letting it silently 400.
pub fn get_message_properties(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getMessageProperties",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// M1: `resendMessages` (TDLib 1.8.67, `schema/td_api.tl:12251` —
/// `resendMessages chat_id message_ids quote paid_message_star_count =
/// Messages;`). Re-sends messages that failed to send (`message.can_retry`,
/// schema line 3038); `quote` null keeps the original reply context.
pub fn resend_messages(extra: RequestId, chat_id: ChatId, message_ids: &[MessageId]) -> String {
    json!({
        "@type": "resendMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "quote": Value::Null,
        "paid_message_star_count": 0,
    })
    .to_string()
}

/// M1: `getChatScheduledMessages` (TDLib 1.8.67, `schema/td_api.tl:12000` —
/// `getChatScheduledMessages chat_id:int53 = Messages;`).
pub fn get_chat_scheduled_messages(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatScheduledMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `reactionTypeEmoji` (TDLib 1.8.67). Custom / paid stay out of this slice.
pub fn reaction_type_emoji(emoji: &str) -> Value {
    json!({
        "@type": "reactionTypeEmoji",
        "emoji": emoji
    })
}

/// `addMessageReaction` (TDLib 1.8.67). Chip / picker add: `is_big` false
/// (tdesktop InlineList click, not the big-animation double-click).
/// `update_recent_reactions` true matches the official picker.
pub fn add_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    emoji: &str,
    is_big: bool,
    update_recent_reactions: bool,
) -> String {
    json!({
        "@type": "addMessageReaction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reaction_type": reaction_type_emoji(emoji),
        "is_big": is_big,
        "update_recent_reactions": update_recent_reactions
    })
    .to_string()
}

/// `removeMessageReaction` (TDLib 1.8.67). A chosen reaction can always be
/// removed (schema). Official chip click on `is_chosen` sends this.
pub fn remove_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    emoji: &str,
) -> String {
    json!({
        "@type": "removeMessageReaction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reaction_type": reaction_type_emoji(emoji)
    })
    .to_string()
}

/// `getCallbackQueryAnswer` (TDLib 1.8.67, `schema/td_api.tl:13138`).
/// Pressing an `inlineKeyboardButtonTypeCallback` button: sends the callback
/// query to the bot; TDLib returns `callbackQueryAnswer`. (Not
/// `answerCallbackQuery` — that one is bots-only per its schema doc
/// comment.) `payload` is `callbackQueryPayloadData` (line 7737); schema
/// `bytes` is base64 in the JSON interface.
pub fn get_callback_query_answer(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    data: &[u8],
) -> String {
    use base64::Engine;
    get_callback_query_answer_payload(
        extra,
        chat_id,
        message_id,
        json!({
            "@type": "callbackQueryPayloadData",
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        }),
    )
}

/// B1: `getCallbackQueryAnswer` for an
/// `inlineKeyboardButtonTypeCallbackWithPassword` button press. `payload`
/// is `callbackQueryPayloadDataWithPassword` (schema 1.8.67, line 7740).
pub fn get_callback_query_answer_with_password(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    password: &str,
    data: &[u8],
) -> String {
    use base64::Engine;
    get_callback_query_answer_payload(
        extra,
        chat_id,
        message_id,
        json!({
            "@type": "callbackQueryPayloadDataWithPassword",
            "password": password,
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        }),
    )
}

/// B1: `getCallbackQueryAnswer` for an `inlineKeyboardButtonTypeCallbackGame`
/// button press. `payload` is `callbackQueryPayloadGame` (schema 1.8.67,
/// line 7743); `game_short_name` comes from the message's `messageGame`
/// content (schema:5234 / game class schema:673). A `sendGame` constructor
/// does not exist in this schema — the game launches through this callback
/// query (TGX `TGInlineKeyboard` does exactly this).
pub fn get_callback_query_answer_game(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    game_short_name: &str,
) -> String {
    get_callback_query_answer_payload(
        extra,
        chat_id,
        message_id,
        json!({
            "@type": "callbackQueryPayloadGame",
            "game_short_name": game_short_name,
        }),
    )
}

fn get_callback_query_answer_payload(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    payload: Value,
) -> String {
    json!({
        "@type": "getCallbackQueryAnswer",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "payload": payload,
    })
    .to_string()
}

/// B1: `getLoginUrlInfo` (TDLib 1.8.67, `schema/td_api.tl:12985`) — resolve
/// an `inlineKeyboardButtonTypeLoginUrl` button (`id`, schema:3780) to the
/// authorized URL. Response is `loginUrlInfo*`.
pub fn get_login_url_info(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    button_id: i64,
) -> String {
    json!({
        "@type": "getLoginUrlInfo",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "button_id": button_id,
    })
    .to_string()
}

/// B1: `getLoginUrl` (TDLib 1.8.67, `schema/td_api.tl:12993`) — the
/// authorized URL after the user consented to a
/// `loginUrlInfoRequestConfirmation`. Response is `httpUrl`.
pub fn get_login_url(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    button_id: i64,
    allow_write_access: bool,
) -> String {
    json!({
        "@type": "getLoginUrl",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "button_id": button_id,
        "allow_write_access": allow_write_access,
    })
    .to_string()
}

/// B1: `deleteChatReplyMarkup` (TDLib 1.8.67, `schema/td_api.tl:13183`).
/// Must be called after a one-time custom keyboard has been used.
pub fn delete_chat_reply_markup(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
) -> String {
    json!({
        "@type": "deleteChatReplyMarkup",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// `pinChatMessage` (TDLib 1.8.67). Official Pin: notify when the chat allows
/// it (`disable_notification` false); pin for everyone (`only_for_self` false).
/// Schema: notifications are always disabled in channels and private chats.
pub fn pin_chat_message(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    disable_notification: bool,
    only_for_self: bool,
) -> String {
    json!({
        "@type": "pinChatMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "disable_notification": disable_notification,
        "only_for_self": only_for_self
    })
    .to_string()
}

/// `setChatSlowModeDelay` (TDLib 1.8.67, `schema/td_api.tl:13551`).
/// `slow_mode_delay` must be one of 0, 5, 10, 30, 60, 300, 900, 3600
/// (0 = off); available only for supergroups and requires the
/// `can_restrict_members` administrator right. The new delay arrives via
/// `updateSupergroupFullInfo`.
pub fn set_chat_slow_mode_delay(extra: RequestId, chat_id: ChatId, slow_mode_delay: i32) -> String {
    json!({
        "@type": "setChatSlowModeDelay",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "slow_mode_delay": slow_mode_delay,
    })
    .to_string()
}

/// `setChatNotificationSettings` (TDLib 1.8.67). Full settings object; callers
/// copy the chat's current settings and change only `mute_for`.
pub fn set_chat_notification_settings(
    extra: RequestId,
    chat_id: ChatId,
    settings: &crate::telegram::envelope::ChatNotificationSettings,
) -> String {
    json!({
        "@type": "setChatNotificationSettings",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "notification_settings": {
            "@type": "chatNotificationSettings",
            "use_default_mute_for": settings.use_default_mute_for,
            "mute_for": settings.mute_for,
            "use_default_sound": settings.use_default_sound,
            "sound_id": settings.sound_id,
            "use_default_show_preview": settings.use_default_show_preview,
            "show_preview": settings.show_preview,
            "use_default_mute_stories": settings.use_default_mute_stories,
            "mute_stories": settings.mute_stories,
            "use_default_story_sound": settings.use_default_story_sound,
            "story_sound_id": settings.story_sound_id,
            "use_default_show_story_poster": settings.use_default_show_story_poster,
            "show_story_poster": settings.show_story_poster,
            "use_default_disable_pinned_message_notifications": settings.use_default_disable_pinned_message_notifications,
            "disable_pinned_message_notifications": settings.disable_pinned_message_notifications,
            "use_default_disable_mention_notifications": settings.use_default_disable_mention_notifications,
            "disable_mention_notifications": settings.disable_mention_notifications
        }
    })
    .to_string()
}

/// `getSavedNotificationSounds` (TDLib 1.8.67, line 13647): the user's saved
/// notification sounds. "If a sound isn't in the list, then default sound
/// needs to be used."
pub fn get_saved_notification_sounds(extra: RequestId) -> String {
    json!({
        "@type": "getSavedNotificationSounds",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `getScopeNotificationSettings` (TDLib 1.8.67, line 13662).
pub fn get_scope_notification_settings(
    extra: RequestId,
    scope: crate::telegram::envelope::NotificationSettingsScope,
) -> String {
    json!({
        "@type": "getScopeNotificationSettings",
        "@extra": extra.as_extra(),
        "scope": { "@type": scope.type_name() },
    })
    .to_string()
}

/// `setScopeNotificationSettings` (TDLib 1.8.67, line 13665). Full settings
/// object; callers copy the scope's current settings and change one field.
pub fn set_scope_notification_settings(
    extra: RequestId,
    scope: crate::telegram::envelope::NotificationSettingsScope,
    settings: &crate::telegram::envelope::ScopeNotificationSettings,
) -> String {
    json!({
        "@type": "setScopeNotificationSettings",
        "@extra": extra.as_extra(),
        "scope": { "@type": scope.type_name() },
        "notification_settings": {
            "@type": "scopeNotificationSettings",
            "mute_for": settings.mute_for,
            "sound_id": settings.sound_id,
            "show_preview": settings.show_preview,
            "use_default_mute_stories": settings.use_default_mute_stories,
            "mute_stories": settings.mute_stories,
            "story_sound_id": settings.story_sound_id,
            "show_story_poster": settings.show_story_poster,
            "disable_pinned_message_notifications": settings.disable_pinned_message_notifications,
            "disable_mention_notifications": settings.disable_mention_notifications
        }
    })
    .to_string()
}

/// `addChatToList` (TDLib 1.8.67). Main and Archive are mutually exclusive.
/// `sendChatAction` (TDLib 1.8.67). `typing` sends `chatActionTyping`;
/// otherwise `chatActionCancel` (Unigram `CancelTyping`). `topic_id` null,
/// `business_connection_id` empty (not a bot business connection).
pub fn send_chat_action(extra: RequestId, chat_id: ChatId, typing: bool) -> String {
    send_chat_action_kind(
        extra,
        chat_id,
        if typing {
            "chatActionTyping"
        } else {
            "chatActionCancel"
        },
    )
}

/// `sendChatAction` with an explicit `ChatAction` constructor (1.8.67).
pub fn send_chat_action_kind(extra: RequestId, chat_id: ChatId, action: &str) -> String {
    json!({
        "@type": "sendChatAction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "business_connection_id": "",
        "action": { "@type": action }
    })
    .to_string()
}

/// `sendMessage` + `inputMessageVoiceNote` / `inputVoiceNote` / `inputFileLocal`.
/// `path` must already be an explicitly recorded or picked file.
/// `waveform_b64` is the 5-bit waveform as TDLib `bytes` (base64); empty if unknown.
/// Voice-note send parameters. Bundled into a struct so the send constructor
/// stays under clippy's argument limit as topic/reply support grows.
pub struct VoiceNoteSend<'a> {
    pub path: &'a str,
    pub duration: i32,
    pub waveform_b64: &'a str,
    pub caption: &'a str,
    pub reply_to: Option<SendReply>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `sendMessage` + `inputMessageVoiceNote` / `inputVoiceNote` / `inputFileLocal`.
/// `path` must already be an explicitly recorded or picked file.
/// `waveform_b64` is the 5-bit waveform as TDLib `bytes` (base64); empty if unknown.
pub fn send_voice_note(extra: RequestId, chat_id: ChatId, voice: VoiceNoteSend<'_>) -> String {
    let caption_json = if voice.caption.is_empty() {
        Value::Null
    } else {
        json!({
            "@type": "formattedText",
            "text": voice.caption,
            "entities": []
        })
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(voice.topic_id),
        "reply_to": send_reply_value(voice.reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageVoiceNote",
            "voice_note": {
                "@type": "inputVoiceNote",
                "voice_note": {
                    "@type": "inputFileLocal",
                    "path": voice.path
                },
                "duration": voice.duration,
                "waveform": voice.waveform_b64
            },
            "caption": caption_json,
            "self_destruct_type": Value::Null
        }
    })
    .to_string()
}

/// `openMessageContent` — user started listening to a voice note (1.8.67).
pub fn open_message_content(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "openMessageContent",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0
    })
    .to_string()
}

/// MED2: `recognizeSpeech` (TDLib 1.8.67, `schema/td_api.tl:12181`).
/// Recognizes speech in a voice note or video note message. Returns `Ok`;
/// the result arrives later as `updateMessageContent` carrying the new
/// `speech_recognition_result` (`speechRecognitionResultPending` →
/// `speechRecognitionResultText` / `speechRecognitionResultError`).
pub fn recognize_speech(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "recognizeSpeech",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0
    })
    .to_string()
}

/// Phase 9.1: `loadActiveStories` (TDLib 1.8.67, `schema/td_api.tl:13762`).
/// The loaded stories arrive as `updateChatActiveStories` updates — they
/// feed the story tray above the chat list.
pub fn load_active_stories(extra: RequestId) -> String {
    json!({
        "@type": "loadActiveStories",
        "@extra": extra.as_extra(),
        "story_list": {"@type": "storyListMain"}
    })
    .to_string()
}

/// Phase 9.1: `getChatActiveStories` (TDLib 1.8.67,
/// `schema/td_api.tl:13768`). Response is `chatActiveStories`; handled like
/// the `updateChatActiveStories` update.
pub fn get_chat_active_stories(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatActiveStories",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0
    })
    .to_string()
}

/// Phase 9.1: `getStory` (TDLib 1.8.67, `schema/td_api.tl:13695`).
/// `only_local: false` — the viewer wants the full content.
pub fn get_story(extra: RequestId, chat_id: ChatId, story_id: i32) -> String {
    json!({
        "@type": "getStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "only_local": false
    })
    .to_string()
}

/// Phase 9.1: `openStory` (TDLib 1.8.67, `schema/td_api.tl:13794`) — the
/// user opened a story for viewing. Response is `ok`.
pub fn open_story(extra: RequestId, chat_id: ChatId, story_id: i32) -> String {
    json!({
        "@type": "openStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id
    })
    .to_string()
}

/// Phase 9.1: `closeStory` (TDLib 1.8.67, `schema/td_api.tl:13799`) — the
/// user closed a story. Response is `ok`.
pub fn close_story(extra: RequestId, chat_id: ChatId, story_id: i32) -> String {
    json!({
        "@type": "closeStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id
    })
    .to_string()
}

/// Phase 9.2: `getStoryAvailableReactions` (TDLib 1.8.67,
/// `schema/td_api.tl:13802`) — emoji reactions the story picker can offer.
/// `row_size` must be 5–25; the viewer requests 10. Response is
/// `availableReactions`.
pub fn get_story_available_reactions(extra: RequestId, row_size: i32) -> String {
    json!({
        "@type": "getStoryAvailableReactions",
        "@extra": extra.as_extra(),
        "row_size": row_size
    })
    .to_string()
}

/// Phase 9.2: `setStoryReaction` (TDLib 1.8.67, `schema/td_api.tl:13809`) —
/// changes the user's chosen reaction on a story. `emoji: None` removes the
/// reaction (`reaction_type: null`); `Some("❤")` sets it. Only
/// `reactionTypeEmoji` is offered (custom emoji is Premium-only; paid
/// reactions can't be set — schema comment). `update_recent_reactions: true`
/// matches the official picker click. Not supported for live stories (the
/// driver gates that). Response is `ok`.
pub fn set_story_reaction(
    extra: RequestId,
    chat_id: ChatId,
    story_id: i32,
    emoji: Option<&str>,
) -> String {
    let reaction_type = match emoji {
        Some(emoji) => reaction_type_emoji(emoji),
        None => Value::Null,
    };
    json!({
        "@type": "setStoryReaction",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "reaction_type": reaction_type,
        "update_recent_reactions": true
    })
    .to_string()
}

/// Phase 9.2: `deleteStory` (TDLib 1.8.67, `schema/td_api.tl:13754`) —
/// deletes a story posted by the current user (`story.can_be_deleted`
/// gates the button). Response is `ok`; the deletion lands as
/// `updateStoryDeleted`.
pub fn delete_story(extra: RequestId, chat_id: ChatId, story_id: i32) -> String {
    json!({
        "@type": "deleteStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id
    })
    .to_string()
}

/// Phase 9.2: story reply target — `inputMessageReplyToStory` (TDLib 1.8.67,
/// `schema/td_api.tl:3099`). Replying to a story sends a message to the
/// story poster quoting the story.
pub fn input_message_reply_to_story(poster_chat_id: ChatId, story_id: i32) -> Value {
    json!({
        "@type": "inputMessageReplyToStory",
        "story_poster_chat_id": poster_chat_id.0,
        "story_id": story_id
    })
}

/// Phase 9.2: `sendMessage` with `inputMessageReplyToStory` — a reply to a
/// story, sent to the poster chat (`story.can_be_replied` gates it).
pub fn send_text_story_reply(
    extra: RequestId,
    chat_id: ChatId,
    poster_chat_id: ChatId,
    story_id: i32,
    text: &str,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "reply_to": input_message_reply_to_story(poster_chat_id, story_id),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageText",
            "text": {
                "@type": "formattedText",
                "text": text,
                "entities": []
            },
            "link_preview_options": Value::Null,
            "clear_draft": true
        }
    })
    .to_string()
}

/// Phase 9.3: `canPostStory` (TDLib 1.8.67, `schema/td_api.tl:13702`) —
/// `canPostStory chat_id:int53 = CanPostStoryResult;` The composer sends
/// it with the Saved Messages chat id (`Session::my_user_id`) before
/// every post.
pub fn can_post_story(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "canPostStory",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0
    })
    .to_string()
}

/// Phase 9.3: `InputStoryContent` for `postStory` (TDLib 1.8.67,
/// `schema/td_api.tl:6673` / `td_api.tl:6681`). Video `duration` is 0 —
/// TDLib derives the real duration from the file during upload.
pub fn input_story_content(kind: StoryMediaKind, path: &str) -> Value {
    let file = json!({ "@type": "inputFileLocal", "path": path });
    match kind {
        StoryMediaKind::Photo => json!({
            "@type": "inputStoryContentPhoto",
            "photo": file,
            "added_sticker_file_ids": []
        }),
        _ => json!({
            "@type": "inputStoryContentVideo",
            "video": file,
            "added_sticker_file_ids": [],
            "duration": 0.0,
            "cover_frame_timestamp": 0.0,
            "is_animation": false
        }),
    }
}

/// Phase 9.3 / 9.4: `postStory` (TDLib 1.8.67, `schema/td_api.tl:13715`)
/// — posts a photo/video story as the current user (the Saved Messages
/// chat id, `Session::my_user_id`). The caption gets the same
/// markup→entities treatment as message captions (`formatted_caption`);
/// `from_story_full_id` is null (not a repost — schema comment: "pass
/// null if the story isn't repost of another story"). Phase 9.4 wires
/// the previously fixed fields: `areas` (`inputStoryAreas`,
/// `td_api.tl:6619`, built by `StoryComposer::areas_json`),
/// `active_period` (one of 21600 / 43200 / 86400 / 172800 per the
/// parameter comment — validated in `ConnectDriver::post_story`),
/// `is_posted_to_chat_page` ("Pass true to keep the story accessible
/// after expiration") and `protect_content` ("Pass true if the content
/// of the story must be protected from forwarding and screenshotting").
/// Response is a `story`; success/failure lands via
/// `updateStoryPostSucceeded` / `updateStoryPostFailed`.
#[allow(clippy::too_many_arguments)] // one arg per schema field, like the other request builders
pub fn post_story(
    extra: RequestId,
    chat_id: ChatId,
    kind: StoryMediaKind,
    path: &str,
    caption: &str,
    privacy_settings: Value,
    areas: Value,
    active_period: i32,
    is_posted_to_chat_page: bool,
    protect_content: bool,
) -> String {
    json!({
        "@type": "postStory",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "content": input_story_content(kind, path),
        "areas": areas,
        "caption": formatted_caption(caption, false),
        "privacy_settings": privacy_settings,
        "album_ids": [],
        "active_period": active_period,
        "from_story_full_id": Value::Null,
        "is_posted_to_chat_page": is_posted_to_chat_page,
        "protect_content": protect_content
    })
    .to_string()
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

/// The `chatFolder` constructor body (TDLib 1.8.67, `schema/td_api.tl:3476`)
/// for `createChatFolder` / `editChatFolder`. Quill sends `icon: null`
/// (default icon; `getChatFolderDefaultIconName` is an async TDLib call
/// clients may use — the null default is what the schema allows),
/// rendering and folder invite links are out of scope. Folder names are
/// 1–12 characters without line feeds (schema doc on `chatFolderName`).
pub fn chat_folder_json(spec: &crate::telegram::envelope::ChatFolderSpec) -> Value {
    json!({
        "@type": "chatFolder",
        "name": {
            "@type": "chatFolderName",
            "text": { "@type": "formattedText", "text": spec.name, "entities": [] },
            "animate_custom_emoji": false,
        },
        "icon": null,
        "color_id": -1,
        "is_shareable": false,
        "pinned_chat_ids": spec.pinned_chat_ids,
        "included_chat_ids": spec.included_chat_ids,
        "excluded_chat_ids": spec.excluded_chat_ids,
        "exclude_muted": spec.exclude_muted,
        "exclude_read": spec.exclude_read,
        "exclude_archived": spec.exclude_archived,
        "include_contacts": spec.include_contacts,
        "include_non_contacts": spec.include_non_contacts,
        "include_bots": spec.include_bots,
        "include_groups": spec.include_groups,
        "include_channels": spec.include_channels,
    })
}

/// `createChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13358`). Response is
/// `chatFolderInfo`.
pub fn create_chat_folder(
    extra: RequestId,
    spec: &crate::telegram::envelope::ChatFolderSpec,
) -> String {
    json!({
        "@type": "createChatFolder",
        "@extra": extra.as_extra(),
        "folder": chat_folder_json(spec),
    })
    .to_string()
}

/// `editChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13361`). Response is
/// `chatFolderInfo`.
pub fn edit_chat_folder(
    extra: RequestId,
    folder_id: i32,
    spec: &crate::telegram::envelope::ChatFolderSpec,
) -> String {
    json!({
        "@type": "editChatFolder",
        "@extra": extra.as_extra(),
        "chat_folder_id": folder_id,
        "folder": chat_folder_json(spec),
    })
    .to_string()
}

/// `deleteChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13364`). Response is
/// `ok`. `leave_chat_ids` are chats to leave; they must be pinned or
/// always included in the folder (empty = keep all chats in the main list).
pub fn delete_chat_folder(extra: RequestId, folder_id: i32, leave_chat_ids: &[i64]) -> String {
    json!({
        "@type": "deleteChatFolder",
        "@extra": extra.as_extra(),
        "chat_folder_id": folder_id,
        "leave_chat_ids": leave_chat_ids,
    })
    .to_string()
}

/// `reorderChatFolders` (TDLib 1.8.67, `schema/td_api.tl:13373`). Response
/// is `ok`. `main_chat_list_position` is always 0 — a non-zero position is
/// Premium-only per the schema doc, and Quill keeps Main first.
pub fn reorder_chat_folders(extra: RequestId, folder_ids: &[i32]) -> String {
    json!({
        "@type": "reorderChatFolders",
        "@extra": extra.as_extra(),
        "chat_folder_ids": folder_ids,
        "main_chat_list_position": 0,
    })
    .to_string()
}

/// `toggleChatFolderTags` (TDLib 1.8.67, `schema/td_api.tl:13376`).
/// Response is `ok`.
pub fn toggle_chat_folder_tags(extra: RequestId, are_tags_enabled: bool) -> String {
    json!({
        "@type": "toggleChatFolderTags",
        "@extra": extra.as_extra(),
        "are_tags_enabled": are_tags_enabled,
    })
    .to_string()
}

/// `getChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13355`). Response is
/// the full `chatFolder` spec — used to prefill the edit dialog.
pub fn get_chat_folder(extra: RequestId, folder_id: i32) -> String {
    json!({
        "@type": "getChatFolder",
        "@extra": extra.as_extra(),
        "chat_folder_id": folder_id,
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

/// `unpinChatMessage` (TDLib 1.8.67). Removes one pinned message.
pub fn unpin_chat_message(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "unpinChatMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0
    })
    .to_string()
}

pub fn runtime_version_request() -> String {
    json!({
        "@type": "getOption",
        "name": "version",
    })
    .to_string()
}

pub fn expected_runtime_label() -> String {
    format!("{TDLIB_CMAKE_VERSION} ({TDLIB_GIT_COMMIT})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::RequestId;

    /// Slice A2: the five 2FA request shapes against the pinned schema
    /// (1.8.67): `getPasswordState` (:11426), `setPassword` (:11434),
    /// `setRecoveryEmailAddress` (:11458),
    /// `resendRecoveryEmailAddressCode` (:11464),
    /// `cancelRecoveryEmailAddressVerification` (:11467). Passwords and
    /// emails ride the JSON body, never `@extra` or logs.
    #[test]
    fn a2_password_request_shapes_match_1_8_67() {
        let v: serde_json::Value =
            serde_json::from_str(&get_password_state(RequestId(71))).unwrap();
        assert_eq!(v["@type"], "getPasswordState");
        assert_eq!(v["@extra"], "71");

        // Enable: empty old password, new password + hint + recovery email
        // in the same call (TGX MODE_NEW sends null old_password).
        let v: serde_json::Value = serde_json::from_str(&set_password(
            RequestId(72),
            "",
            "s3cret",
            "hint",
            Some("me@example.com"),
        ))
        .unwrap();
        assert_eq!(v["@type"], "setPassword");
        assert_eq!(v["old_password"], "");
        assert_eq!(v["new_password"], "s3cret");
        assert_eq!(v["new_hint"], "hint");
        assert_eq!(v["set_recovery_email_address"], true);
        assert_eq!(v["new_recovery_email_address"], "me@example.com");

        // Disable: empty new password, no recovery-email change.
        let v: serde_json::Value =
            serde_json::from_str(&set_password(RequestId(73), "s3cret", "", "", None)).unwrap();
        assert_eq!(v["@type"], "setPassword");
        assert_eq!(v["old_password"], "s3cret");
        assert_eq!(v["new_password"], "");
        assert_eq!(v["set_recovery_email_address"], false);
        assert_eq!(v["new_recovery_email_address"], "");

        let v: serde_json::Value = serde_json::from_str(&set_recovery_email_address(
            RequestId(74),
            "s3cret",
            "new@example.com",
        ))
        .unwrap();
        assert_eq!(v["@type"], "setRecoveryEmailAddress");
        assert_eq!(v["@extra"], "74");
        assert_eq!(v["password"], "s3cret");
        assert_eq!(v["new_recovery_email_address"], "new@example.com");

        let v: serde_json::Value =
            serde_json::from_str(&resend_recovery_email_address_code(RequestId(75))).unwrap();
        assert_eq!(v["@type"], "resendRecoveryEmailAddressCode");
        assert_eq!(v["@extra"], "75");

        let v: serde_json::Value =
            serde_json::from_str(&cancel_recovery_email_address_verification(RequestId(76)))
                .unwrap();
        assert_eq!(v["@type"], "cancelRecoveryEmailAddressVerification");
        assert_eq!(v["@extra"], "76");
    }

    #[test]
    fn cl2_set_pinned_chats_shape_matches_1_8_67() {
        // Slice CL2: `setPinnedChats chat_list:ChatList
        // chat_ids:vector<int53> = Ok;` (schema 1.8.67, line 13681) —
        // the full new pinned order, not a delta.
        let json = set_pinned_chats(RequestId(51), false, &[11, 12, 13]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setPinnedChats");
        assert_eq!(v["@extra"], "51");
        assert_eq!(v["chat_list"]["@type"], "chatListMain");
        assert_eq!(v["chat_ids"], serde_json::json!([11, 12, 13]));

        let archived = set_pinned_chats(RequestId(52), true, &[7]);
        let v: serde_json::Value = serde_json::from_str(&archived).unwrap();
        assert_eq!(v["chat_list"]["@type"], "chatListArchive");
        assert_eq!(v["chat_ids"], serde_json::json!([7]));
    }

    #[test]
    fn cl2_read_chat_list_shape_matches_1_8_67() {
        // Slice CL2: `readChatList chat_list:ChatList = Ok;` (schema
        // 1.8.67, line 13684).
        let json = read_chat_list(RequestId(53), false);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "readChatList");
        assert_eq!(v["@extra"], "53");
        assert_eq!(v["chat_list"]["@type"], "chatListMain");

        let archived = read_chat_list(RequestId(54), true);
        let v: serde_json::Value = serde_json::from_str(&archived).unwrap();
        assert_eq!(v["chat_list"]["@type"], "chatListArchive");
    }

    #[test]
    fn cl2_clear_recently_found_chats_shape_matches_1_8_67() {
        // Slice CL2: `clearRecentlyFoundChats = Ok;` (schema 1.8.67,
        // line 11671) — no fields.
        let json = clear_recently_found_chats(RequestId(55));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "clearRecentlyFoundChats");
        assert_eq!(v["@extra"], "55");
        assert!(v.get("query").is_none());
    }

    #[test]
    fn a3_session_request_shapes_match_1_8_67() {
        // Slice A3: `getActiveSessions = Sessions;` (line 15102),
        // `terminateSession session_id:int64 = Ok;` (line 15105),
        // `terminateAllOtherSessions = Ok;` (line 15108).
        let v: serde_json::Value =
            serde_json::from_str(&get_active_sessions(RequestId(71))).unwrap();
        assert_eq!(v["@type"], "getActiveSessions");
        assert_eq!(v["@extra"], "71");

        let v: serde_json::Value =
            serde_json::from_str(&terminate_session(RequestId(72), 123456789)).unwrap();
        assert_eq!(v["@type"], "terminateSession");
        assert_eq!(v["@extra"], "72");
        assert_eq!(v["session_id"], 123456789);

        let v: serde_json::Value =
            serde_json::from_str(&terminate_all_other_sessions(RequestId(73))).unwrap();
        assert_eq!(v["@type"], "terminateAllOtherSessions");
        assert_eq!(v["@extra"], "73");
    }

    #[test]
    fn cl2_create_private_chat_shape_matches_1_8_67() {
        // Slice CL2: `createPrivateChat user_id:int53 force:Bool =
        // Chat;` (schema 1.8.67, line 13312).
        let json = create_private_chat(RequestId(58), 777, false);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "createPrivateChat");
        assert_eq!(v["@extra"], "58");
        assert_eq!(v["user_id"], 777);
        assert_eq!(v["force"], false);
    }

    #[test]
    fn cl2_archive_chat_list_settings_shapes_match_1_8_67() {
        // Slice CL2: `getArchiveChatListSettings =
        // ArchiveChatListSettings;` (schema 1.8.67, line 13421) and
        // `setArchiveChatListSettings settings:archiveChatListSettings =
        // Ok;` (line 13424); the settings object shape is line 3512.
        let json = get_archive_chat_list_settings(RequestId(56));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getArchiveChatListSettings");
        assert_eq!(v["@extra"], "56");

        let settings = ArchiveChatListSettings {
            archive_and_mute_new_chats_from_unknown_users: true,
            keep_unmuted_chats_archived: false,
            keep_chats_from_folders_archived: true,
        };
        let json = set_archive_chat_list_settings(RequestId(57), settings);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setArchiveChatListSettings");
        assert_eq!(v["@extra"], "57");
        assert_eq!(v["settings"]["@type"], "archiveChatListSettings");
        assert_eq!(
            v["settings"]["archive_and_mute_new_chats_from_unknown_users"],
            true
        );
        assert_eq!(v["settings"]["keep_unmuted_chats_archived"], false);
        assert_eq!(v["settings"]["keep_chats_from_folders_archived"], true);
    }

    #[test]
    fn set_chat_message_auto_delete_time_shape_matches_1_8_67() {
        // Phase B4: `setChatMessageAutoDeleteTime chat_id:int53
        // message_auto_delete_time:int32 = Ok` (schema 1.8.67, line
        // 13454).
        let json = set_chat_message_auto_delete_time(RequestId(21), 41, 3600);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setChatMessageAutoDeleteTime");
        assert_eq!(v["@extra"], "21");
        assert_eq!(v["chat_id"], 41);
        assert_eq!(v["message_auto_delete_time"], 3600);

        let off = set_chat_message_auto_delete_time(RequestId(22), 11, 0);
        let v: serde_json::Value = serde_json::from_str(&off).unwrap();
        assert_eq!(v["message_auto_delete_time"], 0);
    }

    #[test]
    fn toggle_session_can_accept_secret_chats_shape_matches_1_8_67() {
        // Phase S1: `toggleSessionCanAcceptSecretChats session_id:int64
        // can_accept_secret_chats:Bool = Ok` (schema 1.8.67, line 15117);
        // int64 serializes as a JSON string like other int64 ids here.
        let json = toggle_session_can_accept_secret_chats(RequestId(31), 123456789, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "toggleSessionCanAcceptSecretChats");
        assert_eq!(v["@extra"], "31");
        assert_eq!(v["session_id"], "123456789");
        assert_eq!(v["can_accept_secret_chats"], true);

        let off = toggle_session_can_accept_secret_chats(RequestId(32), 123456789, false);
        let v: serde_json::Value = serde_json::from_str(&off).unwrap();
        assert_eq!(v["can_accept_secret_chats"], false);
    }

    #[test]
    fn get_storage_statistics_shape_matches_1_8_67() {
        // Phase S2: `getStorageStatistics chat_limit:int32 =
        // StorageStatistics` (schema 1.8.67, line 15781).
        let json = get_storage_statistics(RequestId(41), 0);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getStorageStatistics");
        assert_eq!(v["@extra"], "41");
        assert_eq!(v["chat_limit"], 0);
    }

    #[test]
    fn get_commands_shape_matches_1_8_67() {
        // `getCommands scope:BotCommandScope language_code:string =
        // BotCommands` (schema 1.8.67 line 14953); a null scope selects the
        // default scope (`botCommandScopeDefault`, line 10360) and an empty
        // language code is allowed.
        let json = get_commands(RequestId(9));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getCommands");
        assert_eq!(v["@extra"], "9");
        assert!(v["scope"].is_null());
        assert_eq!(v["language_code"], "");
    }

    #[test]
    fn send_text_includes_topic_id_null() {
        let json = send_text(
            RequestId(9),
            ChatId(1),
            None,
            "hi",
            None,
            &SendOptions::default(),
        );
        assert!(json.contains("\"topic_id\":null"));
        assert!(!json.contains("message_thread_id"));
        assert!(json.contains("\"@extra\":\"9\""));
        assert!(json.contains("\"reply_to\":null"));
    }

    #[test]
    fn send_rich_message_shape_matches_1_8_67() {
        // M2: `sendMessage` + `inputMessageRichMessage message:inputRichMessage
        // clear_draft:Bool = InputMessageContent` (schema 1.8.67, line 6084).
        let rich = crate::rich::input_rich_message(&[crate::rich::RichBlock::Paragraph {
            text: "hi".into(),
            entities: Vec::new(),
            buttons: Vec::new(),
        }])
        .expect("blocks");
        let json = send_rich_message(
            RequestId(60),
            ChatId(11),
            None,
            &rich,
            None,
            &SendOptions {
                disable_notification: true,
                ..SendOptions::default()
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "60");
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessageRichMessage");
        assert_eq!(content["clear_draft"], true);
        assert_eq!(content["message"]["@type"], "inputRichMessage");
        assert_eq!(
            content["message"]["source"]["blocks"][0]["@type"],
            "inputPageBlockParagraph"
        );
        assert_eq!(v["options"]["disable_notification"], true);
        assert!(v["reply_markup"].is_null());
    }

    #[test]
    fn get_full_rich_message_shape_matches_1_8_67() {
        // M2: `getFullRichMessage chat_id:int53 message_id:int53 =
        // RichMessage` (schema 1.8.67, line 11554).
        let json = get_full_rich_message(RequestId(61), ChatId(11), MessageId(22));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getFullRichMessage");
        assert_eq!(v["@extra"], "61");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 22);
    }

    #[test]
    fn send_text_topic_id_uses_message_topic_forum() {
        // Parity slice 4: `sendMessage.topic_id` (schema 1.8.67, line 12200)
        // takes `messageTopicForum{forum_topic_id}` (line 3004).
        let json = send_text(
            RequestId(9),
            ChatId(16),
            Some(2),
            "hi",
            None,
            &SendOptions::default(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["chat_id"], 16);
        assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
        assert_eq!(v["topic_id"]["forum_topic_id"], 2);
    }

    #[test]
    fn send_text_reply_uses_input_message_reply_to_message() {
        let json = send_text(
            RequestId(10),
            ChatId(11),
            None,
            "sounds good",
            Some(SendReply::plain(MessageId(101))),
            &SendOptions::default(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 101);
        assert_eq!(v["reply_to"]["quote"], Value::Null);
        assert_eq!(v["reply_to"]["checklist_task_id"], 0);
        assert_eq!(v["reply_to"]["poll_option_id"], "");
        assert!(!json.contains("inputMessageReplyToExternalMessage"));
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn send_text_disables_link_preview_for_secret_chats() {
        // Phase S1: secret chats never get link previews (TGX default-off).
        let json = send_text(
            RequestId(11),
            ChatId(41),
            None,
            "see https://example.com",
            None,
            &SendOptions {
                link_preview_disabled: true,
                ..SendOptions::default()
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let opts = &v["input_message_content"]["link_preview_options"];
        assert_eq!(opts["@type"], "linkPreviewOptions");
        assert_eq!(opts["is_disabled"], true);

        let json = send_text(
            RequestId(12),
            ChatId(11),
            None,
            "see https://example.com",
            None,
            &SendOptions::default(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["input_message_content"]["link_preview_options"],
            Value::Null
        );
    }

    #[test]
    fn get_chat_administrators_shape_matches_1_8_67() {
        // Phase D3b: `getChatAdministrators chat_id:int53 = ChatAdministrators;`
        // (schema 1.8.67, line 13632).
        let json = get_chat_administrators(RequestId(71), 13);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getChatAdministrators");
        assert_eq!(v["@extra"], "71");
        assert_eq!(v["chat_id"], 13);
        let schema = include_str!("../../schema/td_api.tl");
        let line = schema
            .lines()
            .find(|l| l.starts_with("getChatAdministrators "))
            .expect("getChatAdministrators in schema");
        assert_eq!(
            line,
            "getChatAdministrators chat_id:int53 = ChatAdministrators;"
        );
    }

    #[test]
    fn set_chat_member_status_promote_shape_matches_1_8_67() {
        // Phase D3b: promote shape — `setChatMemberStatus` (schema 1.8.67,
        // line 13592) with `messageSenderUser` (line 2831) and
        // `chatMemberStatusAdministrator` (line 2500) carrying all 18
        // `chatAdministratorRights` fields (line 1092).
        let member_id = MessageSenderRef::User(888).to_value();
        let rights = crate::telegram::envelope::ChatAdminRights {
            can_manage_chat: true,
            can_promote_members: true,
            ..Default::default()
        };
        let status = chat_member_status_administrator_json(true, &rights);
        let json = set_chat_member_status(RequestId(72), 13, &member_id, &status);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setChatMemberStatus");
        assert_eq!(v["@extra"], "72");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["member_id"]["@type"], "messageSenderUser");
        assert_eq!(v["member_id"]["user_id"], 888);
        assert_eq!(v["status"]["@type"], "chatMemberStatusAdministrator");
        assert_eq!(v["status"]["can_be_edited"], true);
        let schema = include_str!("../../schema/td_api.tl");
        let rights_line = schema
            .lines()
            .find(|l| l.starts_with("chatAdministratorRights "))
            .expect("chatAdministratorRights in schema");
        // Every schema field of chatAdministratorRights must be present.
        for field in rights_line
            .split_whitespace()
            .skip(1)
            .take_while(|token| !token.starts_with('='))
        {
            let name = field.split(':').next().unwrap();
            assert!(
                v["status"]["rights"][name].is_boolean(),
                "missing rights field {name}"
            );
        }
        assert_eq!(v["status"]["rights"]["@type"], "chatAdministratorRights");
        assert_eq!(v["status"]["rights"]["can_manage_chat"], true);
        assert_eq!(v["status"]["rights"]["can_promote_members"], true);
        assert_eq!(v["status"]["rights"]["can_delete_messages"], false);
    }

    #[test]
    fn set_chat_member_status_demote_shape_matches_1_8_67() {
        // Phase D3b: demote shape — `setChatMemberStatus` to
        // `chatMemberStatusMember` (schema 1.8.67, line 2504).
        let member_id = MessageSenderRef::User(888).to_value();
        let status = chat_member_status_member_json();
        let json = set_chat_member_status(RequestId(73), 13, &member_id, &status);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setChatMemberStatus");
        assert_eq!(v["status"]["@type"], "chatMemberStatusMember");
        assert_eq!(v["status"]["member_until_date"], 0);
    }

    #[test]
    fn get_supergroup_members_shape_matches_1_8_67() {
        // Phase D3b: `getSupergroupMembers supergroup_id:int53
        // filter:SupergroupMembersFilter offset:int32 limit:int32 =
        // ChatMembers;` (schema 1.8.67, line 15238) with
        // `supergroupMembersFilterSearch` (line 2568).
        let filter = supergroup_members_filter_search_json("ada");
        let json = get_supergroup_members(RequestId(74), 25, &filter, 0, 200);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getSupergroupMembers");
        assert_eq!(v["@extra"], "74");
        assert_eq!(v["supergroup_id"], 25);
        assert_eq!(v["filter"]["@type"], "supergroupMembersFilterSearch");
        assert_eq!(v["filter"]["query"], "ada");
        assert_eq!(v["offset"], 0);
        assert_eq!(v["limit"], 200);

        let recent = supergroup_members_filter_recent_json();
        assert_eq!(recent["@type"], "supergroupMembersFilterRecent");
    }

    #[test]
    fn get_chat_event_log_shape_matches_1_8_67() {
        // Phase D3c: `getChatEventLog chat_id:int53 query:string
        // from_event_id:int64 limit:int32 filters:chatEventLogFilters
        // user_ids:vector<int53> = ChatEvents;` (schema 1.8.67, line
        // 15252). `filters: None` is the schema's `null` = all event
        // types; a filter set serializes as `chatEventLogFilters`
        // (line 7956) with every field present in schema order.
        let json = get_chat_event_log(RequestId(75), 13, "", 0, 100, None, &[]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getChatEventLog");
        assert_eq!(v["@extra"], "75");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["query"], "");
        assert_eq!(v["from_event_id"], 0);
        assert_eq!(v["limit"], 100);
        assert_eq!(v["filters"], Value::Null);
        assert_eq!(v["user_ids"].as_array().unwrap().len(), 0);

        let filters = ChatEventLogFilterSet {
            member_promotions: true,
            invite_link_changes: true,
            ..Default::default()
        };
        let json = get_chat_event_log(RequestId(76), 13, "", 42, 50, Some(filters), &[7]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["from_event_id"], 42);
        assert_eq!(v["limit"], 50);
        assert_eq!(v["filters"]["@type"], "chatEventLogFilters");
        assert_eq!(v["filters"]["member_promotions"], true);
        assert_eq!(v["filters"]["invite_link_changes"], true);
        assert_eq!(v["filters"]["message_edits"], false);
        // Every schema field of chatEventLogFilters must be present.
        let schema = include_str!("../../schema/td_api.tl");
        let filters_line = schema
            .lines()
            .find(|l| l.starts_with("chatEventLogFilters "))
            .expect("chatEventLogFilters in schema");
        for field in filters_line
            .split_whitespace()
            .skip(1)
            .take_while(|token| !token.starts_with('='))
        {
            let name = field.split(':').next().unwrap();
            assert!(
                v["filters"][name].is_boolean(),
                "missing filters field {name}"
            );
        }
        assert_eq!(v["user_ids"], serde_json::json!([7]));
    }

    #[test]
    fn send_photo_shape_matches_1_8_67() {
        let json = send_photo(
            RequestId(11),
            ChatId(7),
            None,
            "/tmp/picked.png",
            "CANARY_CAP",
            false,
            None,
            None,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "11");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessagePhoto");
        assert_eq!(
            v["input_message_content"]["photo"]["photo"]["@type"],
            "inputFileLocal"
        );
        assert_eq!(
            v["input_message_content"]["photo"]["photo"]["path"],
            "/tmp/picked.png"
        );
        assert_eq!(
            v["input_message_content"]["photo"]["thumbnail"],
            Value::Null
        );
        assert_eq!(v["input_message_content"]["photo"]["video"], Value::Null);
        assert_eq!(v["input_message_content"]["photo"]["width"], 0);
        assert_eq!(v["input_message_content"]["photo"]["height"], 0);
        assert_eq!(v["input_message_content"]["caption"]["text"], "CANARY_CAP");
        assert_eq!(
            v["input_message_content"]["show_caption_above_media"],
            false
        );
        assert_eq!(v["input_message_content"]["has_spoiler"], false);
        assert_eq!(
            v["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert!(!json.contains("message_thread_id"));
    }

    /// M1 fix-up: captions parse composer markup into entities, exactly
    /// like message text (`**bold**` in a caption must not go out
    /// literal).
    #[test]
    fn photo_caption_parses_markup_into_entities() {
        let json = send_photo(
            RequestId(61),
            ChatId(7),
            None,
            "/tmp/picked.png",
            "**bold** and plain",
            false,
            None,
            None,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let caption = &v["input_message_content"]["caption"];
        assert_eq!(caption["text"], "bold and plain");
        let entities = caption["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0]["type"]["@type"], "textEntityTypeBold");
        assert_eq!(entities[0]["offset"], 0);
        assert_eq!(entities[0]["length"], 4);
    }

    /// M1 fix-up: `editMessageCaption` parses markup too.
    #[test]
    fn edit_caption_parses_markup_into_entities() {
        let json = edit_message_caption(
            RequestId(62),
            ChatId(11),
            MessageId(60),
            "*italic* cap",
            false,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let caption = &v["caption"];
        assert_eq!(caption["text"], "italic cap");
        let entities = caption["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0]["type"]["@type"], "textEntityTypeItalic");
    }

    /// M1 fix-up: `textEntityTypeBlockQuote` is stripped for secret chats
    /// (schema: unsupported there); other entities survive.
    #[test]
    fn secret_chat_captions_strip_blockquote_only() {
        let json = send_photo(
            RequestId(63),
            ChatId(7),
            None,
            "/tmp/picked.png",
            "> quoted\n**bold**",
            false,
            None,
            None,
            true,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let caption = &v["input_message_content"]["caption"];
        let entities = caption["entities"].as_array().unwrap();
        assert!(
            entities
                .iter()
                .all(|e| e["type"]["@type"] != "textEntityTypeBlockQuote"),
            "blockquote must be stripped for secret chats: {entities:?}"
        );
        assert!(
            entities
                .iter()
                .any(|e| e["type"]["@type"] == "textEntityTypeBold"),
            "non-blockquote entities survive: {entities:?}"
        );
    }

    /// M1 fix-up: `send_text` strips blockquote entities for secret chats.
    #[test]
    fn secret_chat_text_strips_blockquote_only() {
        let json = send_text(
            RequestId(64),
            ChatId(7),
            None,
            "> quoted\n**bold**",
            None,
            &SendOptions {
                is_secret: true,
                ..SendOptions::default()
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let entities = v["input_message_content"]["text"]["entities"]
            .as_array()
            .unwrap();
        assert!(
            entities
                .iter()
                .all(|e| e["type"]["@type"] != "textEntityTypeBlockQuote"),
            "blockquote must be stripped for secret chats: {entities:?}"
        );
        assert!(
            entities
                .iter()
                .any(|e| e["type"]["@type"] == "textEntityTypeBold"),
            "non-blockquote entities survive: {entities:?}"
        );
    }
    /// MED4: `getWebPageInstantView` (schema 1.8.67, line 14797) —
    /// `only_local: false`; a 404 from TDLib means "no Instant View"
    /// and the caller falls back to the browser.
    #[test]
    fn get_web_page_instant_view_shape() {
        let json = get_web_page_instant_view(RequestId(5), "https://example.com/article");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getWebPageInstantView");
        assert_eq!(v["@extra"], "5");
        assert_eq!(v["url"], "https://example.com/article");
        assert_eq!(v["only_local"], false);
    }

    /// MED4: `show_caption_above_media` rides `inputMessagePhoto` /
    /// `inputMessageVideo` (schema 1.8.67, lines 6117/6128).
    #[test]
    fn caption_above_media_wire() {
        let json = send_photo(
            RequestId(11),
            ChatId(7),
            None,
            "/tmp/picked.png",
            "cap",
            true,
            None,
            None,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["input_message_content"]["show_caption_above_media"], true);
        let json = send_video(
            RequestId(17),
            ChatId(7),
            None,
            "/tmp/picked.mp4",
            &VideoSend {
                duration: 1,
                width: 320,
                height: 180,
                supports_streaming: true,
                self_destruct: None,
            },
            "cap",
            true,
            None,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["input_message_content"]["show_caption_above_media"], true);
    }

    /// `inputMessageVideo` (schema 1.8.67, lines 5915/5918/6117/6128).
    #[test]
    fn send_photo_self_destruct_shapes() {
        for (choice, type_name) in [
            (None, None),
            (
                Some(SelfDestructSend::Timer(30)),
                Some("messageSelfDestructTypeTimer"),
            ),
            (
                Some(SelfDestructSend::Immediately),
                Some("messageSelfDestructTypeImmediately"),
            ),
        ] {
            let json = send_photo(
                RequestId(11),
                ChatId(7),
                None,
                "/tmp/picked.png",
                "cap",
                false,
                None,
                choice,
                false,
            );
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            let sd = &v["input_message_content"]["self_destruct_type"];
            match type_name {
                None => assert_eq!(sd, &Value::Null),
                Some(name) => assert_eq!(sd["@type"], name),
            }
        }
        // Timer carries `self_destruct_time`; Immediately carries no fields.
        let json = send_photo(
            RequestId(11),
            ChatId(7),
            None,
            "/tmp/picked.png",
            "cap",
            false,
            None,
            Some(SelfDestructSend::Timer(30)),
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["input_message_content"]["self_destruct_type"]["self_destruct_time"],
            30
        );
        let json = send_video(
            RequestId(17),
            ChatId(7),
            None,
            "/tmp/picked.mp4",
            &VideoSend {
                duration: 1,
                width: 320,
                height: 180,
                supports_streaming: true,
                self_destruct: Some(SelfDestructSend::Immediately),
            },
            "cap",
            false,
            None,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["input_message_content"]["self_destruct_type"]["@type"],
            "messageSelfDestructTypeImmediately"
        );
        assert!(
            v["input_message_content"]["self_destruct_type"]
                .get("self_destruct_time")
                .is_none()
        );
    }

    #[test]
    fn send_sticker_uses_input_file_id_and_int64_set() {
        let json = send_sticker(
            RequestId(13),
            ChatId(7),
            StickerSend {
                file_id: FileId(41),
                emoji: "😀",
                width: 512,
                height: 512,
                thumb: Some((FileId(42), 128, 128)),
                reply_to: Some(SendReply::plain(MessageId(101))),
                topic_id: None,
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["input_message_content"]["@type"], "inputMessageSticker");
        assert_eq!(v["input_message_content"]["emoji"], "😀");
        let sticker = &v["input_message_content"]["sticker"];
        assert_eq!(sticker["@type"], "inputSticker");
        assert_eq!(sticker["sticker"]["@type"], "inputFileId");
        assert_eq!(sticker["sticker"]["id"], 41);
        assert_eq!(sticker["width"], 512);
        assert_eq!(sticker["height"], 512);
        assert_eq!(sticker["thumbnail"]["@type"], "inputThumbnail");
        assert_eq!(sticker["thumbnail"]["thumbnail"]["id"], 42);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        let installed = get_installed_sticker_sets(RequestId(14));
        let installed: serde_json::Value = serde_json::from_str(&installed).unwrap();
        assert_eq!(installed["@type"], "getInstalledStickerSets");
        assert_eq!(installed["sticker_type"]["@type"], "stickerTypeRegular");
        let set = get_sticker_set(RequestId(15), 77);
        let set: serde_json::Value = serde_json::from_str(&set).unwrap();
        assert_eq!(set["set_id"], "77");
        let bare = send_sticker(
            RequestId(16),
            ChatId(7),
            StickerSend {
                file_id: FileId(41),
                emoji: "",
                width: 512,
                height: 512,
                thumb: None,
                reply_to: None,
                topic_id: None,
            },
        );
        let bare: serde_json::Value = serde_json::from_str(&bare).unwrap();
        assert_eq!(
            bare["input_message_content"]["sticker"]["thumbnail"],
            Value::Null
        );
    }

    #[test]
    fn send_animation_uses_input_animation_and_saved_list_has_no_query() {
        let json = send_animation(
            RequestId(21),
            ChatId(7),
            AnimationSend {
                file_id: FileId(33),
                duration: 2,
                width: 240,
                height: 140,
                reply_to: Some(SendReply::plain(MessageId(101))),
                topic_id: None,
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "21");
        assert_eq!(v["input_message_content"]["@type"], "inputMessageAnimation");
        let animation = &v["input_message_content"]["animation"];
        assert_eq!(animation["@type"], "inputAnimation");
        assert_eq!(animation["animation"]["@type"], "inputFileId");
        assert_eq!(animation["animation"]["id"], 33);
        assert_eq!(animation["thumbnail"], Value::Null);
        assert_eq!(animation["added_sticker_file_ids"], serde_json::json!([]));
        assert_eq!(animation["duration"], 2);
        assert_eq!(animation["width"], 240);
        assert_eq!(animation["height"], 140);
        assert_eq!(v["input_message_content"]["caption"], Value::Null);
        assert_eq!(v["input_message_content"]["has_spoiler"], false);
        assert_eq!(v["reply_to"]["message_id"], 101);
        assert!(!json.contains("tenor"));
        let saved = get_saved_animations(RequestId(22));
        let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved["@type"], "getSavedAnimations");
        assert_eq!(saved["@extra"], "22");
        assert!(saved.get("query").is_none());
    }

    #[test]
    fn send_video_shape_matches_1_8_67() {
        let json = send_video(
            RequestId(17),
            ChatId(7),
            None,
            "/tmp/picked.mp4",
            &VideoSend {
                duration: 1,
                width: 320,
                height: 180,
                supports_streaming: true,
                self_destruct: None,
            },
            "CANARY_VIDEO",
            false,
            Some(SendReply::plain(MessageId(9))),
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "17");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageVideo");
        let video = &v["input_message_content"]["video"];
        assert_eq!(video["@type"], "inputVideo");
        assert_eq!(video["video"]["@type"], "inputFileLocal");
        assert_eq!(video["video"]["path"], "/tmp/picked.mp4");
        assert_eq!(video["thumbnail"], Value::Null);
        assert_eq!(video["cover"], Value::Null);
        assert_eq!(video["start_timestamp"], 0);
        assert_eq!(video["added_sticker_file_ids"], serde_json::json!([]));
        assert_eq!(video["duration"], 1);
        assert_eq!(video["width"], 320);
        assert_eq!(video["height"], 180);
        assert_eq!(video["supports_streaming"], true);
        assert_eq!(
            v["input_message_content"]["caption"]["text"],
            "CANARY_VIDEO"
        );
        assert_eq!(
            v["input_message_content"]["show_caption_above_media"],
            false
        );
        assert_eq!(
            v["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert_eq!(v["input_message_content"]["has_spoiler"], false);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 9);
        assert!(!json.contains("inputMessageVideoNote"));
        assert!(!json.contains("api_hash"));
    }

    #[test]
    fn send_video_note_shape_matches_1_8_67() {
        let json = send_video_note(
            RequestId(18),
            ChatId(7),
            None,
            "/tmp/round.mp4",
            &VideoNoteSend {
                duration: 1,
                length: 240,
                thumbnail: Some(VideoNoteThumbnailSend {
                    path: "/tmp/round.jpg".into(),
                    width: 240,
                    height: 240,
                }),
            },
            Some(SendReply::plain(MessageId(9))),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "18");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageVideoNote");
        let note = &v["input_message_content"]["video_note"];
        assert_eq!(note["@type"], "inputVideoNote");
        assert_eq!(note["video_note"]["@type"], "inputFileLocal");
        assert_eq!(note["video_note"]["path"], "/tmp/round.mp4");
        assert_eq!(note["thumbnail"]["@type"], "inputThumbnail");
        assert_eq!(note["thumbnail"]["thumbnail"]["path"], "/tmp/round.jpg");
        assert_eq!(note["thumbnail"]["width"], 240);
        assert_eq!(note["thumbnail"]["height"], 240);
        assert_eq!(note["duration"], 1);
        assert_eq!(note["length"], 240);
        assert_eq!(
            v["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert!(v["input_message_content"].get("caption").is_none());
        assert_eq!(v["reply_to"]["message_id"], 9);
        assert!(!json.contains("api_hash"));

        let bare = send_video_note(
            RequestId(19),
            ChatId(7),
            None,
            "/tmp/round.mp4",
            &VideoNoteSend {
                duration: 0,
                length: 1,
                thumbnail: None,
            },
            None,
        );
        let bare: serde_json::Value = serde_json::from_str(&bare).unwrap();
        assert_eq!(
            bare["input_message_content"]["video_note"]["thumbnail"],
            Value::Null
        );
        assert_eq!(bare["reply_to"], Value::Null);
    }

    #[test]
    fn send_document_shape_matches_1_8_67() {
        let json = send_document(
            RequestId(12),
            ChatId(7),
            None,
            "/tmp/picked.txt",
            "",
            None,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["input_message_content"]["@type"], "inputMessageDocument");
        assert_eq!(
            v["input_message_content"]["document"]["document"]["@type"],
            "inputFileLocal"
        );
        assert_eq!(
            v["input_message_content"]["document"]["document"]["path"],
            "/tmp/picked.txt"
        );
        assert_eq!(
            v["input_message_content"]["document"]["disable_content_type_detection"],
            false
        );
        assert_eq!(v["input_message_content"]["caption"]["text"], "");
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn extra_is_decimal_string_not_float() {
        let json = get_authorization_state(RequestId(9007199254740993));
        assert!(json.contains("\"@extra\":\"9007199254740993\""));
        assert!(!json.contains("\"@extra\":9007199254740993"));
    }

    #[test]
    fn set_phone_shape_does_not_use_message_thread_id() {
        let json = set_authentication_phone_number(RequestId(3), "+10001112222");
        assert!(json.contains("\"@type\":\"setAuthenticationPhoneNumber\""));
        assert!(json.contains("\"phone_number\":\"+10001112222\""));
        assert!(json.contains("phoneNumberAuthenticationSettings"));
        assert!(json.contains("\"@extra\":\"3\""));
    }

    #[test]
    fn check_authentication_code_shape() {
        let json = check_authentication_code(RequestId(4), "12345");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "checkAuthenticationCode");
        assert_eq!(v["@extra"], "4");
        assert_eq!(v["code"], "12345");
    }

    #[test]
    fn check_authentication_password_shape() {
        let json = check_authentication_password(RequestId(5), "unit-test-password");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "checkAuthenticationPassword");
        assert_eq!(v["@extra"], "5");
        assert_eq!(v["password"], "unit-test-password");
    }

    #[test]
    fn resend_authentication_code_shape() {
        let json = resend_authentication_code(RequestId(6));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "resendAuthenticationCode");
        assert_eq!(v["@extra"], "6");
        assert_eq!(v["reason"]["@type"], "resendCodeReasonUserRequest");
    }

    #[test]
    fn request_qr_code_authentication_shape() {
        let json = request_qr_code_authentication(RequestId(7));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "requestQrCodeAuthentication");
        assert_eq!(v["@extra"], "7");
        assert_eq!(v["other_user_ids"], serde_json::json!([]));
    }

    #[test]
    fn view_messages_uses_chat_history_source() {
        let json = view_messages(
            RequestId(6),
            ChatId(7),
            &[MessageId(11), MessageId(12)],
            "messageSourceChatHistory",
            true,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "viewMessages");
        assert_eq!(v["@extra"], "6");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_ids"], serde_json::json!([11, 12]));
        assert_eq!(v["source"]["@type"], "messageSourceChatHistory");
        assert_eq!(v["force_read"], true);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn view_messages_chat_list_source_for_mark_as_read() {
        // Slice CL1: "Mark as read" from the chat list views with
        // `messageSourceChatList`, like Telegram X's
        // `Tdlib.markChatAsRead(..., new MessageSourceChatList(), ...)`.
        let json = view_messages(
            RequestId(6),
            ChatId(7),
            &[MessageId(42)],
            "messageSourceChatList",
            true,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["source"]["@type"], "messageSourceChatList");
        assert_eq!(v["message_ids"], serde_json::json!([42]));
        assert_eq!(v["force_read"], true);
    }

    #[test]
    fn download_file_shape_matches_1_8_67() {
        let json = download_file(RequestId(12), FileId(44), 32);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "downloadFile");
        assert_eq!(v["@extra"], "12");
        assert_eq!(v["file_id"], 44);
        assert_eq!(v["priority"], 32);
        assert_eq!(v["offset"], 0);
        assert_eq!(v["limit"], 0);
        assert_eq!(v["synchronous"], false);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn cancel_download_file_shape_matches_1_8_67() {
        // `cancelDownloadFile file_id:int32 only_if_pending:Bool = Ok;`
        // (schema 1.8.67, line 13991).
        let json = cancel_download_file(RequestId(13), FileId(45), false);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "cancelDownloadFile");
        assert_eq!(v["@extra"], "13");
        assert_eq!(v["file_id"], 45);
        assert_eq!(v["only_if_pending"], false);
        assert!(v.as_object().unwrap().len() == 4);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn get_contacts_shape_matches_1_8_67() {
        // `getContacts = Users;` — no parameters (schema 1.8.67, line 14520).
        let json = get_contacts(RequestId(60));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getContacts");
        assert_eq!(v["@extra"], "60");
        assert_eq!(v.as_object().unwrap().len(), 2);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn add_contact_shape_matches_1_8_67() {
        // `addContact user_id:int53 contact:importedContact
        // share_phone_number:Bool = Ok;` (schema 1.8.67, line 14513) with
        // `importedContact phone_number:string first_name:string
        // last_name:string note:formattedText` (line 7382).
        let json = add_contact(
            RequestId(61),
            31,
            "+15550131",
            "CANARY-first",
            "CANARY-last",
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "addContact");
        assert_eq!(v["@extra"], "61");
        assert_eq!(v["user_id"], 31);
        let contact = &v["contact"];
        assert_eq!(contact["@type"], "importedContact");
        assert_eq!(contact["phone_number"], "+15550131");
        assert_eq!(contact["first_name"], "CANARY-first");
        assert_eq!(contact["last_name"], "CANARY-last");
        assert_eq!(contact["note"]["@type"], "formattedText");
        assert_eq!(contact["note"]["text"], "");
        assert_eq!(v["share_phone_number"], false);
    }

    #[test]
    fn get_supergroup_full_info_shape_matches_1_8_67() {
        // `getSupergroupFullInfo supergroup_id:int53 = SupergroupFullInfo;`
        // (schema 1.8.67, line 11513).
        let json = get_supergroup_full_info(RequestId(62), 77);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getSupergroupFullInfo");
        assert_eq!(v["@extra"], "62");
        assert_eq!(v["supergroup_id"], 77);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn get_chat_statistics_shape_matches_1_8_67() {
        // `getChatStatistics chat_id:int53 is_dark:Bool = ChatStatistics;`
        // (schema 1.8.67, line 15760).
        let json = get_chat_statistics(RequestId(63), 13, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getChatStatistics");
        assert_eq!(v["@extra"], "63");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["is_dark"], true);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn get_chat_invite_links_shape_matches_1_8_67() {
        // `getChatInviteLinks chat_id:int53 creator_user_id:int53 is_revoked:Bool offset_date:int32 offset_invite_link:string limit:int32 = ChatInviteLinks;`
        // (schema 1.8.67, line 14138).
        let json = get_chat_invite_links(
            RequestId(64),
            101,
            202,
            true,
            1_700_000_000,
            "https://t.me/+offset",
            25,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getChatInviteLinks");
        assert_eq!(v["@extra"], "64");
        assert_eq!(v["chat_id"], 101);
        assert_eq!(v["creator_user_id"], 202);
        assert_eq!(v["is_revoked"], true);
        assert_eq!(v["offset_date"], 1_700_000_000);
        assert_eq!(v["offset_invite_link"], "https://t.me/+offset");
        assert_eq!(v["limit"], 25);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn create_chat_invite_link_shape_matches_1_8_67() {
        // `createChatInviteLink chat_id:int53 name:string expiration_date:int32 member_limit:int32 creates_join_request:Bool = ChatInviteLink;`
        // (schema 1.8.67, line 14097).
        let json = create_chat_invite_link(
            RequestId(65),
            303,
            "Moderated access",
            1_800_000_000,
            50,
            true,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "createChatInviteLink");
        assert_eq!(v["@extra"], "65");
        assert_eq!(v["chat_id"], 303);
        assert_eq!(v["name"], "Moderated access");
        assert_eq!(v["expiration_date"], 1_800_000_000);
        assert_eq!(v["member_limit"], 50);
        assert_eq!(v["creates_join_request"], true);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn edit_chat_invite_link_shape_matches_1_8_67() {
        // `editChatInviteLink chat_id:int53 invite_link:string name:string expiration_date:int32 member_limit:int32 creates_join_request:Bool = ChatInviteLink;`
        // (schema 1.8.67, line 14115).
        let json = edit_chat_invite_link(
            RequestId(66),
            404,
            "https://t.me/+existing",
            "Updated access",
            1_900_000_000,
            75,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editChatInviteLink");
        assert_eq!(v["@extra"], "66");
        assert_eq!(v["chat_id"], 404);
        assert_eq!(v["invite_link"], "https://t.me/+existing");
        assert_eq!(v["name"], "Updated access");
        assert_eq!(v["expiration_date"], 1_900_000_000);
        assert_eq!(v["member_limit"], 75);
        assert_eq!(v["creates_join_request"], false);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn revoke_chat_invite_link_shape_matches_1_8_67() {
        // `revokeChatInviteLink chat_id:int53 invite_link:string = ChatInviteLinks;`
        // (schema 1.8.67, line 14152).
        let json = revoke_chat_invite_link(RequestId(67), 505, "https://t.me/+revoked");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "revokeChatInviteLink");
        assert_eq!(v["@extra"], "67");
        assert_eq!(v["chat_id"], 505);
        assert_eq!(v["invite_link"], "https://t.me/+revoked");
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn get_chat_join_requests_shape_matches_1_8_67() {
        // `getChatJoinRequests chat_id:int53 invite_link:string query:string offset_request:chatJoinRequest limit:int32 = ChatJoinRequests;`
        // (schema 1.8.67, line 14174).
        let json =
            get_chat_join_requests(RequestId(68), 606, "https://t.me/+requests", "alice", 30);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getChatJoinRequests");
        assert_eq!(v["@extra"], "68");
        assert_eq!(v["chat_id"], 606);
        assert_eq!(v["invite_link"], "https://t.me/+requests");
        assert_eq!(v["query"], "alice");
        assert_eq!(
            v["offset_request"],
            serde_json::json!({
                "@type": "chatJoinRequest",
            })
        );
        assert_eq!(v["limit"], 30);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn process_chat_join_request_shape_matches_1_8_67() {
        // `processChatJoinRequest chat_id:int53 user_id:int53 approve:Bool = Ok;`
        // (schema 1.8.67, line 14177).
        let json = process_chat_join_request(RequestId(69), 707, 808, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "processChatJoinRequest");
        assert_eq!(v["@extra"], "69");
        assert_eq!(v["chat_id"], 707);
        assert_eq!(v["user_id"], 808);
        assert_eq!(v["approve"], true);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn search_public_chats_shape_matches_1_8_67() {
        // `searchPublicChats query:string type_filter:SearchChatTypeFilter =
        // Chats` (schema 1.8.67, line 11609); type_filter null = all types.
        let json = search_public_chats(RequestId(23), "quill");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchPublicChats");
        assert_eq!(v["@extra"], "23");
        assert_eq!(v["query"], "quill");
        assert!(v["type_filter"].is_null());
        let schema = include_str!("../../schema/td_api.tl");
        let line = schema
            .lines()
            .find(|l| l.starts_with("searchPublicChats "))
            .expect("searchPublicChats in schema");
        assert_eq!(
            line,
            "searchPublicChats query:string type_filter:SearchChatTypeFilter = Chats;"
        );
    }

    #[test]
    fn load_chats_folder_list_shape_matches_1_8_67() {
        // `loadChats chat_list:ChatList limit:int32 = Ok` (line 11595) with
        // `chatListFolder` (line 3524) — Phase 7.1 folder load.
        let json = load_chats_list(
            RequestId(24),
            json!({ "@type": "chatListFolder", "chat_folder_id": 3 }),
            100,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "loadChats");
        assert_eq!(v["chat_list"]["@type"], "chatListFolder");
        assert_eq!(v["chat_list"]["chat_folder_id"], 3);
        assert_eq!(v["limit"], 100);
    }

    #[test]
    fn folder_request_shapes_match_1_8_67() {
        use crate::telegram::envelope::ChatFolderSpec;
        let spec = ChatFolderSpec {
            name: "Work".into(),
            pinned_chat_ids: vec![11],
            included_chat_ids: vec![12],
            excluded_chat_ids: vec![],
            exclude_muted: true,
            exclude_read: false,
            exclude_archived: false,
            include_contacts: true,
            include_non_contacts: false,
            include_bots: false,
            include_groups: true,
            include_channels: false,
        };
        // `createChatFolder folder:chatFolder = ChatFolderInfo` (line 13358).
        let v: serde_json::Value =
            serde_json::from_str(&create_chat_folder(RequestId(30), &spec)).unwrap();
        assert_eq!(v["@type"], "createChatFolder");
        assert_eq!(v["folder"]["@type"], "chatFolder");
        assert_eq!(v["folder"]["name"]["text"]["text"], "Work");
        assert!(v["folder"]["icon"].is_null());
        assert_eq!(v["folder"]["color_id"], -1);
        assert_eq!(v["folder"]["pinned_chat_ids"], json!([11]));
        assert_eq!(v["folder"]["included_chat_ids"], json!([12]));
        assert!(v["folder"]["exclude_muted"].as_bool().unwrap());
        assert!(v["folder"]["include_contacts"].as_bool().unwrap());
        assert!(v["folder"]["include_groups"].as_bool().unwrap());
        // `editChatFolder chat_folder_id:int32 folder:chatFolder =
        // ChatFolderInfo` (line 13361).
        let v: serde_json::Value =
            serde_json::from_str(&edit_chat_folder(RequestId(31), 7, &spec)).unwrap();
        assert_eq!(v["@type"], "editChatFolder");
        assert_eq!(v["chat_folder_id"], 7);
        assert_eq!(v["folder"]["@type"], "chatFolder");
        // `deleteChatFolder chat_folder_id:int32 leave_chat_ids:vector<int53>
        // = Ok` (line 13364).
        let v: serde_json::Value =
            serde_json::from_str(&delete_chat_folder(RequestId(32), 7, &[12])).unwrap();
        assert_eq!(v["@type"], "deleteChatFolder");
        assert_eq!(v["chat_folder_id"], 7);
        assert_eq!(v["leave_chat_ids"], json!([12]));
        // `reorderChatFolders chat_folder_ids:vector<int32>
        // main_chat_list_position:int32 = Ok` (line 13373).
        let v: serde_json::Value =
            serde_json::from_str(&reorder_chat_folders(RequestId(33), &[2, 1])).unwrap();
        assert_eq!(v["@type"], "reorderChatFolders");
        assert_eq!(v["chat_folder_ids"], json!([2, 1]));
        assert_eq!(v["main_chat_list_position"], 0);
        // `toggleChatFolderTags are_tags_enabled:Bool = Ok` (line 13376).
        let v: serde_json::Value =
            serde_json::from_str(&toggle_chat_folder_tags(RequestId(34), true)).unwrap();
        assert_eq!(v["@type"], "toggleChatFolderTags");
        assert!(v["are_tags_enabled"].as_bool().unwrap());
        // `getChatFolder chat_folder_id:int32 = ChatFolder` (line 13355).
        let v: serde_json::Value =
            serde_json::from_str(&get_chat_folder(RequestId(35), 7)).unwrap();
        assert_eq!(v["@type"], "getChatFolder");
        assert_eq!(v["chat_folder_id"], 7);
        // `getChatListsToAddChat chat_id:int53 = ChatLists` (line 13347).
        let v: serde_json::Value =
            serde_json::from_str(&get_chat_lists_to_add_chat(RequestId(36), ChatId(12))).unwrap();
        assert_eq!(v["@type"], "getChatListsToAddChat");
        assert_eq!(v["chat_id"], 12);
        // `addChatToList` with `chatListFolder` (line 13352 + 3524).
        let v: serde_json::Value = serde_json::from_str(&add_chat_to_list_value(
            RequestId(37),
            ChatId(12),
            json!({ "@type": "chatListFolder", "chat_folder_id": 7 }),
        ))
        .unwrap();
        assert_eq!(v["@type"], "addChatToList");
        assert_eq!(v["chat_id"], 12);
        assert_eq!(v["chat_list"]["@type"], "chatListFolder");
        assert_eq!(v["chat_list"]["chat_folder_id"], 7);
    }

    #[test]
    fn search_chats_shape_matches_1_8_67() {
        let json = search_chats(RequestId(21), "alice", 20);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchChats");
        assert_eq!(v["@extra"], "21");
        assert_eq!(v["query"], "alice");
        assert_eq!(v["type_filter"], Value::Null);
        assert_eq!(v["limit"], 20);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn search_messages_shape_matches_1_8_67() {
        let json = search_messages(RequestId(22), "hello", 20);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchMessages");
        assert_eq!(v["@extra"], "22");
        assert_eq!(v["chat_list"], Value::Null);
        assert_eq!(v["query"], "hello");
        assert_eq!(v["offset"], "");
        assert_eq!(v["limit"], 20);
        assert_eq!(v["filter"], Value::Null);
        assert_eq!(v["chat_type_filter"], Value::Null);
        assert_eq!(v["min_date"], 0);
        assert_eq!(v["max_date"], 0);
        assert!(!json.contains("chatListMain"));
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn search_recently_found_and_add_shapes_match_1_8_67() {
        let recents = search_recently_found_chats(RequestId(23), "", 50);
        let v: serde_json::Value = serde_json::from_str(&recents).unwrap();
        assert_eq!(v["@type"], "searchRecentlyFoundChats");
        assert_eq!(v["@extra"], "23");
        assert_eq!(v["query"], "");
        assert_eq!(v["type_filter"], Value::Null);
        assert_eq!(v["limit"], 50);
        let add = add_recently_found_chat(RequestId(24), ChatId(11));
        let v: serde_json::Value = serde_json::from_str(&add).unwrap();
        assert_eq!(v["@type"], "addRecentlyFoundChat");
        assert_eq!(v["@extra"], "24");
        assert_eq!(v["chat_id"], 11);
        assert!(!recents.contains("CANARY"));
        assert!(!add.contains("CANARY"));
    }

    #[test]
    fn get_callback_query_answer_shape_matches_1_8_67() {
        // `getCallbackQueryAnswer chat_id:int53 message_id:int53
        // payload:CallbackQueryPayload = CallbackQueryAnswer` (schema line
        // 13138); `callbackQueryPayloadData data:bytes` (line 7737) with
        // base64 `bytes` in JSON.
        let json = get_callback_query_answer(RequestId(51), ChatId(21), MessageId(301), &[1, 2, 3]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getCallbackQueryAnswer");
        assert_eq!(v["@extra"], "51");
        assert_eq!(v["chat_id"], 21);
        assert_eq!(v["message_id"], 301);
        assert_eq!(v["payload"]["@type"], "callbackQueryPayloadData");
        assert_eq!(v["payload"]["data"], "AQID");
        assert!(!json.contains("answerCallbackQuery"));
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn edit_message_text_shape_matches_1_8_67() {
        let json = edit_message_text(
            RequestId(31),
            ChatId(11),
            MessageId(102),
            "edited body",
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editMessageText");
        assert_eq!(v["@extra"], "31");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 102);
        assert_eq!(v["reply_markup"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageText");
        assert_eq!(v["input_message_content"]["text"]["@type"], "formattedText");
        assert_eq!(v["input_message_content"]["text"]["text"], "edited body");
        assert_eq!(
            v["input_message_content"]["text"]["entities"],
            serde_json::json!([])
        );
        assert_eq!(
            v["input_message_content"]["link_preview_options"],
            Value::Null
        );
        assert_eq!(v["input_message_content"]["clear_draft"], false);
        assert!(!json.contains("CANARY"));
        assert!(!json.contains("message_thread_id"));
    }

    #[test]
    fn set_chat_draft_message_matches_1_8_67() {
        let save = set_chat_draft_message(
            RequestId(41),
            ChatId(11),
            Some("meet at 6"),
            Some(&SendReply::plain(MessageId(101))),
        );
        let v: serde_json::Value = serde_json::from_str(&save).unwrap();
        assert_eq!(v["@type"], "setChatDraftMessage");
        assert_eq!(v["@extra"], "41");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["draft_message"]["@type"], "draftMessage");
        assert_eq!(v["draft_message"]["date"], 0);
        assert_eq!(v["draft_message"]["effect_id"], "0");
        assert_eq!(v["draft_message"]["suggested_post_info"], Value::Null);
        assert_eq!(
            v["draft_message"]["content"]["@type"],
            "draftMessageContentText"
        );
        assert_eq!(v["draft_message"]["content"]["text"]["text"], "meet at 6");
        assert_eq!(
            v["draft_message"]["content"]["link_preview_options"],
            Value::Null
        );
        assert_eq!(
            v["draft_message"]["reply_to"]["@type"],
            "inputMessageReplyToMessage"
        );
        assert_eq!(v["draft_message"]["reply_to"]["message_id"], 101);
        assert_eq!(
            v["draft_message"]["reply_to"]["quote"],
            Value::Null,
            "whole-message draft reply carries no quote"
        );
        let quoted = set_chat_draft_message(
            RequestId(43),
            ChatId(11),
            Some("agree"),
            Some(&SendReply {
                message_id: MessageId(101),
                quote: Some(("meet at".to_string(), 0)),
            }),
        );
        let v: serde_json::Value = serde_json::from_str(&quoted).unwrap();
        assert_eq!(
            v["draft_message"]["reply_to"]["quote"]["@type"],
            "inputTextQuote"
        );
        assert_eq!(
            v["draft_message"]["reply_to"]["quote"]["text"]["text"],
            "meet at"
        );
        assert_eq!(v["draft_message"]["reply_to"]["quote"]["position"], 0);
        let clear = set_chat_draft_message(RequestId(42), ChatId(11), None, None);
        let v: serde_json::Value = serde_json::from_str(&clear).unwrap();
        assert_eq!(v["draft_message"], Value::Null);
        assert!(!save.contains("CANARY"));
    }

    #[test]
    fn edit_message_caption_shape_matches_1_8_67() {
        let json = edit_message_caption(
            RequestId(32),
            ChatId(11),
            MessageId(60),
            "new cap",
            false,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editMessageCaption");
        assert_eq!(v["@extra"], "32");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 60);
        assert_eq!(v["reply_markup"], Value::Null);
        assert_eq!(v["caption"]["@type"], "formattedText");
        assert_eq!(v["caption"]["text"], "new cap");
        assert_eq!(v["caption"]["entities"], serde_json::json!([]));
        assert_eq!(v["show_caption_above_media"], false);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn delete_messages_shape_matches_1_8_67() {
        let json = delete_messages(RequestId(33), ChatId(11), &[MessageId(102)], true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteMessages");
        assert_eq!(v["@extra"], "33");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_ids"], serde_json::json!([102]));
        assert_eq!(v["revoke"], true);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn forward_messages_shape_matches_1_8_67() {
        let json = forward_messages(
            RequestId(34),
            ChatId(12),
            ChatId(11),
            &[MessageId(101), MessageId(102)],
            false,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "forwardMessages");
        assert_eq!(v["@extra"], "34");
        assert_eq!(v["chat_id"], 12);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["from_chat_id"], 11);
        assert_eq!(v["message_ids"], serde_json::json!([101, 102]));
        assert_eq!(v["options"], Value::Null);
        assert_eq!(v["send_copy"], false);
        assert_eq!(v["remove_caption"], false);
        assert!(!json.contains("CANARY"));
        assert!(!json.contains("message_thread_id"));
        assert!(!json.contains("inputMessageForwarded"));
    }

    #[test]
    fn add_and_remove_message_reaction_shapes_match_1_8_67() {
        let add = add_message_reaction(RequestId(35), ChatId(11), MessageId(101), "❤", false, true);
        let v: serde_json::Value = serde_json::from_str(&add).unwrap();
        assert_eq!(v["@type"], "addMessageReaction");
        assert_eq!(v["@extra"], "35");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 101);
        assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
        assert_eq!(v["reaction_type"]["emoji"], "❤");
        assert_eq!(v["is_big"], false);
        assert_eq!(v["update_recent_reactions"], true);
        assert!(!add.contains("CANARY"));
        assert!(!add.contains("setMessageReactions"));
        assert!(!add.contains("reactionTypeCustomEmoji"));
        assert!(!add.contains("reactionTypePaid"));

        let remove = remove_message_reaction(RequestId(36), ChatId(11), MessageId(101), "❤");
        let v: serde_json::Value = serde_json::from_str(&remove).unwrap();
        assert_eq!(v["@type"], "removeMessageReaction");
        assert_eq!(v["@extra"], "36");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 101);
        assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
        assert_eq!(v["reaction_type"]["emoji"], "❤");
        assert!(!remove.contains("is_big"));
        assert!(!remove.contains("update_recent_reactions"));
        assert!(!remove.contains("CANARY"));
    }

    #[test]
    fn set_chat_slow_mode_delay_shape_matches_1_8_67() {
        // `setChatSlowModeDelay chat_id:int53 slow_mode_delay:int32 = Ok;`
        // (schema 1.8.67, line 13551).
        let json = set_chat_slow_mode_delay(RequestId(44), ChatId(11), 30);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setChatSlowModeDelay");
        assert_eq!(v["@extra"], "44");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["slow_mode_delay"], 30);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn pin_and_unpin_chat_message_shape_matches_1_8_67() {
        let pin = pin_chat_message(RequestId(40), ChatId(11), MessageId(101), false, false);
        let v: serde_json::Value = serde_json::from_str(&pin).unwrap();
        assert_eq!(v["@type"], "pinChatMessage");
        assert_eq!(v["@extra"], "40");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 101);
        assert_eq!(v["disable_notification"], false);
        assert_eq!(v["only_for_self"], false);
        assert!(!pin.contains("CANARY"));
        assert!(!pin.contains("unpinAllChatMessages"));

        let unpin = unpin_chat_message(RequestId(41), ChatId(11), MessageId(101));
        let v: serde_json::Value = serde_json::from_str(&unpin).unwrap();
        assert_eq!(v["@type"], "unpinChatMessage");
        assert_eq!(v["@extra"], "41");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 101);
        assert!(!unpin.contains("disable_notification"));
        assert!(!unpin.contains("only_for_self"));
        assert!(!unpin.contains("CANARY"));
    }

    #[test]
    fn search_chat_messages_shape_matches_1_8_67() {
        let json = search_chat_messages(
            RequestId(25),
            ChatId(11),
            &TopicId::None,
            "hello",
            MessageId(0),
            0,
            50,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchChatMessages");
        assert_eq!(v["@extra"], "25");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["query"], "hello");
        assert_eq!(v["sender_id"], Value::Null);
        assert_eq!(v["from_message_id"], 0);
        assert_eq!(v["offset"], 0);
        assert_eq!(v["limit"], 50);
        assert_eq!(v["filter"], Value::Null);
        assert!(!json.contains("CANARY"));
        assert!(!json.contains("message_thread_id"));
    }

    #[test]
    fn search_chat_messages_forum_topic_uses_message_topic_forum() {
        let json = search_chat_messages(
            RequestId(26),
            ChatId(11),
            &TopicId::Forum { forum_topic_id: 7 },
            "",
            MessageId(0),
            0,
            50,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchChatMessages");
        assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
        assert_eq!(v["topic_id"]["forum_topic_id"], 7);
        assert_eq!(v["query"], "");
    }

    #[test]
    fn topic_id_json_variants() {
        assert_eq!(topic_id_json(&TopicId::None), Value::Null);
        let dm = topic_id_json(&TopicId::DirectMessages {
            direct_messages_chat_topic_id: 42,
        });
        assert_eq!(dm["@type"], "messageTopicDirectMessages");
        assert_eq!(dm["direct_messages_chat_topic_id"], 42);
        let sm = topic_id_json(&TopicId::SavedMessages {
            saved_messages_topic_id: 9,
        });
        assert_eq!(sm["@type"], "messageTopicSavedMessages");
        assert_eq!(sm["saved_messages_topic_id"], 9);
    }

    #[test]
    fn get_forum_topics_shape_matches_1_8_67() {
        let json = get_forum_topics(RequestId(31), ChatId(12), "", 0, MessageId(0), 0, 100);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getForumTopics");
        assert_eq!(v["@extra"], "31");
        assert_eq!(v["chat_id"], 12);
        assert_eq!(v["query"], "");
        assert_eq!(v["offset_date"], 0);
        assert_eq!(v["offset_message_id"], 0);
        assert_eq!(v["offset_forum_topic_id"], 0);
        assert_eq!(v["limit"], 100);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn get_supergroup_shape_matches_1_8_67() {
        let json = get_supergroup(RequestId(32), 77);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getSupergroup");
        assert_eq!(v["@extra"], "32");
        assert_eq!(v["supergroup_id"], 77);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn open_and_close_chat_are_distinct_from_client_close() {
        let open = open_chat(RequestId(8), ChatId(3));
        let close = close_chat(RequestId(9), ChatId(3));
        let client_close = close_request(RequestId(10));
        assert!(open.contains("\"@type\":\"openChat\""));
        assert!(close.contains("\"@type\":\"closeChat\""));
        assert!(client_close.contains("\"@type\":\"close\""));
        assert!(!client_close.contains("closeChat"));
        assert_eq!(serde_json::from_str::<Value>(&close).unwrap()["chat_id"], 3);
    }

    #[test]
    fn sponsored_message_requests_match_1_8_67() {
        let fetch = get_chat_sponsored_messages(RequestId(50), ChatId(13));
        let v: serde_json::Value = serde_json::from_str(&fetch).unwrap();
        assert_eq!(v["@type"], "getChatSponsoredMessages");
        assert_eq!(v["@extra"], "50");
        assert_eq!(v["chat_id"], 13);

        let report = report_chat_sponsored_message(RequestId(51), ChatId(13), 777, "");
        let v: serde_json::Value = serde_json::from_str(&report).unwrap();
        assert_eq!(v["@type"], "reportChatSponsoredMessage");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["message_id"], 777);
        assert_eq!(v["option_id"], "");

        let with_option = report_chat_sponsored_message(RequestId(52), ChatId(13), 777, "b3B0aW9u");
        let v: serde_json::Value = serde_json::from_str(&with_option).unwrap();
        assert_eq!(v["option_id"], "b3B0aW9u");

        let view = view_sponsored_chat(RequestId(53), 4242);
        let v: serde_json::Value = serde_json::from_str(&view).unwrap();
        assert_eq!(v["@type"], "viewSponsoredChat");
        assert_eq!(v["sponsored_chat_unique_id"], 4242);

        let click = click_chat_sponsored_message(RequestId(54), ChatId(13), 9001, false, false);
        let v: serde_json::Value = serde_json::from_str(&click).unwrap();
        assert_eq!(v["@type"], "clickChatSponsoredMessage");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["message_id"], 9001);
        assert_eq!(v["is_media_click"], false);
        assert_eq!(v["from_fullscreen"], false);

        let media_click =
            click_chat_sponsored_message(RequestId(55), ChatId(13), 9002, true, false);
        let v: serde_json::Value = serde_json::from_str(&media_click).unwrap();
        assert_eq!(v["is_media_click"], true);
    }

    #[test]
    fn send_chat_action_typing_and_cancel_match_1_8_67() {
        let typing = send_chat_action(RequestId(42), ChatId(11), true);
        let v: serde_json::Value = serde_json::from_str(&typing).unwrap();
        assert_eq!(v["@type"], "sendChatAction");
        assert_eq!(v["@extra"], "42");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["business_connection_id"], "");
        assert_eq!(v["action"]["@type"], "chatActionTyping");
        assert!(!typing.contains("CANARY"));
        assert!(!typing.contains("message_thread_id"));

        let cancel = send_chat_action(RequestId(43), ChatId(11), false);
        let v: serde_json::Value = serde_json::from_str(&cancel).unwrap();
        assert_eq!(v["action"]["@type"], "chatActionCancel");
        assert!(!cancel.contains("chatActionTyping"));

        let recording =
            send_chat_action_kind(RequestId(44), ChatId(11), "chatActionRecordingVoiceNote");
        let v: serde_json::Value = serde_json::from_str(&recording).unwrap();
        assert_eq!(v["action"]["@type"], "chatActionRecordingVoiceNote");
    }

    #[test]
    fn recognize_speech_shape_matches_1_8_67() {
        // MED2: what the request is ultimately validating — the
        // `recognizeSpeech` constructor with chat and message ids.
        let json = recognize_speech(RequestId(21), ChatId(7), MessageId(9));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "recognizeSpeech");
        assert_eq!(v["@extra"], "21");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 9);
    }

    #[test]
    fn send_voice_note_shape_matches_1_8_67() {
        let json = send_voice_note(
            RequestId(15),
            ChatId(7),
            VoiceNoteSend {
                path: "/tmp/picked.ogg",
                duration: 3,
                waveform_b64: "BASE64WAVE",
                caption: "",
                reply_to: Some(SendReply::plain(MessageId(9))),
                topic_id: None,
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "15");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageVoiceNote");
        assert_eq!(
            v["input_message_content"]["voice_note"]["@type"],
            "inputVoiceNote"
        );
        assert_eq!(
            v["input_message_content"]["voice_note"]["voice_note"]["@type"],
            "inputFileLocal"
        );
        assert_eq!(
            v["input_message_content"]["voice_note"]["voice_note"]["path"],
            "/tmp/picked.ogg"
        );
        assert_eq!(v["input_message_content"]["voice_note"]["duration"], 3);
        assert_eq!(
            v["input_message_content"]["voice_note"]["waveform"],
            "BASE64WAVE"
        );
        assert_eq!(v["input_message_content"]["caption"], Value::Null);
        assert_eq!(
            v["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 9);
        assert!(!json.contains("inputMessageVideoNote"));
        assert!(!json.contains("CANARY"));
    }
}

#[cfg(test)]
mod channel_requests_tests {
    use super::*;
    use crate::story_composer::StoryPrivacy;

    #[test]
    fn channel_request_shapes_match_1_8_67() {
        let me = get_me(RequestId(60));
        let v: serde_json::Value = serde_json::from_str(&me).unwrap();
        assert_eq!(v["@type"], "getMe");
        assert_eq!(v["@extra"], "60");

        let member = get_chat_member(RequestId(61), ChatId(13), 777);
        let v: serde_json::Value = serde_json::from_str(&member).unwrap();
        assert_eq!(v["@type"], "getChatMember");
        assert_eq!(v["@extra"], "61");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["member_id"]["@type"], "messageSenderUser");
        assert_eq!(v["member_id"]["user_id"], 777);

        let join = join_chat(RequestId(62), ChatId(13));
        let v: serde_json::Value = serde_json::from_str(&join).unwrap();
        assert_eq!(v["@type"], "joinChat");
        assert_eq!(v["@extra"], "62");
        assert_eq!(v["chat_id"], 13);

        let leave = leave_chat(RequestId(63), ChatId(13));
        let v: serde_json::Value = serde_json::from_str(&leave).unwrap();
        assert_eq!(v["@type"], "leaveChat");
        assert_eq!(v["@extra"], "63");
        assert_eq!(v["chat_id"], 13);
    }

    #[test]
    fn story_request_shapes_match_1_8_67() {
        // Phase 9.2 story reactions / picker / delete / reply builders.
        let set = set_story_reaction(RequestId(70), ChatId(11), 7, Some("❤"));
        let v: serde_json::Value = serde_json::from_str(&set).unwrap();
        // `setStoryReaction story_poster_chat_id:int53 story_id:int32
        // reaction_type:ReactionType update_recent_reactions:Bool = Ok`
        // (schema 1.8.67 line 13809).
        assert_eq!(v["@type"], "setStoryReaction");
        assert_eq!(v["@extra"], "70");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 7);
        assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
        assert_eq!(v["reaction_type"]["emoji"], "❤");
        assert_eq!(v["update_recent_reactions"], true);

        // Removing sends `reaction_type: null` (schema comment, line 13809).
        let remove = set_story_reaction(RequestId(71), ChatId(11), 7, None);
        let v: serde_json::Value = serde_json::from_str(&remove).unwrap();
        assert_eq!(v["@type"], "setStoryReaction");
        assert_eq!(v["reaction_type"], serde_json::Value::Null);

        // `getStoryAvailableReactions row_size:int32 = AvailableReactions`
        // (schema 1.8.67 line 13802); row_size 10 is inside 5–25.
        let avail = get_story_available_reactions(RequestId(72), 10);
        let v: serde_json::Value = serde_json::from_str(&avail).unwrap();
        assert_eq!(v["@type"], "getStoryAvailableReactions");
        assert_eq!(v["row_size"], 10);

        // `deleteStory story_poster_chat_id:int53 story_id:int32 = Ok`
        // (schema 1.8.67 line 13754).
        let delete = delete_story(RequestId(73), ChatId(11), 7);
        let v: serde_json::Value = serde_json::from_str(&delete).unwrap();
        assert_eq!(v["@type"], "deleteStory");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 7);

        // Story reply: `sendMessage` with `inputMessageReplyToStory
        // story_poster_chat_id:int53 story_id:int32 = InputMessageReplyTo`
        // (schema 1.8.67 line 3099).
        let reply = send_text_story_reply(RequestId(74), ChatId(11), ChatId(11), 7, "Nice!");
        let v: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
        assert_eq!(v["reply_to"]["story_poster_chat_id"], 11);
        assert_eq!(v["reply_to"]["story_id"], 7);
        assert_eq!(v["input_message_content"]["text"]["text"], "Nice!");
    }

    #[test]
    fn set_poll_answer_shape_matches_1_8_67() {
        // `setPollAnswer chat_id:int53 message_id:int53
        // option_ids:vector<int32> = Ok` (schema 1.8.67 line 12932).
        // `option_ids` are 0-based option *positions*, not `pollOption.id`
        // strings (those are `updatePollAnswer.option_ids`, line 11186).
        let json = set_poll_answer(RequestId(9), ChatId(1), MessageId(42), &[0, 2]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setPollAnswer");
        assert_eq!(v["@extra"], "9");
        assert_eq!(v["chat_id"], 1);
        assert_eq!(v["message_id"], 42);
        assert_eq!(v["option_ids"], serde_json::json!([0, 2]));
    }

    #[test]
    fn set_poll_answer_retract_is_empty_option_ids() {
        // Retracting a vote sends an empty `option_ids` vector (still a
        // `setPollAnswer`, not a different constructor).
        let json = set_poll_answer(RequestId(9), ChatId(1), MessageId(42), &[]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setPollAnswer");
        assert_eq!(v["option_ids"], serde_json::json!([]));
    }

    #[test]
    fn send_poll_shape_matches_1_8_67() {
        // `inputMessagePoll` (schema 1.8.67 line 6193) wrapped in
        // `sendMessage`; options are `inputPollOption` (line 462) and the
        // type is `inputPollTypeRegular` (line 481). Quiz types are not
        // sent by this slice.
        let options = ["Sushi place", "Pizza"];
        let json = send_poll(
            RequestId(11),
            ChatId(7),
            PollSend {
                question: "Where should we eat lunch?",
                options: &options,
                is_anonymous: true,
                allows_multiple_answers: false,
                reply_to: Some(SendReply::plain(MessageId(101))),
                topic_id: None,
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "11");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 101);
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessagePoll");
        assert_eq!(content["question"]["text"], "Where should we eat lunch?");
        assert_eq!(content["question"]["entities"], serde_json::json!([]));
        assert_eq!(content["options"][0]["@type"], "inputPollOption");
        assert_eq!(content["options"][0]["text"]["text"], "Sushi place");
        assert!(content["options"][0]["media"].is_null());
        assert_eq!(content["options"][1]["text"]["text"], "Pizza");
        assert!(content["description"].is_null());
        assert!(content["media"].is_null());
        assert_eq!(content["is_anonymous"], true);
        assert_eq!(content["allows_multiple_answers"], false);
        assert_eq!(content["allows_revoting"], true);
        assert_eq!(content["type"]["@type"], "inputPollTypeRegular");
        assert_eq!(content["type"]["allow_adding_options"], false);
        assert_eq!(content["open_period"], 0);
        assert_eq!(content["close_date"], 0);
        assert_eq!(content["is_closed"], false);
    }

    #[test]
    fn story_requests_match_1_8_67() {
        // `loadActiveStories story_list:StoryList = Ok` (schema line
        // 13762); `getChatActiveStories chat_id:int53 = ChatActiveStories`
        // (line 13768); `getStory story_poster_chat_id:int53 story_id:int32
        // only_local:Bool = Story` (line 13695); `openStory` / `closeStory`
        // `story_poster_chat_id:int53 story_id:int32 = Ok` (lines 13794,
        // 13799).
        let v: serde_json::Value =
            serde_json::from_str(&load_active_stories(RequestId(1))).unwrap();
        assert_eq!(v["@type"], "loadActiveStories");
        assert_eq!(v["story_list"]["@type"], "storyListMain");

        let v: serde_json::Value =
            serde_json::from_str(&get_chat_active_stories(RequestId(2), ChatId(11))).unwrap();
        assert_eq!(v["@type"], "getChatActiveStories");
        assert_eq!(v["chat_id"], 11);

        let v: serde_json::Value =
            serde_json::from_str(&get_story(RequestId(3), ChatId(11), 5)).unwrap();
        assert_eq!(v["@type"], "getStory");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 5);
        assert_eq!(v["only_local"], false);

        let v: serde_json::Value =
            serde_json::from_str(&open_story(RequestId(4), ChatId(11), 5)).unwrap();
        assert_eq!(v["@type"], "openStory");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 5);

        let v: serde_json::Value =
            serde_json::from_str(&close_story(RequestId(5), ChatId(11), 5)).unwrap();
        assert_eq!(v["@type"], "closeStory");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 5);
    }

    /// Phase C1: call request shapes — verified against the pinned
    /// schema (1.8.67) constructors, never invented.
    #[test]
    fn call_request_shapes() {
        // The advertised protocol is signaling-only: no media
        // capability claimed (min/max layer pinned by the schema).
        let p = call_protocol();
        assert_eq!(p["@type"], "callProtocol");
        assert_eq!(p["udp_p2p"], false);
        assert_eq!(p["udp_reflector"], false);
        assert_eq!(p["min_layer"], 65);
        assert_eq!(p["max_layer"], 92);
        assert_eq!(p["library_versions"].as_array().unwrap().len(), 0);

        let v: serde_json::Value =
            serde_json::from_str(&create_call(RequestId(1), 41, false)).unwrap();
        assert_eq!(v["@type"], "createCall");
        assert_eq!(v["user_id"], 41);
        assert_eq!(v["is_video"], false);
        assert_eq!(v["protocol"]["@type"], "callProtocol");

        // Phase C1b: a video call sends `is_video: true` (signaling
        // only — no transport yet).
        let v: serde_json::Value =
            serde_json::from_str(&create_call(RequestId(5), 41, true)).unwrap();
        assert_eq!(v["@type"], "createCall");
        assert_eq!(v["user_id"], 41);
        assert_eq!(v["is_video"], true);

        let v: serde_json::Value = serde_json::from_str(&accept_call(RequestId(2), 77)).unwrap();
        assert_eq!(v["@type"], "acceptCall");
        assert_eq!(v["call_id"], 77);
        assert_eq!(v["protocol"]["@type"], "callProtocol");

        let v: serde_json::Value =
            serde_json::from_str(&discard_call(RequestId(3), 77, false, 42, false)).unwrap();
        assert_eq!(v["@type"], "discardCall");
        assert_eq!(v["call_id"], 77);
        assert_eq!(v["is_disconnected"], false);
        assert_eq!(v["invite_link"], "");
        assert_eq!(v["duration"], 42);
        assert_eq!(v["is_video"], false);
        assert_eq!(v["connection_id"], 0);

        // Phase C1b: discarding a video call reports `is_video: true`
        // (schema 1.8.67, :14227).
        let v: serde_json::Value =
            serde_json::from_str(&discard_call(RequestId(6), 78, false, 7, true)).unwrap();
        assert_eq!(v["@type"], "discardCall");
        assert_eq!(v["call_id"], 78);
        assert_eq!(v["duration"], 7);
        assert_eq!(v["is_video"], true);

        let v: serde_json::Value =
            serde_json::from_str(&send_call_rating(RequestId(4), 77, 5)).unwrap();
        assert_eq!(v["@type"], "sendCallRating");
        assert_eq!(v["call_id"]["@type"], "inputCallDiscarded");
        assert_eq!(v["call_id"]["call_id"], 77);
        assert_eq!(v["rating"], 5);
        assert_eq!(v["comment"], "");
        assert_eq!(v["problems"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn call_history_and_settings_shapes_match_1_8_67() {
        // Phase C2i: `searchCallMessages offset:string limit:int32
        // only_missed:Bool = FoundMessages` (schema 1.8.67 line 11903).
        let v: serde_json::Value =
            serde_json::from_str(&search_call_messages(RequestId(10), "", 40)).unwrap();
        assert_eq!(v["@type"], "searchCallMessages");
        assert_eq!(v["offset"], "");
        assert_eq!(v["limit"], 40);
        assert_eq!(v["only_missed"], false);

        // Phase C2i: full rating detail — `sendCallRating
        // call_id:InputCall rating:int32 comment:string
        // problems:vector<CallProblem> = Ok` (schema 1.8.67 line 14234).
        let v: serde_json::Value = serde_json::from_str(&send_call_rating_detail(
            RequestId(11),
            77,
            2,
            "robotic voice",
            &["callProblemEcho", "callProblemDistortedSpeech"],
        ))
        .unwrap();
        assert_eq!(v["@type"], "sendCallRating");
        assert_eq!(v["call_id"]["@type"], "inputCallDiscarded");
        assert_eq!(v["rating"], 2);
        assert_eq!(v["comment"], "robotic voice");
        let problems = v["problems"].as_array().unwrap();
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0]["@type"], "callProblemEcho");
        assert_eq!(problems[1]["@type"], "callProblemDistortedSpeech");

        // Phase C2i: `sendCallLog call_id:InputCall log_file:InputFile
        // = Ok` (schema 1.8.67 line 14240); only inputFileLocal /
        // inputFileGenerated are supported.
        let v: serde_json::Value =
            serde_json::from_str(&send_call_log(RequestId(12), 77, "/tmp/quill-call-77.log"))
                .unwrap();
        assert_eq!(v["@type"], "sendCallLog");
        assert_eq!(v["call_id"]["@type"], "inputCallDiscarded");
        assert_eq!(v["log_file"]["@type"], "inputFileLocal");
        assert_eq!(v["log_file"]["path"], "/tmp/quill-call-77.log");

        // Phase C2i: `getUserPrivacySettingRules
        // setting:UserPrivacySetting = UserPrivacySettingRules` (schema
        // 1.8.67 line 15620); `userPrivacySettingAllowCalls` (:9006).
        let v: serde_json::Value = serde_json::from_str(&get_user_privacy_setting_rules(
            RequestId(13),
            CallPrivacySetting::AllowCalls,
        ))
        .unwrap();
        assert_eq!(v["@type"], "getUserPrivacySettingRules");
        assert_eq!(v["setting"]["@type"], "userPrivacySettingAllowCalls");
        let v: serde_json::Value = serde_json::from_str(&get_user_privacy_setting_rules(
            RequestId(14),
            CallPrivacySetting::PeerToPeer,
        ))
        .unwrap();
        assert_eq!(
            v["setting"]["@type"],
            "userPrivacySettingAllowPeerToPeerCalls"
        );

        // Phase C2i: `setUserPrivacySettingRules
        // setting:UserPrivacySetting rules:userPrivacySettingRules = Ok`
        // (schema 1.8.67 line 15617); Nobody =
        // `[userPrivacySettingRuleRestrictAll]` (:8961).
        let v: serde_json::Value = serde_json::from_str(&set_user_privacy_setting_rules(
            RequestId(15),
            CallPrivacySetting::AllowCalls,
            PrivacyWho::Nobody,
        ))
        .unwrap();
        assert_eq!(v["@type"], "setUserPrivacySettingRules");
        assert_eq!(v["setting"]["@type"], "userPrivacySettingAllowCalls");
        assert_eq!(v["rules"]["@type"], "userPrivacySettingRules");
        let rules = v["rules"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["@type"], "userPrivacySettingRuleRestrictAll");

        // Everybody / Contacts map to AllowAll / AllowContacts; server
        // rule names map back, mixed/custom rules map to None.
        assert_eq!(
            PrivacyWho::Everybody.rules()[0]["@type"],
            "userPrivacySettingRuleAllowAll"
        );
        assert_eq!(
            PrivacyWho::Contacts.rules()[0]["@type"],
            "userPrivacySettingRuleAllowContacts"
        );
        assert_eq!(
            PrivacyWho::from_rule_names(&["userPrivacySettingRuleAllowContacts".to_string()]),
            Some(PrivacyWho::Contacts)
        );
        assert_eq!(
            PrivacyWho::from_rule_names(&[
                "userPrivacySettingRuleAllowUsers".to_string(),
                "userPrivacySettingRuleRestrictAll".to_string()
            ]),
            Some(PrivacyWho::Nobody)
        );
        assert_eq!(PrivacyWho::from_rule_names(&[]), None);
    }

    #[test]
    fn send_call_signaling_data_shape() {
        let v: serde_json::Value =
            serde_json::from_str(&send_call_signaling_data(RequestId(7), 77, b"signal-bytes"))
                .unwrap();
        assert_eq!(v["@type"], "sendCallSignalingData");
        assert_eq!(v["call_id"], 77);
        assert_eq!(v["data"], "c2lnbmFsLWJ5dGVz");
    }

    #[test]
    fn send_call_debug_information_shape() {
        let v: serde_json::Value = serde_json::from_str(&send_call_debug_information(
            RequestId(8),
            77,
            r#"{"transport":"failed"}"#,
        ))
        .unwrap();
        assert_eq!(v["@type"], "sendCallDebugInformation");
        assert_eq!(v["call_id"]["@type"], "inputCallDiscarded");
        assert_eq!(v["call_id"]["call_id"], 77);
        assert_eq!(v["debug_information"], r#"{"transport":"failed"}"#);
    }

    /// Phase C3a: group-call request shapes — verified against the
    /// pinned schema (1.8.67) constructors, never invented.
    #[test]
    fn group_call_request_shapes() {
        // The honest signaling-only join params: no device, no payload.
        let p = GroupCallJoinParams::honest_no_device();
        let pv = p.to_value();
        assert_eq!(pv["@type"], "groupCallJoinParameters");
        assert_eq!(pv["audio_source_id"], 0);
        assert_eq!(pv["payload"], "");
        assert_eq!(pv["is_muted"], false);
        assert_eq!(pv["is_my_video_enabled"], false);

        assert_eq!(
            MessageSenderRef::User(41).to_value()["@type"],
            "messageSenderUser"
        );
        assert_eq!(MessageSenderRef::User(41).to_value()["user_id"], 41);
        assert_eq!(
            MessageSenderRef::Chat(100).to_value()["@type"],
            "messageSenderChat"
        );
        assert_eq!(MessageSenderRef::Chat(100).to_value()["chat_id"], 100);

        assert_eq!(
            InputGroupCallRef::Link("https://t.me/abc".to_string()).to_value()["@type"],
            "inputGroupCallLink"
        );
        let mv = InputGroupCallRef::Message {
            chat_id: 100,
            message_id: 7,
        }
        .to_value();
        assert_eq!(mv["@type"], "inputGroupCallMessage");
        assert_eq!(mv["chat_id"], 100);
        assert_eq!(mv["message_id"], 7);

        let v: serde_json::Value =
            serde_json::from_str(&create_video_chat(RequestId(1), 100, "Standup", 0, false))
                .unwrap();
        assert_eq!(v["@type"], "createVideoChat");
        assert_eq!(v["chat_id"], 100);
        assert_eq!(v["title"], "Standup");
        assert_eq!(v["start_date"], 0);
        assert_eq!(v["is_rtmp_stream"], false);

        // `None` → null join_parameters: create the link only, don't join.
        let v: serde_json::Value =
            serde_json::from_str(&create_group_call(RequestId(2), None)).unwrap();
        assert_eq!(v["@type"], "createGroupCall");
        assert!(v["join_parameters"].is_null());
        let v: serde_json::Value =
            serde_json::from_str(&create_group_call(RequestId(3), Some(&p))).unwrap();
        assert_eq!(v["join_parameters"]["@type"], "groupCallJoinParameters");

        // `None` participant → null: join as self.
        let v: serde_json::Value =
            serde_json::from_str(&join_video_chat(RequestId(4), 555, None, &p, "")).unwrap();
        assert_eq!(v["@type"], "joinVideoChat");
        assert_eq!(v["group_call_id"], 555);
        assert!(v["participant_id"].is_null());
        assert_eq!(v["join_parameters"]["@type"], "groupCallJoinParameters");
        assert_eq!(v["invite_hash"], "");

        let v: serde_json::Value = serde_json::from_str(&join_video_chat(
            RequestId(5),
            555,
            Some(&MessageSenderRef::User(42)),
            &p,
            "hash",
        ))
        .unwrap();
        assert_eq!(v["participant_id"]["@type"], "messageSenderUser");
        assert_eq!(v["participant_id"]["user_id"], 42);

        let link = InputGroupCallRef::Link("https://t.me/abc".to_string());
        let v: serde_json::Value =
            serde_json::from_str(&join_group_call(RequestId(6), &link, &p)).unwrap();
        assert_eq!(v["@type"], "joinGroupCall");
        assert_eq!(v["input_group_call"]["@type"], "inputGroupCallLink");

        let v: serde_json::Value =
            serde_json::from_str(&get_group_call(RequestId(7), 555)).unwrap();
        assert_eq!(v["@type"], "getGroupCall");
        assert_eq!(v["group_call_id"], 555);

        let v: serde_json::Value =
            serde_json::from_str(&get_group_call_participants(RequestId(8), &link, 50)).unwrap();
        assert_eq!(v["@type"], "getGroupCallParticipants");
        assert_eq!(v["limit"], 50);

        let v: serde_json::Value =
            serde_json::from_str(&load_group_call_participants(RequestId(9), 555, 100)).unwrap();
        assert_eq!(v["@type"], "loadGroupCallParticipants");
        assert_eq!(v["group_call_id"], 555);
        assert_eq!(v["limit"], 100);

        let v: serde_json::Value =
            serde_json::from_str(&leave_group_call(RequestId(10), 555)).unwrap();
        assert_eq!(v["@type"], "leaveGroupCall");
        assert_eq!(v["group_call_id"], 555);

        let v: serde_json::Value =
            serde_json::from_str(&end_group_call(RequestId(11), 555)).unwrap();
        assert_eq!(v["@type"], "endGroupCall");
        assert_eq!(v["group_call_id"], 555);

        let v: serde_json::Value = serde_json::from_str(&toggle_group_call_is_my_video_enabled(
            RequestId(12),
            555,
            true,
        ))
        .unwrap();
        assert_eq!(v["@type"], "toggleGroupCallIsMyVideoEnabled");
        assert_eq!(v["is_my_video_enabled"], true);

        let v: serde_json::Value = serde_json::from_str(&toggle_group_call_is_my_video_paused(
            RequestId(13),
            555,
            true,
        ))
        .unwrap();
        assert_eq!(v["@type"], "toggleGroupCallIsMyVideoPaused");
        assert_eq!(v["is_my_video_paused"], true);

        let v: serde_json::Value = serde_json::from_str(&toggle_group_call_participant_is_muted(
            RequestId(14),
            555,
            &MessageSenderRef::User(42),
            true,
        ))
        .unwrap();
        assert_eq!(v["@type"], "toggleGroupCallParticipantIsMuted");
        assert_eq!(v["participant_id"]["user_id"], 42);
        assert_eq!(v["is_muted"], true);

        let v: serde_json::Value =
            serde_json::from_str(&toggle_group_call_participant_is_hand_raised(
                RequestId(15),
                555,
                &MessageSenderRef::User(41),
                true,
            ))
            .unwrap();
        assert_eq!(v["@type"], "toggleGroupCallParticipantIsHandRaised");
        assert_eq!(v["is_hand_raised"], true);

        // Exact schema name: toggleVideoChatMuteNewParticipants (not
        // toggleGroupCallMuteNewParticipants).
        let v: serde_json::Value = serde_json::from_str(&toggle_video_chat_mute_new_participants(
            RequestId(16),
            555,
            true,
        ))
        .unwrap();
        assert_eq!(v["@type"], "toggleVideoChatMuteNewParticipants");
        assert_eq!(v["mute_new_participants"], true);

        let v: serde_json::Value =
            serde_json::from_str(&set_video_chat_title(RequestId(17), 555, "Standup")).unwrap();
        assert_eq!(v["@type"], "setVideoChatTitle");
        assert_eq!(v["title"], "Standup");

        let v: serde_json::Value =
            serde_json::from_str(&get_video_chat_invite_link(RequestId(18), 555, true)).unwrap();
        assert_eq!(v["@type"], "getVideoChatInviteLink");
        assert_eq!(v["can_self_unmute"], true);

        // Phase C2h: the video-chat management requests.
        let v: serde_json::Value =
            serde_json::from_str(&revoke_group_call_invite_link(RequestId(21), 555)).unwrap();
        assert_eq!(v["@type"], "revokeGroupCallInviteLink");
        assert_eq!(v["group_call_id"], 555);

        let v: serde_json::Value = serde_json::from_str(&start_group_call_recording(
            RequestId(22),
            555,
            "Sync",
            true,
            false,
        ))
        .unwrap();
        assert_eq!(v["@type"], "startGroupCallRecording");
        assert_eq!(v["group_call_id"], 555);
        assert_eq!(v["title"], "Sync");
        assert_eq!(v["record_video"], true);
        assert_eq!(v["use_portrait_orientation"], false);

        let v: serde_json::Value =
            serde_json::from_str(&end_group_call_recording(RequestId(23), 555)).unwrap();
        assert_eq!(v["@type"], "endGroupCallRecording");
        assert_eq!(v["group_call_id"], 555);

        let v: serde_json::Value =
            serde_json::from_str(&start_scheduled_video_chat(RequestId(24), 555)).unwrap();
        assert_eq!(v["@type"], "startScheduledVideoChat");
        assert_eq!(v["group_call_id"], 555);

        // `toggleVideoChatEnabledStartNotification` (schema 1.8.67,
        // :14282).
        let v: serde_json::Value = serde_json::from_str(
            &toggle_video_chat_enabled_start_notification(RequestId(25), 555, true),
        )
        .unwrap();
        assert_eq!(v["@type"], "toggleVideoChatEnabledStartNotification");
        assert_eq!(v["group_call_id"], 555);
        assert_eq!(v["enabled_start_notification"], true);

        let v: serde_json::Value =
            serde_json::from_str(&get_video_chat_rtmp_url(RequestId(26), 51)).unwrap();
        assert_eq!(v["@type"], "getVideoChatRtmpUrl");
        assert_eq!(v["chat_id"], 51);

        let v: serde_json::Value =
            serde_json::from_str(&replace_video_chat_rtmp_url(RequestId(27), 51)).unwrap();
        assert_eq!(v["@type"], "replaceVideoChatRtmpUrl");
        assert_eq!(v["chat_id"], 51);

        let v: serde_json::Value =
            serde_json::from_str(&send_group_call_message(RequestId(28), 555, "hello")).unwrap();
        assert_eq!(v["@type"], "sendGroupCallMessage");
        assert_eq!(v["group_call_id"], 555);
        assert_eq!(v["text"]["@type"], "formattedText");
        assert_eq!(v["text"]["text"], "hello");
        assert_eq!(v["paid_message_star_count"], 0);

        let v: serde_json::Value = serde_json::from_str(&toggle_group_call_are_messages_allowed(
            RequestId(29),
            555,
            false,
        ))
        .unwrap();
        assert_eq!(v["@type"], "toggleGroupCallAreMessagesAllowed");
        assert_eq!(v["are_messages_allowed"], false);

        let v: serde_json::Value =
            serde_json::from_str(&decline_group_call_invitation(RequestId(19), 100, 7)).unwrap();
        assert_eq!(v["@type"], "declineGroupCallInvitation");
        assert_eq!(v["chat_id"], 100);
        assert_eq!(v["message_id"], 7);
    }

    #[test]
    fn group_call_participant_management_shapes_match_1_8_67() {
        // Phase C2f: `inviteGroupCallParticipant group_call_id:int32
        // user_id:int53 is_video:Bool = InviteGroupCallParticipantResult`
        // (schema 1.8.67, line 14375).
        let v: serde_json::Value =
            serde_json::from_str(&invite_group_call_participant(RequestId(31), 555, 42, true))
                .unwrap();
        assert_eq!(v["@type"], "inviteGroupCallParticipant");
        assert_eq!(v["@extra"], "31");
        assert_eq!(v["group_call_id"], 555);
        assert_eq!(v["user_id"], 42);
        assert_eq!(v["is_video"], true);

        // Phase C2f: `banGroupCallParticipants group_call_id:int32
        // user_ids:vector<int64> = Ok` (schema 1.8.67, line 14385).
        let v: serde_json::Value =
            serde_json::from_str(&ban_group_call_participants(RequestId(32), 555, &[42, 43]))
                .unwrap();
        assert_eq!(v["@type"], "banGroupCallParticipants");
        assert_eq!(v["@extra"], "32");
        assert_eq!(v["group_call_id"], 555);
        assert_eq!(v["user_ids"], serde_json::json!([42, 43]));

        // Phase C2f: `setGroupCallParticipantVolumeLevel
        // group_call_id:int32 participant_id:MessageSender
        // volume_level:int32 = Ok` (schema 1.8.67, line 14438).
        let v: serde_json::Value = serde_json::from_str(&set_group_call_participant_volume_level(
            RequestId(33),
            555,
            &MessageSenderRef::User(42),
            15000,
        ))
        .unwrap();
        assert_eq!(v["@type"], "setGroupCallParticipantVolumeLevel");
        assert_eq!(v["@extra"], "33");
        assert_eq!(v["group_call_id"], 555);
        assert_eq!(v["participant_id"]["@type"], "messageSenderUser");
        assert_eq!(v["participant_id"]["user_id"], 42);
        assert_eq!(v["volume_level"], 15000);
    }

    // M1: composer markup reaches the wire as `textEntities` with UTF-16
    // offsets (schema 1.8.67 lines 5743–5773); the marker syntax never
    // leaks into the sent text.
    #[test]
    fn send_text_converts_markup_to_entities() {
        let json = send_text(
            RequestId(50),
            ChatId(11),
            None,
            "😀 **bold** and `code`",
            None,
            &SendOptions::default(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let content = &v["input_message_content"]["text"];
        assert_eq!(content["text"], "😀 bold and code");
        let entities = content["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0]["type"]["@type"], "textEntityTypeBold");
        assert_eq!(entities[0]["offset"], 3); // 😀 = 2 UTF-16 units + space
        assert_eq!(entities[0]["length"], 4);
        assert_eq!(entities[1]["type"]["@type"], "textEntityTypeCode");
        assert_eq!(entities[1]["offset"], 12);
        assert_eq!(entities[1]["length"], 4);
    }

    #[test]
    fn send_text_link_preview_disabled_on_wire() {
        let json = send_text(
            RequestId(51),
            ChatId(11),
            None,
            "see https://example.com",
            None,
            &SendOptions {
                link_preview_disabled: true,
                ..SendOptions::default()
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let opts = &v["input_message_content"]["link_preview_options"];
        assert_eq!(opts["@type"], "linkPreviewOptions");
        assert_eq!(opts["is_disabled"], true);
    }

    /// MED4b: `getLinkPreview` shape matches the pinned schema
    /// (`schema/td_api.tl:14792`).
    #[test]
    fn get_link_preview_shape_matches_1_8_67() {
        let json = get_link_preview(RequestId(61), "https://example.com/story");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getLinkPreview");
        assert_eq!(v["@extra"], "61");
        assert_eq!(v["text"]["@type"], "formattedText");
        assert_eq!(v["text"]["text"], "https://example.com/story");
        assert_eq!(v["link_preview_options"], Value::Null);
        let schema = include_str!("../../schema/td_api.tl");
        let line = schema
            .lines()
            .find(|l| l.starts_with("getLinkPreview "))
            .expect("getLinkPreview in schema");
        assert_eq!(
            line,
            "getLinkPreview text:formattedText link_preview_options:linkPreviewOptions = LinkPreview;"
        );
    }

    /// MED4b: above-text + force-large ride `inputMessageText`;
    /// the force flags require the explicit URL (schema:2234-2235).
    #[test]
    fn send_text_link_preview_full_options_on_wire() {
        let json = send_text(
            RequestId(62),
            ChatId(11),
            None,
            "see https://example.com/story",
            None,
            &SendOptions {
                link_preview_above_text: true,
                link_preview_media: PreviewMediaSize::ForceLarge,
                ..SendOptions::default()
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let opts = &v["input_message_content"]["link_preview_options"];
        assert_eq!(opts["@type"], "linkPreviewOptions");
        assert_eq!(opts["is_disabled"], false);
        assert_eq!(opts["url"], "https://example.com/story");
        assert_eq!(opts["force_small_media"], false);
        assert_eq!(opts["force_large_media"], true);
        assert_eq!(opts["show_above_text"], true);
        // Above-text alone (no force) still sends the options object with
        // the detected URL — equivalent to empty per the schema (first
        // URL is used), and keeps one code path.
        let json = send_text(
            RequestId(63),
            ChatId(11),
            None,
            "see https://example.com/story",
            None,
            &SendOptions {
                link_preview_above_text: true,
                ..SendOptions::default()
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let opts = &v["input_message_content"]["link_preview_options"];
        assert_eq!(opts["show_above_text"], true);
        assert_eq!(opts["url"], "https://example.com/story");
        assert_eq!(opts["force_small_media"], false);
        assert_eq!(opts["force_large_media"], false);
        // Defaults keep the old behavior: null options.
        let json = send_text(
            RequestId(64),
            ChatId(11),
            None,
            "see https://example.com/story",
            None,
            &SendOptions::default(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["input_message_content"]["link_preview_options"],
            Value::Null
        );
    }

    #[test]
    fn send_text_silent_and_scheduled_options() {
        let json = send_text(
            RequestId(52),
            ChatId(11),
            None,
            "hi",
            None,
            &SendOptions {
                disable_notification: true,
                scheduling: ComposerScheduling::SendAtDate(1_700_000_000),
                ..SendOptions::default()
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let options = &v["options"];
        assert_eq!(options["@type"], "messageSendOptions");
        assert_eq!(options["disable_notification"], true);
        assert_eq!(
            options["scheduling_state"]["@type"],
            "messageSchedulingStateSendAtDate"
        );
        assert_eq!(options["scheduling_state"]["send_date"], 1_700_000_000);
        assert_eq!(options["scheduling_state"]["repeat_period"], 0);

        let json = send_text(
            RequestId(53),
            ChatId(11),
            None,
            "hi",
            None,
            &SendOptions {
                scheduling: ComposerScheduling::SendWhenOnline,
                ..SendOptions::default()
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["options"]["scheduling_state"]["@type"],
            "messageSchedulingStateSendWhenOnline"
        );

        let json = send_text(
            RequestId(54),
            ChatId(11),
            None,
            "hi",
            None,
            &SendOptions::default(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["options"]["disable_notification"], false);
        assert_eq!(v["options"]["scheduling_state"], Value::Null);
    }

    #[test]
    fn forward_messages_send_copy_and_remove_caption() {
        let v: serde_json::Value = serde_json::from_str(&forward_messages(
            RequestId(60),
            ChatId(11),
            ChatId(12),
            &[MessageId(101)],
            true,
            true,
        ))
        .unwrap();
        assert_eq!(v["@type"], "forwardMessages");
        assert_eq!(v["send_copy"], true);
        assert_eq!(v["remove_caption"], true);
        // `remove_caption` is ignored unless `send_copy` is true.
        let v: serde_json::Value = serde_json::from_str(&forward_messages(
            RequestId(61),
            ChatId(11),
            ChatId(12),
            &[MessageId(101)],
            false,
            true,
        ))
        .unwrap();
        assert_eq!(v["send_copy"], false);
        assert_eq!(v["remove_caption"], false);
    }

    #[test]
    fn delete_messages_revoke_flag() {
        let v: serde_json::Value = serde_json::from_str(&delete_messages(
            RequestId(62),
            ChatId(11),
            &[MessageId(101)],
            true,
        ))
        .unwrap();
        assert_eq!(v["@type"], "deleteMessages");
        assert_eq!(v["revoke"], true);
        let v: serde_json::Value = serde_json::from_str(&delete_messages(
            RequestId(63),
            ChatId(11),
            &[MessageId(101)],
            false,
        ))
        .unwrap();
        assert_eq!(v["revoke"], false);
    }

    #[test]
    fn m1_message_action_requests() {
        let v: serde_json::Value =
            serde_json::from_str(&unpin_all_chat_messages(RequestId(64), ChatId(11))).unwrap();
        assert_eq!(v["@type"], "unpinAllChatMessages");
        assert_eq!(v["chat_id"], 11);

        let v: serde_json::Value =
            serde_json::from_str(&get_message_link(RequestId(65), ChatId(11), MessageId(101)))
                .unwrap();
        assert_eq!(v["@type"], "getMessageLink");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 101);

        let v: serde_json::Value = serde_json::from_str(&resend_messages(
            RequestId(66),
            ChatId(11),
            &[MessageId(101)],
        ))
        .unwrap();
        assert_eq!(v["@type"], "resendMessages");
        assert_eq!(v["message_ids"], serde_json::json!([101]));

        let v: serde_json::Value =
            serde_json::from_str(&get_chat_scheduled_messages(RequestId(67), ChatId(11))).unwrap();
        assert_eq!(v["@type"], "getChatScheduledMessages");
        assert_eq!(v["chat_id"], 11);
    }

    #[test]
    fn g1_create_shapes_match_1_8_67() {
        // Slice G1: `createNewBasicGroupChat user_ids:vector<int53>
        // title:string message_auto_delete_time:int32 =
        // CreatedBasicGroupChat` (schema 1.8.67, line 13327).
        let v: serde_json::Value = serde_json::from_str(&create_new_basic_group_chat(
            RequestId(70),
            &[7, 8],
            "Study",
        ))
        .unwrap();
        assert_eq!(v["@type"], "createNewBasicGroupChat");
        assert_eq!(v["user_ids"], serde_json::json!([7, 8]));
        assert_eq!(v["title"], "Study");
        assert_eq!(v["message_auto_delete_time"], 0);

        // Slice G1: `createNewSupergroupChat title:string is_forum:Bool
        // is_channel:Bool description:string location:chatLocation
        // message_auto_delete_time:int32 for_import:Bool = Chat`
        // (schema 1.8.67, line 13337).
        let v: serde_json::Value = serde_json::from_str(&create_new_supergroup_chat(
            RequestId(71),
            "News",
            true,
            "desc",
        ))
        .unwrap();
        assert_eq!(v["@type"], "createNewSupergroupChat");
        assert_eq!(v["title"], "News");
        assert!(v["is_forum"].as_bool() == Some(false));
        assert!(v["is_channel"].as_bool() == Some(true));
        assert_eq!(v["description"], "desc");
        assert!(v["location"].is_null());
        assert!(v["for_import"].as_bool() == Some(false));
    }

    #[test]
    fn g1_group_admin_shapes_match_1_8_67() {
        // Slice G1: `toggleSupergroupIsBroadcastGroup supergroup_id:int53
        // = Ok` (schema 1.8.67, line 15221).
        let v: serde_json::Value =
            serde_json::from_str(&toggle_supergroup_is_broadcast_group(RequestId(72), 13)).unwrap();
        assert_eq!(v["@type"], "toggleSupergroupIsBroadcastGroup");
        assert_eq!(v["supergroup_id"], 13);

        // Slice G1: `addChatMembers chat_id:int53 user_ids:vector<int53>
        // = FailedToAddMembers` (schema 1.8.67, line 13584).
        let v: serde_json::Value =
            serde_json::from_str(&add_chat_members(RequestId(73), 11, &[7])).unwrap();
        assert_eq!(v["@type"], "addChatMembers");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["user_ids"], serde_json::json!([7]));

        // Slice G1: `setChatPermissions chat_id:int53
        // permissions:chatPermissions = Ok` (schema 1.8.67, line 13464).
        let perms =
            serde_json::json!({"@type": "chatPermissions", "can_send_basic_messages": true});
        let v: serde_json::Value =
            serde_json::from_str(&set_chat_permissions(RequestId(74), 11, &perms)).unwrap();
        assert_eq!(v["@type"], "setChatPermissions");
        assert_eq!(v["permissions"]["can_send_basic_messages"], true);

        // Slice G1: `replacePrimaryChatInviteLink chat_id:int53 =
        // ChatInviteLink` (schema 1.8.67, line 14089).
        let v: serde_json::Value =
            serde_json::from_str(&replace_primary_chat_invite_link(RequestId(75), 11)).unwrap();
        assert_eq!(v["@type"], "replacePrimaryChatInviteLink");
        assert_eq!(v["chat_id"], 11);

        // Slice G1: `toggleSupergroupJoinByRequest supergroup_id:int53
        // join_by_request:Bool guard_bot_user_id:int53
        // apply_to_invite_links:Bool = Ok` (schema 1.8.67, line 15188).
        let v: serde_json::Value =
            serde_json::from_str(&toggle_supergroup_join_by_request(RequestId(76), 13, true))
                .unwrap();
        assert_eq!(v["@type"], "toggleSupergroupJoinByRequest");
        assert_eq!(v["supergroup_id"], 13);
        assert!(v["join_by_request"].as_bool() == Some(true));
        assert_eq!(v["guard_bot_user_id"], 0);
        assert!(v["apply_to_invite_links"].as_bool() == Some(false));

        // Slice G1: `setSupergroupUsername supergroup_id:int53
        // username:string = Ok` (schema 1.8.67, line 15136).
        let v: serde_json::Value =
            serde_json::from_str(&set_supergroup_username(RequestId(77), 13, "news")).unwrap();
        assert_eq!(v["@type"], "setSupergroupUsername");
        assert_eq!(v["username"], "news");

        // Slice G1: `deleteChat chat_id:int53 = Ok` (schema 1.8.67, line
        // 11850).
        let v: serde_json::Value = serde_json::from_str(&delete_chat(RequestId(78), 11)).unwrap();
        assert_eq!(v["@type"], "deleteChat");
        assert_eq!(v["chat_id"], 11);

        // Slice CL1: `toggleChatIsPinned chat_list:ChatList chat_id:int53
        // is_pinned:Bool = Ok` (schema 1.8.67, line 13678).
        let v: serde_json::Value =
            serde_json::from_str(&toggle_chat_is_pinned(RequestId(79), 12, false, true)).unwrap();
        assert_eq!(v["@type"], "toggleChatIsPinned");
        assert_eq!(v["chat_id"], 12);
        assert_eq!(v["chat_list"]["@type"], "chatListMain");
        assert_eq!(v["is_pinned"], true);
        let v: serde_json::Value =
            serde_json::from_str(&toggle_chat_is_pinned(RequestId(80), 12, true, false)).unwrap();
        assert_eq!(v["chat_list"]["@type"], "chatListArchive");
        assert_eq!(v["is_pinned"], false);

        // Slice CL1: `toggleChatIsMarkedAsUnread chat_id:int53
        // is_marked_as_unread:Bool = Ok` (schema 1.8.67, line 13519).
        let v: serde_json::Value =
            serde_json::from_str(&toggle_chat_is_marked_as_unread(RequestId(81), 13, true))
                .unwrap();
        assert_eq!(v["@type"], "toggleChatIsMarkedAsUnread");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["is_marked_as_unread"], true);

        // Slice CL1: `deleteChatHistory chat_id:int53
        // remove_from_chat_list:Bool revoke:Bool = Ok` (schema 1.8.67,
        // line 11845). Clear-history keeps the chat in the list;
        // remove-from-list drops it (Telegram X `Tdlib.deleteChat`).
        let v: serde_json::Value =
            serde_json::from_str(&delete_chat_history(RequestId(82), 14, false, true)).unwrap();
        assert_eq!(v["@type"], "deleteChatHistory");
        assert_eq!(v["chat_id"], 14);
        assert_eq!(v["remove_from_chat_list"], false);
        assert_eq!(v["revoke"], true);
        let v: serde_json::Value =
            serde_json::from_str(&delete_chat_history(RequestId(83), 15, true, false)).unwrap();
        assert_eq!(v["remove_from_chat_list"], true);
        assert_eq!(v["revoke"], false);
    }

    #[test]
    fn g1_member_status_shapes_match_1_8_67() {
        // Slice G1: `chatMemberStatusRestricted is_member:Bool
        // restricted_until_date:int32 permissions:chatPermissions =
        // ChatMemberStatus` (schema 1.8.67, line 2510).
        let perms = serde_json::json!({"@type": "chatPermissions"});
        let v = chat_member_status_restricted_json(true, 1700000000, &perms);
        assert_eq!(v["@type"], "chatMemberStatusRestricted");
        assert!(v["is_member"].as_bool() == Some(true));
        assert_eq!(v["restricted_until_date"], 1700000000);
        assert_eq!(v["permissions"]["@type"], "chatPermissions");

        // Slice G1: `chatMemberStatusBanned banned_until_date:int32 =
        // ChatMemberStatus` (schema 1.8.67, line 2517).
        let v = chat_member_status_banned_json(0);
        assert_eq!(v["@type"], "chatMemberStatusBanned");
        assert_eq!(v["banned_until_date"], 0);

        // Slice G1: member-list filters (schema 1.8.67, lines 2563/2571/2574).
        assert_eq!(
            supergroup_members_filter_administrators_json()["@type"],
            "supergroupMembersFilterAdministrators"
        );
        let v = supergroup_members_filter_restricted_json("");
        assert_eq!(v["@type"], "supergroupMembersFilterRestricted");
        assert_eq!(v["query"], "");
        let v = supergroup_members_filter_banned_json("x");
        assert_eq!(v["@type"], "supergroupMembersFilterBanned");
        assert_eq!(v["query"], "x");
    }

    #[test]
    fn g1_basic_group_full_info_shape_matches_1_8_67() {
        // Slice G1: `getBasicGroupFullInfo basic_group_id:int53 =
        // BasicGroupFullInfo` (schema 1.8.67, line 11507).
        let v: Value = serde_json::from_str(&get_basic_group_full_info(RequestId(9), 42)).unwrap();
        assert_eq!(v["@type"], "getBasicGroupFullInfo");
        assert_eq!(v["basic_group_id"], 42);
        // `addChatMember` (schema 1.8.67, line 13578).
        let v: Value = serde_json::from_str(&add_chat_member(RequestId(9), 7, 11)).unwrap();
        assert_eq!(v["@type"], "addChatMember");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["user_id"], 11);
    }

    #[test]
    fn g1_reply_quote_shape_matches_1_8_67() {
        // Slice G1: `inputMessageReplyToMessage message_id:int53
        // quote:inputTextQuote checklist_task_id:int32 poll_option_id:string
        // = InputMessageReplyTo` (schema 1.8.67, line 3086) with
        // `inputTextQuote text:formattedText position:int32 =
        // InputTextQuote` (line 3056).
        let v = input_message_reply_to_with_quote(Some(MessageId(101)), Some(("sel", 7)));
        assert_eq!(v["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["message_id"], 101);
        assert_eq!(v["quote"]["@type"], "inputTextQuote");
        assert_eq!(v["quote"]["text"]["text"], "sel");
        assert_eq!(v["quote"]["position"], 7);

        // Whole-message reply keeps `quote: null` (existing behavior).
        let v = input_message_reply_to_with_quote(Some(MessageId(101)), None);
        assert!(v["quote"].is_null());
        assert!(input_message_reply_to_with_quote(None, Some(("sel", 7))).is_null());
    }

    #[test]
    fn g1_send_reply_quote_rides_send_text() {
        // Slice G1: `SendReply` with a quote produces
        // `inputMessageReplyToMessage` with a populated `inputTextQuote`
        // through the `sendMessage` builder.
        let reply = SendReply {
            message_id: MessageId(101),
            quote: Some(("sel".to_string(), 7)),
        };
        let json = send_text(
            RequestId(1),
            ChatId(7),
            None,
            "hi",
            Some(reply),
            &SendOptions::default(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 101);
        assert_eq!(v["reply_to"]["quote"]["@type"], "inputTextQuote");
        assert_eq!(v["reply_to"]["quote"]["text"]["text"], "sel");
        assert_eq!(v["reply_to"]["quote"]["position"], 7);
        // Plain replies keep `quote: null`.
        let json = send_text(
            RequestId(1),
            ChatId(7),
            None,
            "hi",
            Some(SendReply::plain(MessageId(101))),
            &SendOptions::default(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["reply_to"]["quote"].is_null());
    }

    #[test]
    fn g1_set_chat_member_tag_shape_matches_1_8_67() {
        // Slice G1: `setChatMemberTag chat_id:int53 user_id:int53
        // tag:string = Ok` (schema 1.8.67, line 13598) — the admin
        // custom-title setter.
        let json = set_chat_member_tag(RequestId(3), ChatId(7), 42, "boss");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setChatMemberTag");
        assert_eq!(v["@extra"], "3");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["user_id"], 42);
        assert_eq!(v["tag"], "boss");
        // Empty tag clears the title.
        let json = set_chat_member_tag(RequestId(3), ChatId(7), 42, "");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["tag"], "");
    }

    #[test]
    fn g2_toggle_sign_messages_shape_matches_1_8_67() {
        // Slice G2: `toggleSupergroupSignMessages supergroup_id:int53
        // sign_messages:Bool show_message_sender:Bool = Ok` (schema 1.8.67,
        // line 15175). `show_message_sender` is forced false when
        // `sign_messages` is false (Telegram X behavior).
        let json = toggle_supergroup_sign_messages(RequestId(3), 42, true, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "toggleSupergroupSignMessages");
        assert_eq!(v["supergroup_id"], 42);
        assert_eq!(v["sign_messages"], true);
        assert_eq!(v["show_message_sender"], true);
        let json = toggle_supergroup_sign_messages(RequestId(3), 42, false, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["sign_messages"], false);
        assert_eq!(v["show_message_sender"], false);
    }

    #[test]
    fn g2_toggle_anti_spam_shape_matches_1_8_67() {
        // Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled
        // supergroup_id:int53 has_aggressive_anti_spam_enabled:Bool = Ok`
        // (schema 1.8.67, line 15212).
        let json = toggle_supergroup_aggressive_anti_spam(RequestId(3), 42, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "toggleSupergroupHasAggressiveAntiSpamEnabled");
        assert_eq!(v["supergroup_id"], 42);
        assert_eq!(v["has_aggressive_anti_spam_enabled"], true);
    }

    #[test]
    fn g2_forum_topic_request_shapes_match_1_8_67() {
        // createForumTopic (schema 1.8.67, line 12665).
        let json = create_forum_topic(RequestId(3), ChatId(7), "Announcements");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "createForumTopic");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["name"], "Announcements");
        assert_eq!(v["is_name_implicit"], false);
        assert_eq!(v["icon"]["@type"], "forumTopicIcon");
        // editForumTopic (line 12674) — name only.
        let json = edit_forum_topic(RequestId(3), ChatId(7), 2, "News");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editForumTopic");
        assert_eq!(v["forum_topic_id"], 2);
        assert_eq!(v["name"], "News");
        assert_eq!(v["edit_icon_custom_emoji"], false);
        // toggleForumTopicIsClosed (line 12713).
        let json = toggle_forum_topic_closed(RequestId(3), ChatId(7), 2, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "toggleForumTopicIsClosed");
        assert_eq!(v["is_closed"], true);
        // toggleForumTopicIsPinned (line 12725).
        let json = toggle_forum_topic_pinned(RequestId(3), ChatId(7), 2, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "toggleForumTopicIsPinned");
        assert_eq!(v["is_pinned"], true);
        // deleteForumTopic (line 12736).
        let json = delete_forum_topic(RequestId(3), ChatId(7), 2);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteForumTopic");
        assert_eq!(v["forum_topic_id"], 2);
        // toggleGeneralForumTopicIsHidden (line 12718) — General only.
        let json = toggle_general_forum_topic_hidden(RequestId(3), ChatId(7), true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "toggleGeneralForumTopicIsHidden");
        assert_eq!(v["is_hidden"], true);
    }

    #[test]
    fn g2_thread_boost_welcome_request_shapes_match_1_8_67() {
        // getMessageThreadHistory (schema 1.8.67, line 11839).
        let json =
            get_message_thread_history(RequestId(3), ChatId(7), MessageId(101), MessageId(0), 50);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getMessageThreadHistory");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 101);
        assert_eq!(v["from_message_id"], 0);
        assert_eq!(v["limit"], 50);
        // getChatBoostStatus (line 13917) / getAvailableChatBoostSlots
        // (line 13914) / boostChat (line 13922).
        let json = get_chat_boost_status(RequestId(3), ChatId(7));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getChatBoostStatus");
        let json = get_available_chat_boost_slots(RequestId(3));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getAvailableChatBoostSlots");
        let json = boost_chat(RequestId(3), ChatId(7), &[1, 2]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "boostChat");
        assert_eq!(v["slot_ids"], serde_json::json!([1, 2]));
        // loadChatWelcomeMessages (line 12630) /
        // addChatWelcomeMessage (line 12639) /
        // editChatWelcomeMessage (line 12646) /
        // deleteChatWelcomeMessage (line 12651).
        let json = load_chat_welcome_messages(RequestId(3), ChatId(7));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "loadChatWelcomeMessages");
        let json = add_chat_welcome_message(RequestId(3), ChatId(7), "Welcome!");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "addChatWelcomeMessage");
        assert_eq!(v["input_message_content"]["text"]["text"], "Welcome!");
        let json = edit_chat_welcome_message(RequestId(3), ChatId(7), 5, "Hello!");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editChatWelcomeMessage");
        assert_eq!(v["welcome_message_id"], 5);
        let json = delete_chat_welcome_message(RequestId(3), ChatId(7), 5);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteChatWelcomeMessage");
        assert_eq!(v["welcome_message_id"], 5);
    }

    #[test]
    fn s1_post_story_shapes_match_1_8_67() {
        // Phase 9.3: `canPostStory chat_id:int53 = CanPostStoryResult`
        // (schema 1.8.67, line 13702); `postStory chat_id:int53
        // content:InputStoryContent areas:inputStoryAreas
        // caption:formattedText privacy_settings:StoryPrivacySettings
        // album_ids:vector<int32> active_period:int32
        // from_story_full_id:storyFullId is_posted_to_chat_page:Bool
        // protect_content:Bool = Story` (line 13715);
        // `inputStoryContentPhoto photo:InputFile
        // added_sticker_file_ids:vector<int32>` (line 6673);
        // `inputStoryContentVideo video:InputFile
        // added_sticker_file_ids:vector<int32> duration:double
        // cover_frame_timestamp:double is_animation:Bool` (line 6681);
        // `inputStoryAreas areas:vector<inputStoryArea>` (line 6619);
        // `inputFileLocal path:string` (line 325).
        let json = can_post_story(RequestId(70), ChatId(777));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "canPostStory");
        assert_eq!(v["@extra"], "70");
        assert_eq!(v["chat_id"], 777);

        let privacy = StoryPrivacy::Contacts.settings_json(&[]);
        let areas = json!({ "@type": "inputStoryAreas", "areas": [] });
        let json = post_story(
            RequestId(71),
            ChatId(777),
            StoryMediaKind::Photo,
            "/tmp/pic.jpg",
            "hello **bold**",
            privacy,
            areas.clone(),
            86400,
            false,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "postStory");
        assert_eq!(v["chat_id"], 777);
        assert_eq!(v["content"]["@type"], "inputStoryContentPhoto");
        assert_eq!(v["content"]["photo"]["@type"], "inputFileLocal");
        assert_eq!(v["content"]["photo"]["path"], "/tmp/pic.jpg");
        assert_eq!(
            v["content"]["added_sticker_file_ids"],
            serde_json::json!([])
        );
        assert_eq!(v["areas"]["@type"], "inputStoryAreas");
        assert_eq!(v["areas"]["areas"], serde_json::json!([]));
        assert_eq!(v["caption"]["text"], "hello bold");
        assert!(!v["caption"]["entities"].as_array().unwrap().is_empty());
        assert_eq!(
            v["privacy_settings"]["@type"],
            "storyPrivacySettingsContacts"
        );
        assert_eq!(v["album_ids"], serde_json::json!([]));
        assert_eq!(v["active_period"], 86400);
        assert!(v["from_story_full_id"].is_null());
        assert_eq!(v["is_posted_to_chat_page"], false);
        assert_eq!(v["protect_content"], false);

        let json = post_story(
            RequestId(72),
            ChatId(777),
            StoryMediaKind::Video,
            "/tmp/clip.mp4",
            "",
            StoryPrivacy::CloseFriends.settings_json(&[]),
            json!({ "@type": "inputStoryAreas", "areas": [] }),
            86400,
            false,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["content"]["@type"], "inputStoryContentVideo");
        assert_eq!(v["content"]["video"]["path"], "/tmp/clip.mp4");
        assert_eq!(v["content"]["duration"], 0.0);
        assert_eq!(v["content"]["is_animation"], false);
        assert_eq!(
            v["privacy_settings"]["@type"],
            "storyPrivacySettingsCloseFriends"
        );
    }

    #[test]
    fn s2_post_story_options_match_1_8_67() {
        // Phase 9.4: `postStory` (schema 1.8.67, line 13715) with the
        // previously fixed fields wired — a link + reaction areas block
        // (`td_api.tl:6619`), a 48h active period (td_api.tl:13715
        // comment: legal), and both toggles on.
        let areas = json!({
            "@type": "inputStoryAreas",
            "areas": [{
                "@type": "inputStoryArea",
                "position": {
                    "@type": "storyAreaPosition",
                    "x_percentage": 35.0, "y_percentage": 80.0,
                    "width_percentage": 30.0, "height_percentage": 9.0,
                    "rotation_angle": 0.0, "corner_radius_percentage": 20.0
                },
                "type": { "@type": "inputStoryAreaTypeLink", "url": "https://t.me/quill" }
            }]
        });
        let json = post_story(
            RequestId(73),
            ChatId(777),
            StoryMediaKind::Photo,
            "/tmp/pic.jpg",
            "",
            StoryPrivacy::Everyone.settings_json(&[]),
            areas,
            172800,
            true,
            true,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["areas"]["areas"][0]["type"]["@type"],
            "inputStoryAreaTypeLink"
        );
        assert_eq!(v["areas"]["areas"][0]["type"]["url"], "https://t.me/quill");
        assert_eq!(v["active_period"], 172800);
        assert_eq!(v["is_posted_to_chat_page"], true);
        assert_eq!(v["protect_content"], true);
    }

    #[test]
    fn cl3_report_chat_shape_matches_1_8_67() {
        // Slice CL3: `reportChat chat_id:int53 option_id:bytes
        // message_ids:vector<int53> text:string = ReportChatResult;`
        // (schema 1.8.67, line 15693) — the simple spam-report flow
        // sends empty option_id/message_ids/text (schema:3667).
        let json = report_chat(RequestId(61), 11);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "reportChat");
        assert_eq!(v["@extra"], "61");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["option_id"], "");
        assert_eq!(v["message_ids"], serde_json::json!([]));
        assert_eq!(v["text"], "");
    }

    #[test]
    fn cl3_set_message_sender_block_list_shapes_match_1_8_67() {
        // Slice CL3: `setMessageSenderBlockList sender_id:MessageSender
        // block_list:BlockList = Ok;` (schema 1.8.67, line 14492) —
        // block uses `blockListMain` (line 9692); unblock passes null
        // `block_list` (TGX `Tdlib.unblockSender`).
        let json = set_message_sender_block_list(RequestId(62), 11, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setMessageSenderBlockList");
        assert_eq!(v["@extra"], "62");
        assert_eq!(v["sender_id"]["@type"], "messageSenderUser");
        assert_eq!(v["sender_id"]["user_id"], 11);
        assert_eq!(v["block_list"]["@type"], "blockListMain");

        let json = set_message_sender_block_list(RequestId(63), 11, false);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setMessageSenderBlockList");
        assert!(v["block_list"].is_null());
    }

    #[test]
    fn b2_send_bot_start_message_and_get_bot_similar_bots_shapes_match_1_8_67() {
        // Slice B2: `sendBotStartMessage bot_user_id:int53 chat_id:int53
        // parameter:string = Message;` (schema 1.8.67, line 12216) and
        // `getBotSimilarBots bot_user_id:int53 = Users;` (line 11640).
        let json = send_bot_start_message(RequestId(64), 21, 21, "demo_xyz");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendBotStartMessage");
        assert_eq!(v["@extra"], "64");
        assert_eq!(v["bot_user_id"], 21);
        assert_eq!(v["chat_id"], 21);
        assert_eq!(v["parameter"], "demo_xyz");

        let json = get_bot_similar_bots(RequestId(65), 21);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getBotSimilarBots");
        assert_eq!(v["@extra"], "65");
        assert_eq!(v["bot_user_id"], 21);
    }

    #[test]
    fn b1_callback_query_with_password_shape_matches_1_8_67() {
        // B1: `getCallbackQueryAnswer` with `callbackQueryPayloadDataWithPassword`
        // (schema 1.8.67, lines 13138 / 7740); schema `bytes` is base64.
        let json = get_callback_query_answer_with_password(
            RequestId(61),
            ChatId(21),
            MessageId(301),
            "s3cr3t",
            &[1, 2, 3],
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getCallbackQueryAnswer");
        assert_eq!(v["@extra"], "61");
        assert_eq!(v["chat_id"], 21);
        assert_eq!(v["message_id"], 301);
        assert_eq!(
            v["payload"]["@type"],
            "callbackQueryPayloadDataWithPassword"
        );
        assert_eq!(v["payload"]["password"], "s3cr3t");
        assert_eq!(v["payload"]["data"], "AQID");
    }

    #[test]
    fn b1_callback_query_game_shape_matches_1_8_67() {
        // B1: `getCallbackQueryAnswer` with `callbackQueryPayloadGame`
        // (schema 1.8.67, line 7743).
        let json =
            get_callback_query_answer_game(RequestId(62), ChatId(21), MessageId(301), "chess");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getCallbackQueryAnswer");
        assert_eq!(v["@extra"], "62");
        assert_eq!(v["payload"]["@type"], "callbackQueryPayloadGame");
        assert_eq!(v["payload"]["game_short_name"], "chess");
    }

    #[test]
    fn b1_get_login_url_info_shape_matches_1_8_67() {
        // B1: `getLoginUrlInfo` (schema 1.8.67, line 12985).
        let json = get_login_url_info(RequestId(63), ChatId(21), MessageId(301), 7);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getLoginUrlInfo");
        assert_eq!(v["@extra"], "63");
        assert_eq!(v["chat_id"], 21);
        assert_eq!(v["message_id"], 301);
        assert_eq!(v["button_id"], 7);
    }

    #[test]
    fn b1_get_login_url_shape_matches_1_8_67() {
        // B1: `getLoginUrl` (schema 1.8.67, line 12993).
        let json = get_login_url(RequestId(65), ChatId(21), MessageId(301), 7, true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getLoginUrl");
        assert_eq!(v["@extra"], "65");
        assert_eq!(v["chat_id"], 21);
        assert_eq!(v["message_id"], 301);
        assert_eq!(v["button_id"], 7);
        assert_eq!(v["allow_write_access"], true);
    }

    #[test]
    fn b1_delete_chat_reply_markup_shape_matches_1_8_67() {
        // B1: `deleteChatReplyMarkup` (schema 1.8.67, line 13183).
        let json = delete_chat_reply_markup(RequestId(64), ChatId(21), MessageId(306));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteChatReplyMarkup");
        assert_eq!(v["@extra"], "64");
        assert_eq!(v["chat_id"], 21);
        assert_eq!(v["message_id"], 306);
    }
}
