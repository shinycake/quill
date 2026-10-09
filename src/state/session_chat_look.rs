//! Per-chat wallpaper and emoji chat theme: the reducer side
//! (`chat.background`, `chat.theme`, `updateEmojiChatThemes`).
use super::*;
use crate::telegram::envelope::{ChatBackground, EmojiChatTheme, ThemeSettings};

impl Session {
    /// `chat.background` / `updateChatBackground`; `None` clears it.
    pub fn set_chat_background(&mut self, chat_id: i64, background: Option<ChatBackground>) {
        match background {
            Some(background) => {
                if let Some(file) = &background.background.file {
                    self.upsert_file(file.clone(), false);
                }
                self.chat_backgrounds.insert(chat_id, background);
            }
            None => {
                self.chat_backgrounds.remove(&chat_id);
            }
        }
    }

    /// `chat.theme` / `updateChatTheme`; `None` clears it.
    pub fn set_chat_theme_name(&mut self, chat_id: i64, name: Option<String>) {
        match name {
            Some(name) => {
                self.chat_theme_names.insert(chat_id, name);
            }
            None => {
                self.chat_theme_names.remove(&chat_id);
            }
        }
    }

    /// The emoji theme a chat uses, once `updateEmojiChatThemes` has
    /// delivered its description.
    pub fn chat_emoji_theme(&self, chat_id: i64) -> Option<&EmojiChatTheme> {
        let name = self.chat_theme_names.get(&chat_id)?;
        self.emoji_chat_themes.iter().find(|t| &t.name == name)
    }

    /// The theme's settings for the light or dark interface.
    pub fn chat_theme_settings(&self, chat_id: i64, dark: bool) -> Option<&ThemeSettings> {
        self.chat_emoji_theme(chat_id).map(|t| t.settings(dark))
    }

    /// The wallpaper a chat shows and its dark-mode dimming: its own
    /// wallpaper first, else the theme's (tdesktop keeps the peer's wallpaper
    /// over the theme's background).
    pub fn chat_wallpaper(&self, chat_id: i64, dark: bool) -> Option<(&Background, i32)> {
        if let Some(own) = self.chat_backgrounds.get(&chat_id) {
            return Some((&own.background, own.dark_theme_dimming));
        }
        self.chat_theme_settings(chat_id, dark)
            .and_then(|s| s.background.as_ref())
            .map(|b| (b, 0))
    }
}
