//! Mini-app objects (TDLib 1.8.68): `attachmentMenuBot` (line 7782),
//! `foundWebApp` (line 1131), `webAppInfo` (1141), `webAppUrl` (1136),
//! `mainWebApp` (1146), `customRequestResult`.

use super::json_helpers::{int53_or_zero, json_field_str};
use serde_json::Value;

/// A bot in the attachment or side menu. The icons are left out: Quill
/// draws the bot's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentMenuBot {
    pub bot_user_id: i64,
    pub name: String,
    pub supports_self_chat: bool,
    pub supports_user_chats: bool,
    pub supports_bot_chats: bool,
    pub supports_group_chats: bool,
    pub supports_channel_chats: bool,
    /// The bot asks to message the person; the add box offers a checkbox.
    pub request_write_access: bool,
    /// Explicitly added by the person; otherwise the first launch must
    /// `toggleBotIsAddedToAttachmentMenu` first.
    pub is_added: bool,
    pub show_in_attachment_menu: bool,
    pub show_in_side_menu: bool,
}

/// `foundWebApp`: a named app (`t.me/bot/app`), with what the open box
/// must say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundWebApp {
    pub short_name: String,
    pub title: String,
    pub description: String,
    pub request_write_access: bool,
    pub skip_confirmation: bool,
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn parse_attachment_menu_bot(value: &Value) -> Option<AttachmentMenuBot> {
    let bot_user_id = int53_or_zero(value.get("bot_user_id"));
    if bot_user_id == 0 {
        return None;
    }
    Some(AttachmentMenuBot {
        bot_user_id,
        name: json_field_str(value, "name"),
        supports_self_chat: flag(value, "supports_self_chat"),
        supports_user_chats: flag(value, "supports_user_chats"),
        supports_bot_chats: flag(value, "supports_bot_chats"),
        supports_group_chats: flag(value, "supports_group_chats"),
        supports_channel_chats: flag(value, "supports_channel_chats"),
        request_write_access: flag(value, "request_write_access"),
        is_added: flag(value, "is_added"),
        show_in_attachment_menu: flag(value, "show_in_attachment_menu"),
        show_in_side_menu: flag(value, "show_in_side_menu"),
    })
}

pub(crate) fn parse_attachment_menu_bots(value: &Value) -> Vec<AttachmentMenuBot> {
    value
        .get("bots")
        .and_then(Value::as_array)
        .map(|bots| bots.iter().filter_map(parse_attachment_menu_bot).collect())
        .unwrap_or_default()
}

pub(crate) fn parse_found_web_app(value: &Value) -> FoundWebApp {
    let app = value.get("web_app").unwrap_or(&Value::Null);
    FoundWebApp {
        short_name: json_field_str(app, "short_name"),
        title: json_field_str(app, "title"),
        description: json_field_str(app, "description"),
        request_write_access: flag(value, "request_write_access"),
        skip_confirmation: flag(value, "skip_confirmation"),
    }
}

/// The `url` string of a `webAppUrl` object (or of a field holding one).
pub(crate) fn web_app_url(value: &Value) -> String {
    match value.get("url") {
        Some(Value::String(url)) => url.clone(),
        Some(inner @ Value::Object(_)) => json_field_str(inner, "url"),
        _ => String::new(),
    }
}

impl AttachmentMenuBot {
    /// Whether the bot can open from the attachment menu of a chat of this
    /// kind (tdesktop `PeerMatchesTypes`).
    pub fn supports_chat(&self, kind: AttachChatKind) -> bool {
        match kind {
            AttachChatKind::SelfChat => self.supports_self_chat,
            AttachChatKind::User => self.supports_user_chats,
            AttachChatKind::Bot => self.supports_bot_chats,
            AttachChatKind::Group => self.supports_group_chats,
            AttachChatKind::Channel => self.supports_channel_chats,
        }
    }
}

/// The chat kinds `attachmentMenuBot` distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachChatKind {
    SelfChat,
    User,
    Bot,
    Group,
    Channel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn attachment_menu_bot_parses_flags_and_name() {
        let value = json!({
            "@type": "attachmentMenuBot", "bot_user_id": 42, "name": "Shop",
            "supports_user_chats": true, "supports_group_chats": true,
            "request_write_access": true, "is_added": false,
            "show_in_attachment_menu": true, "show_in_side_menu": false,
            "default_icon": {"@type": "file", "id": 7}
        });
        let bot = parse_attachment_menu_bot(&value).unwrap();
        assert_eq!(bot.bot_user_id, 42);
        assert_eq!(bot.name, "Shop");
        assert!(bot.supports_chat(AttachChatKind::User));
        assert!(bot.supports_chat(AttachChatKind::Group));
        assert!(!bot.supports_chat(AttachChatKind::Channel));
        assert!(!bot.supports_chat(AttachChatKind::SelfChat));
        assert!(bot.request_write_access);
        assert!(!bot.is_added);
        assert!(parse_attachment_menu_bot(&json!({"name": "x"})).is_none());
        let list = parse_attachment_menu_bots(&json!({"bots": [value, {"bot_user_id": 0}]}));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn found_web_app_and_urls() {
        let found = parse_found_web_app(&json!({
            "web_app": {"short_name": "shop", "title": "Shop", "description": "Buy"},
            "request_write_access": true, "skip_confirmation": false
        }));
        assert_eq!(found.short_name, "shop");
        assert_eq!(found.title, "Shop");
        assert!(found.request_write_access);
        assert!(!found.skip_confirmation);
        assert_eq!(web_app_url(&json!({"url": "https://a/"})), "https://a/");
        assert_eq!(
            web_app_url(&json!({"url": {"@type": "webAppUrl", "url": "https://b/"}})),
            "https://b/"
        );
        assert_eq!(web_app_url(&json!({})), "");
    }
}
