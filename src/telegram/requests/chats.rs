use super::{SendReply, send_reply_value};
use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::{Value, json};

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

/// Slice: group/channel title edit — `setChatTitle` (TDLib 1.8.67,
/// `schema/td_api.tl:13430`):
/// `setChatTitle chat_id:int53 title:string = Ok;`
/// "Changes the chat title. Supported only for basic groups, supergroups
/// and channels. Requires can_change_info member right" — "New title of
/// the chat; 1-128 characters". Length is validated client-side by the
/// driver; the server confirms via `updateChatTitle`.
pub fn set_chat_title(extra: RequestId, chat_id: ChatId, title: &str) -> String {
    json!({
        "@type": "setChatTitle",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "title": title,
    })
    .to_string()
}

/// Slice: group/channel description edit — `setChatDescription`
/// (TDLib 1.8.67, `schema/td_api.tl:13533`):
/// `setChatDescription chat_id:int53 description:string = Ok;`
/// "Changes information about a chat. Available for basic groups,
/// supergroups, and channels. Requires can_change_info member right" —
/// "New chat description; 0-255 characters". Empty string clears the
/// description. TDLib has no `updateChatDescription` broadcast; the new
/// description arrives on the next `getSupergroupFullInfo` /
/// `getBasicGroupFullInfo` pull. Length is validated client-side by the
/// driver.
pub fn set_chat_description(extra: RequestId, chat_id: ChatId, description: &str) -> String {
    json!({
        "@type": "setChatDescription",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "description": description,
    })
    .to_string()
}

/// Slice: group/channel photo edit — `setChatPhoto` (TDLib 1.8.67,
/// `schema/td_api.tl:13435`):
/// `setChatPhoto chat_id:int53 photo:InputChatPhoto = Ok;`
/// "Changes the photo of a chat. Supported only for basic groups,
/// supergroups and channels. Requires can_change_info member right" —
/// "New chat photo; pass null to delete the chat photo". `photo_json` is
/// the `InputChatPhoto` object — the `inputChatPhotoStatic` /
/// `inputFileLocal` shape (schema lines 1042, 1039), or
/// `serde_json::Value::Null` to delete (a null top-level `photo`, not an
/// `inputChatPhotoPrevious`, which is only "a previously used profile
/// photo of the current user"). The server confirms via `updateChatPhoto`.
pub fn set_chat_photo(extra: RequestId, chat_id: ChatId, photo_json: Value) -> String {
    json!({
        "@type": "setChatPhoto",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "photo": photo_json,
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

/// `setReactionNotificationSettings` (TDLib 1.8.67, line 13668). Full
/// settings object; callers copy the current settings and change one
/// field. No getter exists — the current values arrive as
/// `updateReactionNotificationSettings`.
pub fn set_reaction_notification_settings(
    extra: RequestId,
    settings: &crate::telegram::envelope::ReactionNotificationSettings,
) -> String {
    let source = |s: crate::telegram::envelope::ReactionNotificationSource| json!({ "@type": s.type_name() });
    json!({
        "@type": "setReactionNotificationSettings",
        "@extra": extra.as_extra(),
        "notification_settings": {
            "@type": "reactionNotificationSettings",
            "message_reaction_source": source(settings.message_reaction_source),
            "story_reaction_source": source(settings.story_reaction_source),
            "poll_vote_source": source(settings.poll_vote_source),
            "sound_id": settings.sound_id,
            "show_preview": settings.show_preview
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
