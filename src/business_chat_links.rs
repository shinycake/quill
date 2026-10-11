//! Telegram Business links to chat: short links that open a chat with the
//! account and put a preset message in the field.
//!
//! tdesktop `settings/business/settings_chat_links.cpp`: "Create a Link to
//! Chat" (hidden at `business_chat_links_limit`, 100), a list of links
//! (title, or the link without `https://`; the message; "N clicks" / "no
//! clicks"); a click opens a menu with Copy, Share, Get QR Code, Edit and
//! Delete (confirmed). The edit box has the message and an optional name.
//! The footer offers the plain `t.me/username` and `t.me/+phone` links.

use serde_json::{Value, json};

/// tdesktop's `business_chat_links_limit` default; TDLib reports the
/// limit as the option `business_chat_link_count_max`.
pub const LINKS_MAX: usize = 100;

/// `businessChatLink`. The message keeps its formatting as composer
/// markup (`**bold**`, ...), the form Quill's text fields edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLink {
    pub link: String,
    /// The message without formatting, for the list.
    pub text: String,
    /// The message as composer markup, for the editor.
    pub markup: String,
    pub title: String,
    pub view_count: i32,
}

impl ChatLink {
    /// tdesktop `Row::generateName`: the title, or the link without
    /// `https://`.
    pub fn name(&self) -> String {
        if self.title.is_empty() {
            self.link
                .strip_prefix("https://")
                .unwrap_or(&self.link)
                .to_string()
        } else {
            self.title.clone()
        }
    }

    /// tdesktop `ComputeClicks`.
    pub fn clicks_label(&self) -> String {
        match self.view_count {
            n if n <= 0 => "no clicks".to_string(),
            1 => "1 click".to_string(),
            n => format!("{n} clicks"),
        }
    }
}

fn formatted_text(value: Option<&Value>) -> (String, String) {
    let text = value
        .and_then(|v| v.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let entities = crate::telegram::envelope::parse_text_entities(&text, value);
    let markup = crate::composer::entities_to_markup(&text, &entities);
    (text, markup)
}

/// `businessChatLink`.
pub fn parse_link(value: &Value) -> Option<ChatLink> {
    let (text, markup) = formatted_text(value.get("text"));
    Some(ChatLink {
        link: value.get("link")?.as_str()?.to_string(),
        text,
        markup,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        view_count: value
            .get("view_count")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(0),
    })
}

/// `businessChatLinks`.
pub fn parse_links(value: &Value) -> Vec<ChatLink> {
    value
        .get("links")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(parse_link).collect())
        .unwrap_or_default()
}

/// `inputBusinessChatLink` from the editor's markup and name.
fn input_link(markup: &str, title: &str) -> Value {
    let (text, entities) = crate::composer::parse_format_markup(markup.trim());
    let entities: Vec<Value> = entities
        .iter()
        .map(crate::telegram::requests::format_entity_json)
        .collect();
    json!({
        "@type": "inputBusinessChatLink",
        "text": {"@type": "formattedText", "text": text, "entities": entities},
        "title": title.trim(),
    })
}

pub fn get_links_request(extra: &str) -> String {
    json!({"@type": "getBusinessChatLinks", "@extra": extra}).to_string()
}

pub fn create_link_request(extra: &str, markup: &str, title: &str) -> String {
    json!({
        "@type": "createBusinessChatLink",
        "@extra": extra,
        "link_info": input_link(markup, title),
    })
    .to_string()
}

pub fn edit_link_request(extra: &str, link: &str, markup: &str, title: &str) -> String {
    json!({
        "@type": "editBusinessChatLink",
        "@extra": extra,
        "link": link,
        "link_info": input_link(markup, title),
    })
    .to_string()
}

pub fn delete_link_request(extra: &str, link: &str) -> String {
    json!({"@type": "deleteBusinessChatLink", "@extra": extra, "link": link}).to_string()
}

/// The footer's plain links to the account: `t.me/username` (when there
/// is one) and `t.me/+phone`.
pub fn plain_links(username: &str, phone: &str) -> Vec<String> {
    let mut out = Vec::new();
    if !username.is_empty() {
        out.push(format!("t.me/{username}"));
    }
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    if !digits.is_empty() {
        out.push(format!("t.me/+{digits}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_links_and_names_them() {
        let value: Value = serde_json::from_str(r#"{"@type":"businessChatLinks","links":[{"@type":"businessChatLink","link":"https://t.me/m/AbCd","text":{"@type":"formattedText","text":"Hello!","entities":[]},"title":"","view_count":0},{"@type":"businessChatLink","link":"https://t.me/m/Ef","text":{"@type":"formattedText","text":"Price?","entities":[]},"title":"Ads","view_count":3}]}"#).unwrap();
        let links = parse_links(&value);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].name(), "t.me/m/AbCd");
        assert_eq!(links[0].clicks_label(), "no clicks");
        assert_eq!(links[1].name(), "Ads");
        assert_eq!(links[1].text, "Price?");
        assert_eq!(links[1].markup, "Price?");
        assert_eq!(links[1].clicks_label(), "3 clicks");
    }

    #[test]
    fn request_json() {
        let create: Value =
            serde_json::from_str(&create_link_request("1", "**Hi** there", " Promo ")).unwrap();
        assert_eq!(create["@type"], "createBusinessChatLink");
        assert_eq!(create["link_info"]["text"]["text"], "Hi there");
        assert_eq!(
            create["link_info"]["text"]["entities"][0]["type"]["@type"],
            "textEntityTypeBold"
        );
        assert_eq!(create["link_info"]["title"], "Promo");
        let edit: Value =
            serde_json::from_str(&edit_link_request("2", "https://t.me/m/x", "Yo", "")).unwrap();
        assert_eq!(edit["@type"], "editBusinessChatLink");
        assert_eq!(edit["link"], "https://t.me/m/x");
        let delete: Value =
            serde_json::from_str(&delete_link_request("3", "https://t.me/m/x")).unwrap();
        assert_eq!(delete["@type"], "deleteBusinessChatLink");
        let get: Value = serde_json::from_str(&get_links_request("4")).unwrap();
        assert_eq!(get["@type"], "getBusinessChatLinks");
    }

    #[test]
    fn footer_links() {
        assert_eq!(
            plain_links("shop", "+1 555 0100"),
            vec!["t.me/shop", "t.me/+15550100"]
        );
        assert_eq!(plain_links("", "15550100"), vec!["t.me/+15550100"]);
    }
}
