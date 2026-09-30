use super::{formatted_caption, reaction_type_custom_emoji, reaction_type_emoji};
use crate::ids::{ChatId, RequestId};
use crate::story_composer::StoryMediaKind;
use serde_json::{Value, json};

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

/// Phase 9.2+: `setStoryReaction` with a `reactionTypeCustomEmoji` (TDLib
/// 1.8.67, `schema/td_api.tl:13809` — "Custom emoji reactions can be used
/// only by Telegram Premium users"; enforcement is server-side, the picker
/// gates on `availableReaction.needs_premium`). Separate builder (not a
/// widened `set_story_reaction`) so the existing emoji/remove call sites —
/// including the two in the frozen `src/ui/mod.rs` — keep their
/// `Option<&str>` signatures untouched.
pub fn set_story_custom_emoji_reaction(
    extra: RequestId,
    chat_id: ChatId,
    story_id: i32,
    custom_emoji_id: i64,
) -> String {
    json!({
        "@type": "setStoryReaction",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "reaction_type": reaction_type_custom_emoji(custom_emoji_id),
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

/// Phase 9.5: `getStoryInteractions` (TDLib 1.8.67,
/// `schema/td_api.tl:13819`) — one page of an own story's viewers.
/// `query` filters by name/username/title (empty = all);
/// `offset` is the previous page's `next_offset` (empty = first page).
/// The schema places no explicit max on `limit`; the viewer pages 50.
pub fn get_story_interactions(
    extra: RequestId,
    story_id: i32,
    query: &str,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getStoryInteractions",
        "@extra": extra.as_extra(),
        "story_id": story_id,
        "query": query,
        "only_contacts": false,
        "prefer_forwards": false,
        "prefer_with_reaction": false,
        "offset": offset,
        "limit": limit
    })
    .to_string()
}

/// Phase 9.5: `reportStory` (TDLib 1.8.67, `schema/td_api.tl:13835`) —
/// reports a story to the Telegram moderators. `option_id` is the
/// base64 `reportOption.id` from a `reportStoryResultOptionRequired`
/// answer (empty for the initial call); `text` is the extra detail
/// (empty for the initial call). Response is `ReportStoryResult`.
pub fn report_story(
    extra: RequestId,
    chat_id: ChatId,
    story_id: i32,
    option_id: &str,
    text: &str,
) -> String {
    json!({
        "@type": "reportStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "option_id": option_id,
        "text": text
    })
    .to_string()
}

/// Phase 9.5: `activateStoryStealthMode` (TDLib 1.8.67,
/// `schema/td_api.tl:13839`) — hides the current user's story views in
/// the last `story_stealth_mode_past_period` seconds and the next
/// `story_stealth_mode_future_period` seconds; Premium only. Response
/// is `ok`; the state lands as `updateStoryStealthMode`.
pub fn activate_story_stealth_mode(extra: RequestId) -> String {
    json!({
        "@type": "activateStoryStealthMode",
        "@extra": extra.as_extra(),
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

/// Phase 9.5: `editStory` (TDLib 1.8.67, `schema/td_api.tl:13732`) —
/// `editStory story_poster_chat_id:int53 story_id:int32
/// content:InputStoryContent areas:inputStoryAreas caption:formattedText
/// = Ok;` The schema comment: "pass null to keep the current
/// content/areas/caption". `content: None` keeps the media; `areas:
/// None` keeps the areas (areas can't change unless content does —
/// the driver rejects area-only edits); `caption: None` keeps the
/// caption. Response is `ok`; the edited story arrives via
/// `updateStory`.
pub fn edit_story(
    extra: RequestId,
    chat_id: ChatId,
    story_id: i32,
    content: Option<Value>,
    areas: Option<Value>,
    caption: Option<&str>,
) -> String {
    json!({
        "@type": "editStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "content": content.unwrap_or(Value::Null),
        "areas": areas.unwrap_or(Value::Null),
        "caption": caption.map(|text| formatted_caption(text, false)).unwrap_or(Value::Null)
    })
    .to_string()
}

/// Phase 9.5: `editStoryCover` (TDLib 1.8.67, `schema/td_api.tl:13738`) —
/// `editStoryCover story_poster_chat_id:int53 story_id:int32
/// cover_frame_timestamp:double = Ok;` Changes the video story's cover
/// frame. Only when `story.can_be_edited` (driver-gated).
pub fn edit_story_cover(
    extra: RequestId,
    chat_id: ChatId,
    story_id: i32,
    cover_frame_timestamp: f64,
) -> String {
    json!({
        "@type": "editStoryCover",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "cover_frame_timestamp": cover_frame_timestamp
    })
    .to_string()
}

/// Phase 9.5: `setStoryPrivacySettings` (TDLib 1.8.67,
/// `schema/td_api.tl:13743`) — `setStoryPrivacySettings story_id:int32
/// privacy_settings:StoryPrivacySettings = Ok;` Only for stories posted
/// on behalf of the current user with `story.can_set_privacy_settings`
/// (driver-gated). Response is `ok`.
pub fn set_story_privacy_settings(
    extra: RequestId,
    story_id: i32,
    privacy_settings: Value,
) -> String {
    json!({
        "@type": "setStoryPrivacySettings",
        "@extra": extra.as_extra(),
        "story_id": story_id,
        "privacy_settings": privacy_settings
    })
    .to_string()
}

/// Phase 9.5: `getChatsToPostStories` (TDLib 1.8.67,
/// `schema/td_api.tl:13698`) — `getChatsToPostStories = Chats;`
/// "Returns supergroup and channel chats in which the current user has
/// the right to post stories. The chats must be rechecked with
/// canPostStory before actually trying to post a story there."
pub fn get_chats_to_post_stories(extra: RequestId) -> String {
    json!({
        "@type": "getChatsToPostStories",
        "@extra": extra.as_extra()
    })
    .to_string()
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
/// Phase 9.5: `from_story` carries a repost source as `storyFullId`
/// (`td_api.tl:6766`); `chat_id` may be a channel/supergroup from
/// `getChatsToPostStories` (privacy is server-ignored for those —
/// schema comment on `postStory`).
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
    from_story: Option<(i64, i32)>,
    is_posted_to_chat_page: bool,
    protect_content: bool,
) -> String {
    let from_story_full_id = from_story.map(|(poster_chat_id, story_id)| {
        json!({
            "@type": "storyFullId",
            "poster_chat_id": poster_chat_id,
            "story_id": story_id
        })
    });
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
        "from_story_full_id": from_story_full_id.unwrap_or(Value::Null),
        "is_posted_to_chat_page": is_posted_to_chat_page,
        "protect_content": protect_content
    })
    .to_string()
}
