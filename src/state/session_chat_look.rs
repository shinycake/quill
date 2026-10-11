//! Per-chat wallpaper and emoji chat theme: the reducer side
//! (`chat.background`, `chat.theme`, `updateEmojiChatThemes`).
use super::*;
use crate::telegram::envelope::{ChatBackground, ChatThemeRef, EmojiChatTheme, ThemeSettings};

impl Session {
    /// Track a background's file and thumbnail so downloads update them.
    pub fn upsert_background_files(&mut self, background: &Background) {
        for file in [&background.file, &background.thumbnail]
            .into_iter()
            .flatten()
        {
            self.upsert_file(file.clone(), false);
        }
    }

    /// `chat.background` / `updateChatBackground`; `None` clears it.
    pub fn set_chat_background(&mut self, chat_id: i64, background: Option<ChatBackground>) {
        match background {
            Some(background) => {
                self.upsert_background_files(&background.background);
                self.chats_state
                    .chat_backgrounds
                    .insert(chat_id, background);
            }
            None => {
                self.chats_state.chat_backgrounds.remove(&chat_id);
            }
        }
    }

    /// `chat.theme` / `updateChatTheme`; `None` clears it.
    pub fn set_chat_theme_name(&mut self, chat_id: i64, name: Option<String>) {
        match name {
            Some(name) => {
                self.chats_state.chat_theme_names.insert(chat_id, name);
            }
            None => {
                self.chats_state.chat_theme_names.remove(&chat_id);
            }
        }
    }

    /// `chat.theme` / `updateChatTheme`: an emoji theme by name, or a gift
    /// theme with its own settings; `None` clears it.
    pub fn set_chat_theme(&mut self, chat_id: i64, theme: Option<ChatThemeRef>) {
        self.chats_state.chat_gift_themes.remove(&chat_id);
        match theme {
            Some(ChatThemeRef::Emoji(name)) => self.set_chat_theme_name(chat_id, Some(name)),
            Some(ChatThemeRef::Gift(theme)) => {
                self.set_chat_theme_name(chat_id, None);
                for settings in [&theme.light, &theme.dark] {
                    if let Some(background) = &settings.background {
                        self.upsert_background_files(background);
                    }
                }
                self.chats_state.chat_gift_themes.insert(chat_id, *theme);
            }
            None => self.set_chat_theme_name(chat_id, None),
        }
    }

    /// The theme a chat uses: its gift theme, or its emoji theme once
    /// `updateEmojiChatThemes` has delivered the description.
    pub fn chat_emoji_theme(&self, chat_id: i64) -> Option<&EmojiChatTheme> {
        if let Some(gift) = self.chats_state.chat_gift_themes.get(&chat_id) {
            return Some(gift);
        }
        let name = self.chats_state.chat_theme_names.get(&chat_id)?;
        self.chats_state
            .emoji_chat_themes
            .iter()
            .find(|t| &t.name == name)
    }

    /// The theme's settings for the light or dark interface.
    pub fn chat_theme_settings(&self, chat_id: i64, dark: bool) -> Option<&ThemeSettings> {
        self.chat_emoji_theme(chat_id).map(|t| t.settings(dark))
    }

    /// The wallpaper a chat shows and its dark-mode dimming: its own
    /// wallpaper first, else the theme's (tdesktop keeps the peer's wallpaper
    /// over the theme's background).
    pub fn chat_wallpaper(&self, chat_id: i64, dark: bool) -> Option<(&Background, i32)> {
        if let Some(own) = self.chats_state.chat_backgrounds.get(&chat_id) {
            return Some((&own.background, own.dark_theme_dimming));
        }
        self.chat_theme_settings(chat_id, dark)
            .and_then(|s| s.background.as_ref())
            .map(|b| (b, 0))
    }
}
