//! Telegram Business chatbots ("Chat Automation"): one bot connected to the
//! account, the chats it can reach and its rights.
//!
//! tdesktop `settings/business/settings_chatbots.cpp`: a username field
//! ("Enter bot URL or username") looks the bot up (exact username, then
//! up to three business-capable bots from search); "Add" connects it; the
//! recipients selector also offers exclusions in "Only Selected Chats";
//! the permissions list (`ChatbotsPermissionsLabels`) has "Read Messages"
//! always on; enabling gift, star or username rights shows a warning;
//! "Remove Bot" disconnects it. Saved when the screen closes
//! (`data_business_chatbots.cpp`).

use crate::business_settings::{BusinessChats, BusinessRecipients, ChatTypes};
use serde_json::{Value, json};

/// tdesktop `kMaxSearchBots`.
pub const MAX_FOUND_BOTS: usize = 3;

/// `businessBotRights`; `can_read_messages` is tdesktop's "Mark Messages
/// as Read" (its "Read Messages" is always granted).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BotRights {
    pub can_reply: bool,
    pub can_read_messages: bool,
    pub can_delete_sent_messages: bool,
    pub can_delete_all_messages: bool,
    pub can_edit_name: bool,
    pub can_edit_bio: bool,
    pub can_edit_profile_photo: bool,
    pub can_edit_username: bool,
    pub can_view_gifts_and_stars: bool,
    pub can_sell_gifts: bool,
    pub can_change_gift_settings: bool,
    pub can_transfer_and_upgrade_gifts: bool,
    pub can_transfer_stars: bool,
    pub can_manage_stories: bool,
}

/// One right of the permissions list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotRight {
    Reply,
    MarkAsRead,
    DeleteSent,
    DeleteReceived,
    EditName,
    EditBio,
    EditPhoto,
    EditUsername,
    ViewGifts,
    SellGifts,
    GiftSettings,
    TransferGifts,
    TransferStars,
    ManageStories,
}

/// One group of the permissions list: its title (none for the last) and
/// its rights with their labels.
pub type RightGroup = (Option<&'static str>, &'static [(BotRight, &'static str)]);

/// tdesktop's groups (`ChatbotsPermissionsLabels`).
pub const RIGHT_GROUPS: [RightGroup; 4] = [
    (
        Some("Manage Messages"),
        &[
            (BotRight::Reply, "Reply to Messages"),
            (BotRight::MarkAsRead, "Mark Messages as Read"),
            (BotRight::DeleteSent, "Delete Sent Messages"),
            (BotRight::DeleteReceived, "Delete Received Messages"),
        ],
    ),
    (
        Some("Manage Profile"),
        &[
            (BotRight::EditName, "Edit Name"),
            (BotRight::EditBio, "Edit Bio"),
            (BotRight::EditPhoto, "Edit Profile Picture"),
            (BotRight::EditUsername, "Edit Username"),
        ],
    ),
    (
        Some("Manage Gifts and Stars"),
        &[
            (BotRight::ViewGifts, "View Gifts"),
            (BotRight::SellGifts, "Sell Gifts"),
            (BotRight::GiftSettings, "Change Gift Settings"),
            (BotRight::TransferGifts, "Transfer and Upgrade Gifts"),
            (BotRight::TransferStars, "Transfer Stars"),
        ],
    ),
    (None, &[(BotRight::ManageStories, "Manage Stories")]),
];

impl BotRights {
    fn slot(&mut self, right: BotRight) -> &mut bool {
        match right {
            BotRight::Reply => &mut self.can_reply,
            BotRight::MarkAsRead => &mut self.can_read_messages,
            BotRight::DeleteSent => &mut self.can_delete_sent_messages,
            BotRight::DeleteReceived => &mut self.can_delete_all_messages,
            BotRight::EditName => &mut self.can_edit_name,
            BotRight::EditBio => &mut self.can_edit_bio,
            BotRight::EditPhoto => &mut self.can_edit_profile_photo,
            BotRight::EditUsername => &mut self.can_edit_username,
            BotRight::ViewGifts => &mut self.can_view_gifts_and_stars,
            BotRight::SellGifts => &mut self.can_sell_gifts,
            BotRight::GiftSettings => &mut self.can_change_gift_settings,
            BotRight::TransferGifts => &mut self.can_transfer_and_upgrade_gifts,
            BotRight::TransferStars => &mut self.can_transfer_stars,
            BotRight::ManageStories => &mut self.can_manage_stories,
        }
    }

    pub fn has(&self, right: BotRight) -> bool {
        let mut copy = *self;
        *copy.slot(right)
    }

    pub fn set(&mut self, right: BotRight, on: bool) {
        *self.slot(right) = on;
    }

    fn fields() -> [(&'static str, BotRight); 14] {
        [
            ("can_reply", BotRight::Reply),
            ("can_read_messages", BotRight::MarkAsRead),
            ("can_delete_sent_messages", BotRight::DeleteSent),
            ("can_delete_all_messages", BotRight::DeleteReceived),
            ("can_edit_name", BotRight::EditName),
            ("can_edit_bio", BotRight::EditBio),
            ("can_edit_profile_photo", BotRight::EditPhoto),
            ("can_edit_username", BotRight::EditUsername),
            ("can_view_gifts_and_stars", BotRight::ViewGifts),
            ("can_sell_gifts", BotRight::SellGifts),
            ("can_change_gift_settings", BotRight::GiftSettings),
            ("can_transfer_and_upgrade_gifts", BotRight::TransferGifts),
            ("can_transfer_stars", BotRight::TransferStars),
            ("can_manage_stories", BotRight::ManageStories),
        ]
    }

    pub fn from_value(value: &Value) -> Self {
        let mut rights = Self::default();
        for (key, right) in Self::fields() {
            rights.set(
                right,
                value.get(key).and_then(Value::as_bool).unwrap_or(false),
            );
        }
        rights
    }

    pub fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("@type".into(), "businessBotRights".into());
        for (key, right) in Self::fields() {
            map.insert(key.into(), self.has(right).into());
        }
        Value::Object(map)
    }

    /// tdesktop's warning box after turning on risky rights: the text,
    /// with `{bot}` for the bot's name.
    pub fn warning(was: &BotRights, now: &BotRights) -> Option<&'static str> {
        let enabled = |right| now.has(right) && !was.has(right);
        let gifts = enabled(BotRight::TransferGifts) || enabled(BotRight::SellGifts);
        let stars = enabled(BotRight::TransferStars);
        if gifts && stars {
            Some(
                "The bot {bot} will be able to manage your gifts and stars, including giving them away to other users.",
            )
        } else if gifts {
            Some(
                "The bot {bot} will be able to manage your gifts, including giving them away to other users.",
            )
        } else if stars {
            Some("The bot {bot} will be able to transfer your stars.")
        } else if enabled(BotRight::EditUsername) {
            Some(
                "The bot {bot} will be able to set and remove usernames for your account, which may result in the loss of your current username.",
            )
        } else {
            None
        }
    }
}

/// `businessConnectedBot`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedBot {
    pub bot_user_id: i64,
    pub recipients: BusinessRecipients,
    pub rights: BotRights,
}

/// Bot recipients from TDLib: like a message's, plus `excluded_chat_ids`
/// for "Only Selected Chats".
pub fn bot_recipients_from_value(value: &Value) -> BusinessRecipients {
    let mut recipients = BusinessRecipients::from_value(value);
    if !recipients.all_but_excluded {
        recipients.excluded.chat_ids = value
            .get("excluded_chat_ids")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default();
    }
    recipients
}

/// tdesktop `ForBotsToMTP`: the active side, plus the excluded chats of
/// "Only Selected Chats".
pub fn bot_recipients_to_json(recipients: &BusinessRecipients) -> Value {
    let mut value = recipients.to_json();
    if !recipients.all_but_excluded {
        value["excluded_chat_ids"] = json!(recipients.excluded.chat_ids);
    }
    value
}

/// tdesktop's exclude-picker save for a bot: in "Only Selected Chats" only
/// chats can be excluded (no types), and an excluded chat leaves the
/// included list. tdesktop also empties the included list there, which
/// would make exclusions in that mode impossible; Quill keeps it.
pub fn bot_set_excluded(recipients: &mut BusinessRecipients, mut value: BusinessChats) {
    if recipients.all_but_excluded {
        recipients.set_excluded(value);
        return;
    }
    value.types = ChatTypes::default();
    recipients
        .included
        .chat_ids
        .retain(|id| !value.chat_ids.contains(id));
    recipients.excluded = value;
    if recipients.included.is_empty() {
        recipients.all_but_excluded = true;
    }
}

/// tdesktop's include-picker save for a bot: an included chat leaves the
/// excluded list.
pub fn bot_set_included(recipients: &mut BusinessRecipients, value: BusinessChats) {
    recipients
        .excluded
        .chat_ids
        .retain(|id| !value.chat_ids.contains(id));
    recipients.set_included(value);
}

/// `businessConnectedBotInfo` (or the bare `businessConnectedBot`).
pub fn parse_connected_bot(value: &Value) -> Option<ConnectedBot> {
    let bot = value.get("bot").filter(|b| !b.is_null()).unwrap_or(value);
    Some(ConnectedBot {
        bot_user_id: bot.get("bot_user_id")?.as_i64()?,
        recipients: bot
            .get("recipients")
            .filter(|r| !r.is_null())
            .map(bot_recipients_from_value)
            .unwrap_or_default(),
        rights: bot
            .get("rights")
            .filter(|r| !r.is_null())
            .map(BotRights::from_value)
            .unwrap_or_default(),
    })
}

pub fn get_connected_bot_request(extra: &str) -> String {
    json!({"@type": "getBusinessConnectedBot", "@extra": extra}).to_string()
}

pub fn set_connected_bot_request(extra: &str, bot: &ConnectedBot) -> String {
    json!({
        "@type": "setBusinessConnectedBot",
        "@extra": extra,
        "bot": {
            "@type": "businessConnectedBot",
            "bot_user_id": bot.bot_user_id,
            "recipients": bot_recipients_to_json(&bot.recipients),
            "rights": bot.rights.to_json(),
        },
    })
    .to_string()
}

pub fn delete_connected_bot_request(extra: &str, bot_user_id: i64) -> String {
    json!({
        "@type": "deleteBusinessConnectedBot",
        "@extra": extra,
        "bot_user_id": bot_user_id,
    })
    .to_string()
}

/// tdesktop `ParseUsernameQuery`: `@name`, `name`, `t.me/name` or
/// `https://t.me/name` -> `name` when it only has letters, digits, `_`
/// and `.`.
pub fn parse_username_query(text: &str) -> Option<String> {
    let text = text.trim();
    let name = if let Some(rest) = text.strip_prefix('@') {
        rest.to_string()
    } else {
        let rest = text.strip_prefix("https://").unwrap_or(text);
        let is_host = |c: char| c.is_ascii_alphanumeric() || c == '.';
        let rest = match rest.find('/') {
            Some(at) if rest[..at].chars().all(is_host) => &rest[at + 1..],
            _ => rest,
        };
        rest.chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
            .collect()
    };
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    valid.then_some(name)
}

/// The lookup requests: the exact username, the account's own matching
/// chats and global results, both limited to bots.
pub fn search_public_chat_request(extra: &str, username: &str) -> String {
    json!({"@type": "searchPublicChat", "@extra": extra, "username": username}).to_string()
}

pub fn search_bots_on_server_request(extra: &str, query: &str) -> String {
    json!({
        "@type": "searchChatsOnServer",
        "@extra": extra,
        "query": query,
        "type_filter": {"@type": "searchChatTypeFilterBot"},
        "limit": 50,
    })
    .to_string()
}

pub fn search_public_bots_request(extra: &str, query: &str) -> String {
    json!({
        "@type": "searchPublicChats",
        "@extra": extra,
        "query": query,
        "type_filter": {"@type": "searchChatTypeFilterBot"},
    })
    .to_string()
}

/// tdesktop `MakeReadySearchState`: the exact match first, then the own
/// results, then global ones; only business bots, at most three.
pub fn merge_found_bots(
    exact: Option<i64>,
    mine: &[i64],
    global: &[i64],
    usable: impl Fn(i64) -> bool,
) -> Vec<i64> {
    let mut out = Vec::new();
    for id in exact
        .into_iter()
        .chain(mine.iter().copied())
        .chain(global.iter().copied())
    {
        if out.len() >= MAX_FOUND_BOTS {
            break;
        }
        if usable(id) && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

#[cfg(test)]
#[path = "business_bots_tests.rs"]
mod tests;
