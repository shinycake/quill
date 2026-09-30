use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::{Value, json};

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
