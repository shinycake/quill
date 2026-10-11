//! Session side of the "Your color" editor: the icon choices.
use super::*;

impl Session {
    /// `getDefaultBackgroundCustomEmojiStickers` answered: keep the custom
    /// emoji stickers the editor offers as reply and profile icons.
    pub(crate) fn accept_background_emoji_icons(&mut self, stickers: Vec<StickerItem>) {
        self.users_state.background_emoji_icons = stickers
            .into_iter()
            .filter(|sticker| sticker.custom_emoji_id.is_some_and(|id| id > 0))
            .collect();
        self.users_state.background_emoji_icons_failed = false;
    }
}
