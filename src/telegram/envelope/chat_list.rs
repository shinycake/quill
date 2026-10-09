use super::SatI32;
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
/// dropped), icon name, color id, and the share flags that decide whether
/// the "Share" action shows its link list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFolderInfo {
    pub id: i32,
    pub name: String,
    pub icon_name: String,
    pub color_id: i32,
    /// At least one invite link has been created for the folder.
    pub is_shareable: bool,
    /// The current user created invite links for the folder.
    pub has_my_invite_links: bool,
}

/// Parity slice: the full editable `chatFolder` spec (TDLib 1.8.67,
/// `schema/td_api.tl:3476`) behind `createChatFolder` / `editChatFolder`
/// (`:13358` / `:13361`) and returned by `getChatFolder` (`:13355`).
/// `icon_name` is the chosen `chatFolderIcon.name` (`None` = TDLib's default
/// icon, sent as `icon: null`); `color_id` is carried through unchanged
/// (−1 = no tag color).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFolderSpec {
    pub name: String,
    pub icon_name: Option<String>,
    pub color_id: i32,
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

impl Default for ChatFolderSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            icon_name: None,
            color_id: -1,
            pinned_chat_ids: Vec::new(),
            included_chat_ids: Vec::new(),
            excluded_chat_ids: Vec::new(),
            exclude_muted: false,
            exclude_read: false,
            exclude_archived: false,
            include_contacts: false,
            include_non_contacts: false,
            include_bots: false,
            include_groups: false,
            include_channels: false,
        }
    }
}

/// `chatFolderInviteLink` (`schema/td_api.tl:3801`): a shareable link to a
/// folder, with the chats a joiner will get.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFolderInviteLink {
    pub invite_link: String,
    pub name: String,
    pub chat_ids: Vec<i64>,
}

/// `recommendedChatFolder` (`schema/td_api.tl:3813`): a folder suggested by
/// Telegram, ready to pass to `createChatFolder`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecommendedChatFolder {
    pub spec: ChatFolderSpec,
    pub description: String,
}

/// `chatFolderInviteLinkInfo` (`schema/td_api.tl:3810`): what an `addlist`
/// link offers. `folder.id` is 0 when the user has no such folder yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFolderInviteLinkInfo {
    pub folder: ChatFolderInfo,
    pub missing_chat_ids: Vec<i64>,
    pub added_chat_ids: Vec<i64>,
}

fn id_list(value: &Value, key: &str) -> Vec<i64> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

pub(crate) fn parse_chat_folder_invite_link(value: &Value) -> Option<ChatFolderInviteLink> {
    if value.get("@type").and_then(Value::as_str) != Some("chatFolderInviteLink") {
        return None;
    }
    Some(ChatFolderInviteLink {
        invite_link: value
            .get("invite_link")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        chat_ids: id_list(value, "chat_ids"),
    })
}

pub(crate) fn parse_chat_folder_invite_links(value: &Value) -> Option<Vec<ChatFolderInviteLink>> {
    if value.get("@type").and_then(Value::as_str) != Some("chatFolderInviteLinks") {
        return None;
    }
    Some(
        value
            .get("invite_links")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(parse_chat_folder_invite_link)
                    .collect()
            })
            .unwrap_or_default(),
    )
}

/// `premiumLimit` — the answer of `getPremiumLimit` (`schema/td_api.tl:8559`):
/// the constructor name of the limit type plus the free and Premium values.
pub(crate) fn parse_premium_limit(value: &Value) -> Option<(String, i32, i32)> {
    if value.get("@type").and_then(Value::as_str) != Some("premiumLimit") {
        return None;
    }
    let type_name = value.get("type")?.get("@type")?.as_str()?.to_string();
    let number = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_i64)
            .map(|n| i32::try_from(n).unwrap_or(i32::MAX))
    };
    Some((
        type_name,
        number("default_value")?,
        number("premium_value")?,
    ))
}

pub(crate) fn parse_recommended_chat_folders(value: &Value) -> Option<Vec<RecommendedChatFolder>> {
    if value.get("@type").and_then(Value::as_str) != Some("recommendedChatFolders") {
        return None;
    }
    Some(
        value
            .get("chat_folders")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        Some(RecommendedChatFolder {
                            spec: parse_chat_folder(item.get("folder")?)?,
                            description: item
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    )
}

pub(crate) fn parse_chat_folder_invite_link_info(
    value: &Value,
) -> Option<ChatFolderInviteLinkInfo> {
    if value.get("@type").and_then(Value::as_str) != Some("chatFolderInviteLinkInfo") {
        return None;
    }
    Some(ChatFolderInviteLinkInfo {
        folder: parse_chat_folder_info(value.get("chat_folder_info")?)?,
        missing_chat_ids: id_list(value, "missing_chat_ids"),
        added_chat_ids: id_list(value, "added_chat_ids"),
    })
}

pub(crate) fn parse_chat_list(value: Option<&Value>) -> ChatList {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("chatListMain") => ChatList::Main,
        Some("chatListArchive") => ChatList::Archive,
        Some("chatListFolder") => ChatList::Folder(
            value
                .and_then(|v| v.get("chat_folder_id"))
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
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
        id: value
            .get("id")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        name,
        icon_name,
        color_id: value
            .get("color_id")
            .and_then(Value::as_i64)
            .unwrap_or(-1)
            .sat_i32(),
        is_shareable: value
            .get("is_shareable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        has_my_invite_links: value
            .get("has_my_invite_links")
            .and_then(Value::as_bool)
            .unwrap_or(false),
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
    let ids = |key: &str| id_list(value, key);
    let icon_name = value
        .get("icon")
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    Some(ChatFolderSpec {
        name,
        icon_name,
        color_id: value.get("color_id").and_then(Value::as_i64).unwrap_or(-1) as i32,
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
