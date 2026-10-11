//! Per-chat wallpapers and emoji chat themes: `chatBackground`
//! (`schema/td_api.tl:745`), `chat.theme` / `ChatTheme` (`:8835`),
//! `themeSettings` (`:4331`), `emojiChatTheme` (`:8821`) and the updates
//! `updateChatBackground`, `updateChatTheme`, `updateEmojiChatThemes`.

use super::backgrounds::{Background, BackgroundFill, parse_background, parse_background_fill};
use serde_json::Value;

/// `chatBackground`: the wallpaper of one chat, with the dimming Telegram
/// applies in dark mode (0-100).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatBackground {
    pub background: Background,
    pub dark_theme_dimming: i32,
}

/// `themeSettings`, the light or dark half of an emoji chat theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeSettings {
    /// 0xRRGGBB (the alpha byte of the ARGB value is dropped).
    pub accent_color: u32,
    pub background: Option<Background>,
    /// The fill behind outgoing bubbles; `None` uses the base theme's.
    pub outgoing_fill: Option<BackgroundFill>,
    pub outgoing_accent_color: Option<u32>,
}

impl ThemeSettings {
    /// The color outgoing bubbles use: the theme's fill (averaged when it is
    /// a gradient), else its outgoing accent, else its accent.
    pub fn outgoing_bubble_color(&self) -> u32 {
        self.outgoing_fill
            .as_ref()
            .map(BackgroundFill::average)
            .or(self.outgoing_accent_color)
            .unwrap_or(self.accent_color)
    }
}

/// `emojiChatTheme`: a named theme with a light and a dark variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmojiChatTheme {
    pub name: String,
    pub light: ThemeSettings,
    pub dark: ThemeSettings,
}

impl EmojiChatTheme {
    pub fn settings(&self, dark: bool) -> &ThemeSettings {
        if dark { &self.dark } else { &self.light }
    }
}

fn rgb(value: Option<&Value>) -> u32 {
    value.and_then(Value::as_i64).unwrap_or(0) as u32 & 0x00ff_ffff
}

pub(crate) fn parse_chat_background(value: Option<&Value>) -> Option<ChatBackground> {
    let value = value.filter(|v| !v.is_null())?;
    Some(ChatBackground {
        background: parse_background(value.get("background")?)?,
        dark_theme_dimming: value
            .get("dark_theme_dimming")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, 100) as i32,
    })
}

/// `ChatTheme` (`schema/td_api.tl:8838`): an emoji theme by name (its
/// settings come from `updateEmojiChatThemes`), or a theme made from an
/// upgraded gift, which carries its own settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatThemeRef {
    Emoji(String),
    Gift(Box<EmojiChatTheme>),
}

/// `chat.theme` / `updateChatTheme.theme`; `None` for no theme.
pub(crate) fn parse_chat_theme(value: Option<&Value>) -> Option<ChatThemeRef> {
    let value = value.filter(|v| !v.is_null())?;
    match value.get("@type")?.as_str()? {
        "chatThemeEmoji" => value
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
            .map(|n| ChatThemeRef::Emoji(n.to_string())),
        "chatThemeGift" => {
            let theme = value.get("gift_theme")?;
            let gift = theme.get("gift");
            let name = gift
                .and_then(|g| g.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some(ChatThemeRef::Gift(Box::new(EmojiChatTheme {
                name,
                light: parse_theme_settings(theme.get("light_settings"))?,
                dark: parse_theme_settings(theme.get("dark_settings"))?,
            })))
        }
        _ => None,
    }
}

fn parse_theme_settings(value: Option<&Value>) -> Option<ThemeSettings> {
    let value = value.filter(|v| !v.is_null())?;
    let has_outgoing_accent = value
        .get("has_outgoing_message_accent_color")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(ThemeSettings {
        accent_color: rgb(value.get("accent_color")),
        background: value.get("background").and_then(parse_background),
        outgoing_fill: parse_background_fill(value.get("outgoing_message_fill")),
        outgoing_accent_color: has_outgoing_accent
            .then(|| rgb(value.get("outgoing_message_accent_color"))),
    })
}

pub(crate) fn parse_emoji_chat_themes(value: &Value) -> Vec<EmojiChatTheme> {
    value
        .get("chat_themes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    Some(EmojiChatTheme {
                        name: t.get("name").and_then(Value::as_str)?.to_string(),
                        light: parse_theme_settings(t.get("light_settings"))?,
                        dark: parse_theme_settings(t.get("dark_settings"))?,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_background_carries_dimming() {
        let bg = parse_chat_background(Some(&json!({
            "@type":"chatBackground","dark_theme_dimming":140,
            "background":{"@type":"background","id":9,"name":"n",
                "type":{"@type":"backgroundTypeFill","fill":{"@type":"backgroundFillSolid","color":255}}}
        })))
        .unwrap();
        assert_eq!(bg.background.id, 9);
        assert_eq!(bg.dark_theme_dimming, 100);
        assert!(parse_chat_background(Some(&json!(null))).is_none());
    }

    #[test]
    fn emoji_and_gift_themes_parse() {
        assert_eq!(
            parse_chat_theme(Some(&json!({"@type":"chatThemeEmoji","name":"🏠"}))),
            Some(ChatThemeRef::Emoji("🏠".into()))
        );
        assert_eq!(
            parse_chat_theme(Some(&json!({"@type":"chatThemeGift","gift_theme":{}}))),
            None,
            "a gift theme without settings"
        );
        let settings = json!({"@type":"themeSettings","accent_color":0x22aa33,
            "outgoing_message_fill":{"@type":"backgroundFillSolid","color":0x445566}});
        let gift = parse_chat_theme(Some(&json!({"@type":"chatThemeGift","gift_theme":{
            "@type":"giftChatTheme","gift":{"@type":"upgradedGift","name":"PlushPepe-1"},
            "light_settings":settings,"dark_settings":settings}})));
        match gift {
            Some(ChatThemeRef::Gift(theme)) => {
                assert_eq!(theme.name, "PlushPepe-1");
                assert_eq!(theme.light.accent_color, 0x22aa33);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(parse_chat_theme(None), None);
    }

    #[test]
    fn emoji_themes_parse_both_variants() {
        let settings = json!({"@type":"themeSettings","accent_color":0xff3366cc_u32 as i64,
            "outgoing_message_fill":{"@type":"backgroundFillSolid","color":0x112233},
            "has_outgoing_message_accent_color":false});
        let themes = parse_emoji_chat_themes(&json!({"chat_themes":[
            {"@type":"emojiChatTheme","name":"🏠","light_settings":settings,"dark_settings":settings},
            {"@type":"emojiChatTheme","name":"broken"}
        ]}));
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].light.accent_color, 0x3366cc);
        assert_eq!(themes[0].dark.outgoing_bubble_color(), 0x112233);
    }
}
