use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatList {
    Main,
    Archive,
    Folder(i32),
    Unknown,
}

/// `chatFolderInfo` (TDLib 1.8.67, `schema/td_api.tl:3485`). Only the fields
/// Quill needs for folder tabs: identifier, display name (plain text — the
/// schema allows only CustomEmoji entities in folder names, which are
/// dropped), icon name, and color id. `is_shareable` / `has_my_invite_links`
/// are dropped (folder create/edit/share is out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFolderInfo {
    pub id: i32,
    pub name: String,
    pub icon_name: String,
    pub color_id: i32,
}

/// Parity slice: the full editable `chatFolder` spec (TDLib 1.8.67,
/// `schema/td_api.tl:3476`) behind `createChatFolder` / `editChatFolder`
/// (`:13358` / `:13361`) and returned by `getChatFolder` (`:13355`).
/// Quill always sends `icon: null` (default icon) and `color_id: -1`
/// (disabled) — custom-emoji icon rendering is out of scope — and
/// `is_shareable: false` (invite links out of scope). The remaining fields
/// are the create/edit dialog's model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatFolderSpec {
    pub name: String,
    pub pinned_chat_ids: Vec<i64>,
    pub included_chat_ids: Vec<i64>,
    pub excluded_chat_ids: Vec<i64>,
    pub exclude_muted: bool,
    pub exclude_read: bool,
    pub exclude_archived: bool,
    pub include_contacts: bool,
    pub include_non_contacts: bool,
    pub include_bots: bool,
    pub include_groups: bool,
    pub include_channels: bool,
}

pub(crate) fn parse_chat_list(value: Option<&Value>) -> ChatList {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("chatListMain") => ChatList::Main,
        Some("chatListArchive") => ChatList::Archive,
        Some("chatListFolder") => ChatList::Folder(
            value
                .and_then(|v| v.get("chat_folder_id"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        ),
        _ => ChatList::Unknown,
    }
}

/// `chatFolderInfo` (TDLib 1.8.67, `schema/td_api.tl:3485`): id from
/// `chat_folder_id`-style int32, name from `chatFolderName` (`:3458`) →
/// `formattedText.text` (`:117`), icon from `chatFolderIcon.name` (`:3453`).
pub(crate) fn parse_chat_folder_info(value: &Value) -> Option<ChatFolderInfo> {
    if value.get("@type").and_then(Value::as_str) != Some("chatFolderInfo") {
        return None;
    }
    let name = value
        .get("name")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let icon_name = value
        .get("icon")
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(ChatFolderInfo {
        id: value.get("id").and_then(Value::as_i64).unwrap_or(0) as i32,
        name,
        icon_name,
        color_id: value.get("color_id").and_then(Value::as_i64).unwrap_or(-1) as i32,
    })
}

/// Parity slice: parse the full `chatFolder` spec (TDLib 1.8.67,
/// `schema/td_api.tl:3476`) from a `getChatFolder` response (`:13355`).
/// `name` carries only CustomEmoji entities per the schema docs, so the
/// plain text is taken. Returns `None` when `@type` is not `chatFolder`.
pub(crate) fn parse_chat_folder(value: &Value) -> Option<ChatFolderSpec> {
    if value.get("@type").and_then(Value::as_str) != Some("chatFolder") {
        return None;
    }
    let name = value
        .get("name")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let ids = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_i64).collect::<Vec<i64>>())
            .unwrap_or_default()
    };
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    Some(ChatFolderSpec {
        name,
        pinned_chat_ids: ids("pinned_chat_ids"),
        included_chat_ids: ids("included_chat_ids"),
        excluded_chat_ids: ids("excluded_chat_ids"),
        exclude_muted: flag("exclude_muted"),
        exclude_read: flag("exclude_read"),
        exclude_archived: flag("exclude_archived"),
        include_contacts: flag("include_contacts"),
        include_non_contacts: flag("include_non_contacts"),
        include_bots: flag("include_bots"),
        include_groups: flag("include_groups"),
        include_channels: flag("include_channels"),
    })
}
