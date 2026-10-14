//! The bar at the top of a private chat that a connected business bot
//! manages (`chat.business_bot_manage_bar`).
//!
//! tdesktop `history_view_contact_status.cpp` (`BusinessBotStatus`): the
//! bot's photo, its name, and a status line under it ("bot manages this
//! chat" when it can reply, "bot paused" while paused, "bot has access to
//! this chat" when it can only read). A round button toggles the bot:
//! "Stop" while it runs, "Start" while paused; it is hidden when the bot can
//! neither reply nor is paused. A "..." menu holds "Manage bot" (opens
//! `manage_url`) and "Remove bot from this chat" (the label is "Revoke
//! access to this chat" when the toggle button is hidden).
#![deny(clippy::indexing_slicing)]

use serde_json::{Value, json};

/// `businessBotManageBar` (TDLib 1.8.68).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessBotManageBar {
    pub bot_user_id: i64,
    pub manage_url: String,
    pub is_paused: bool,
    pub can_reply: bool,
}

/// What the toggle button does when pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotToggle {
    Pause,
    Resume,
}

impl BotToggle {
    /// tdesktop `lng_chatbot_button_pause` / `lng_chatbot_button_resume`.
    pub fn label(self) -> &'static str {
        match self {
            BotToggle::Pause => "Stop",
            BotToggle::Resume => "Start",
        }
    }

    /// The `is_paused` value the request sets.
    pub fn target_paused(self) -> bool {
        self == BotToggle::Pause
    }
}

impl BusinessBotManageBar {
    /// The status line under the bot's name.
    pub fn status(&self) -> &'static str {
        if self.is_paused {
            "bot paused"
        } else if self.can_reply {
            "bot manages this chat"
        } else {
            "bot has access to this chat"
        }
    }

    /// The toggle button, hidden when the bot can neither reply nor is
    /// paused.
    pub fn toggle(&self) -> Option<BotToggle> {
        if self.is_paused {
            Some(BotToggle::Resume)
        } else if self.can_reply {
            Some(BotToggle::Pause)
        } else {
            None
        }
    }

    /// The destructive menu item; reading-only bots are "revoked".
    pub fn remove_label(&self) -> &'static str {
        if self.toggle().is_some() {
            "Remove bot from this chat"
        } else {
            "Revoke access to this chat"
        }
    }

    /// Optimistic local state after the toggle request is sent.
    pub fn with_paused(&self, paused: bool) -> Self {
        Self {
            is_paused: paused,
            ..self.clone()
        }
    }
}

/// Parse `chat.business_bot_manage_bar` / the update's field. Null, missing
/// or a bar without a bot is no bar (tdesktop hides it without a bot).
pub fn parse_business_bot_manage_bar(value: Option<&Value>) -> Option<BusinessBotManageBar> {
    let value = value?;
    let bot_user_id = value.get("bot_user_id").and_then(Value::as_i64)?;
    if bot_user_id <= 0 {
        return None;
    }
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    Some(BusinessBotManageBar {
        bot_user_id,
        manage_url: value
            .get("manage_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        is_paused: flag("is_bot_paused"),
        can_reply: flag("can_bot_reply"),
    })
}

/// `toggleBusinessConnectedBotChatIsPaused chat_id:int53 is_paused:Bool = Ok;`
pub fn toggle_paused_request(extra: &str, chat_id: i64, is_paused: bool) -> String {
    json!({
        "@type": "toggleBusinessConnectedBotChatIsPaused",
        "@extra": extra,
        "chat_id": chat_id,
        "is_paused": is_paused,
    })
    .to_string()
}

/// `removeBusinessConnectedBotFromChat chat_id:int53 = Ok;`
pub fn remove_from_chat_request(extra: &str, chat_id: i64) -> String {
    json!({
        "@type": "removeBusinessConnectedBotFromChat",
        "@extra": extra,
        "chat_id": chat_id,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        BotToggle, BusinessBotManageBar, parse_business_bot_manage_bar, remove_from_chat_request,
        toggle_paused_request,
    };
    use serde_json::{Value, json};

    fn bar(is_paused: bool, can_reply: bool) -> BusinessBotManageBar {
        BusinessBotManageBar {
            bot_user_id: 42,
            manage_url: "https://t.me/bot?start=bizChat1".into(),
            is_paused,
            can_reply,
        }
    }

    #[test]
    fn parses_the_bar_and_ignores_empty_ones() {
        let value = json!({"@type":"businessBotManageBar","bot_user_id":42,
            "manage_url":"https://t.me/bot?start=x","is_bot_paused":true,"can_bot_reply":true});
        let parsed = parse_business_bot_manage_bar(Some(&value)).unwrap();
        assert_eq!(parsed.bot_user_id, 42);
        assert!(parsed.is_paused && parsed.can_reply);
        assert_eq!(parsed.manage_url, "https://t.me/bot?start=x");
        assert!(parse_business_bot_manage_bar(None).is_none());
        assert!(parse_business_bot_manage_bar(Some(&Value::Null)).is_none());
        let none = json!({"@type":"businessBotManageBar","bot_user_id":0});
        assert!(parse_business_bot_manage_bar(Some(&none)).is_none());
    }

    #[test]
    fn status_and_button_follow_tdesktop() {
        let running = bar(false, true);
        assert_eq!(running.status(), "bot manages this chat");
        assert_eq!(running.toggle(), Some(BotToggle::Pause));
        assert_eq!(BotToggle::Pause.label(), "Stop");
        assert_eq!(running.remove_label(), "Remove bot from this chat");

        let paused = bar(true, true);
        assert_eq!(paused.status(), "bot paused");
        assert_eq!(paused.toggle(), Some(BotToggle::Resume));
        assert_eq!(BotToggle::Resume.label(), "Start");
        // Paused wins even when the bot could no longer reply.
        assert_eq!(bar(true, false).toggle(), Some(BotToggle::Resume));

        let reader = bar(false, false);
        assert_eq!(reader.status(), "bot has access to this chat");
        assert_eq!(reader.toggle(), None);
        assert_eq!(reader.remove_label(), "Revoke access to this chat");
    }

    #[test]
    fn toggle_flips_the_paused_flag() {
        assert!(BotToggle::Pause.target_paused());
        assert!(!BotToggle::Resume.target_paused());
        assert!(bar(false, true).with_paused(true).is_paused);
    }

    #[test]
    fn request_json_matches_the_schema() {
        let v: Value = serde_json::from_str(&toggle_paused_request("7", 11, true)).unwrap();
        assert_eq!(v["@type"], "toggleBusinessConnectedBotChatIsPaused");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["is_paused"], true);
        let v: Value = serde_json::from_str(&remove_from_chat_request("8", 11)).unwrap();
        assert_eq!(v["@type"], "removeBusinessConnectedBotFromChat");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["@extra"], "8");
    }
}
