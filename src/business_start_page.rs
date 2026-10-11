//! The Telegram Business start page ("Custom Intro"): what people see in
//! an empty chat with the account before they write.
//!
//! tdesktop `settings/business/settings_chat_intro.cpp`: a preview, a
//! title (32 characters by default) and a message (70), "Choose Sticker"
//! (a random hello sticker when none is chosen), "Reset to Default", saved
//! when the screen closes. `history_view_about_view.cpp` shows a set
//! start page in empty chats with "{name} added the message below for all
//! empty chats. How?".

use crate::telegram::envelope::{StickerItem, parse_sticker_value};
use serde_json::{Value, json};

/// tdesktop's `intro_title_length_limit` default; TDLib reports the real
/// limit as the option `business_start_page_title_length_max`.
pub const TITLE_MAX: usize = 32;
/// tdesktop's `intro_description_length_limit` default; TDLib option
/// `business_start_page_message_length_max`.
pub const MESSAGE_MAX: usize = 70;

/// tdesktop `lng_chat_intro_default_title`.
pub const DEFAULT_TITLE: &str = "No messages here yet...";
/// tdesktop `lng_chat_intro_default_message`.
pub const DEFAULT_MESSAGE: &str = "Send a message or click on the greeting below";

/// `businessStartPage`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartPage {
    pub title: String,
    pub message: String,
    pub sticker: Option<StickerItem>,
}

impl StartPage {
    /// tdesktop `ChatIntro::operator bool`.
    pub fn is_empty(&self) -> bool {
        self.title.is_empty() && self.message.is_empty() && self.sticker.is_none()
    }

    pub fn shown_title(&self) -> &str {
        if self.title.is_empty() {
            DEFAULT_TITLE
        } else {
            &self.title
        }
    }

    pub fn shown_message(&self) -> &str {
        if self.message.is_empty() {
            DEFAULT_MESSAGE
        } else {
            &self.message
        }
    }
}

/// `businessInfo.start_page`; `None` when absent or empty.
pub fn parse_start_page(value: Option<&Value>) -> Option<StartPage> {
    let value = value.filter(|v| !v.is_null())?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let page = StartPage {
        title: text("title"),
        message: text("message"),
        sticker: parse_sticker_value(value.get("sticker").filter(|v| !v.is_null())).0,
    };
    (!page.is_empty()).then_some(page)
}

/// `setBusinessStartPage`. An empty title, message and no sticker remove
/// the custom start page (`null`). `sticker_file_id` is the sticker's
/// TDLib file id.
pub fn set_start_page_request(
    extra: &str,
    title: &str,
    message: &str,
    sticker_file_id: Option<i32>,
) -> String {
    let page = if title.is_empty() && message.is_empty() && sticker_file_id.is_none() {
        Value::Null
    } else {
        json!({
            "@type": "inputBusinessStartPage",
            "title": title,
            "message": message,
            "sticker": sticker_file_id.map_or(Value::Null, |id| {
                json!({"@type": "inputFileId", "id": id})
            }),
        })
    };
    json!({
        "@type": "setBusinessStartPage",
        "@extra": extra,
        "start_page": page,
    })
    .to_string()
}

/// "{name} added the message below for all empty chats." (tdesktop
/// `lng_action_set_chat_intro`, without its "How?" link).
pub fn added_line(name: &str) -> String {
    format!("{name} added the message below for all empty chats.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_start_page_with_a_sticker() {
        let value: Value = serde_json::from_str(r#"{"@type":"businessStartPage","title":"Hi","message":"Ask away","sticker":{"@type":"sticker","id":5,"set_id":9,"width":512,"height":512,"emoji":"👋","format":{"@type":"stickerFormatWebp"},"full_type":{"@type":"stickerFullTypeRegular","premium_animation":null},"thumbnail":null,"sticker":{"@type":"file","id":44,"size":100,"expected_size":100,"local":{"path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":100}}}}"#).unwrap();
        let page = parse_start_page(Some(&value)).unwrap();
        assert_eq!(page.shown_title(), "Hi");
        assert_eq!(page.shown_message(), "Ask away");
        assert_eq!(page.sticker.unwrap().file_id.0, 44);
    }

    #[test]
    fn empty_pages_are_none_and_use_the_defaults() {
        let value: Value =
            serde_json::from_str(r#"{"title":"","message":"","sticker":null}"#).unwrap();
        assert!(parse_start_page(Some(&value)).is_none());
        assert!(parse_start_page(Some(&Value::Null)).is_none());
        let page = StartPage::default();
        assert_eq!(page.shown_title(), DEFAULT_TITLE);
        assert_eq!(page.shown_message(), DEFAULT_MESSAGE);
    }

    #[test]
    fn request_json() {
        let body: Value =
            serde_json::from_str(&set_start_page_request("4", "Hello", "", Some(44))).unwrap();
        assert_eq!(body["@type"], "setBusinessStartPage");
        assert_eq!(body["start_page"]["@type"], "inputBusinessStartPage");
        assert_eq!(body["start_page"]["title"], "Hello");
        assert_eq!(body["start_page"]["sticker"]["id"], 44);
        let no_sticker: Value =
            serde_json::from_str(&set_start_page_request("4", "", "Hi", None)).unwrap();
        assert!(no_sticker["start_page"]["sticker"].is_null());
        let reset: Value =
            serde_json::from_str(&set_start_page_request("4", "", "", None)).unwrap();
        assert!(reset["start_page"].is_null());
    }
}
