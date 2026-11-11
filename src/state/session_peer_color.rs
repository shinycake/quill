//! Session side of the "Your color" editor: the icon choices.
use super::*;

fn custom_emoji_only(stickers: Vec<StickerItem>) -> Vec<StickerItem> {
    stickers
        .into_iter()
        .filter(|sticker| sticker.custom_emoji_id.is_some_and(|id| id > 0))
        .collect()
}

impl Session {
    /// `getDefaultProfilePhotoCustomEmojiStickers` answered: keep the custom
    /// emoji the avatar builder offers.
    pub(crate) fn accept_profile_photo_emoji_icons(&mut self, stickers: Vec<StickerItem>) {
        self.users_state
            .profile_photo_emoji_icons
            .succeed(custom_emoji_only(stickers));
    }

    /// `getDefaultChatPhotoCustomEmojiStickers` answered: the same, for a
    /// group or channel photo.
    pub(crate) fn accept_chat_photo_emoji_icons(&mut self, stickers: Vec<StickerItem>) {
        self.users_state
            .chat_photo_emoji_icons
            .succeed(custom_emoji_only(stickers));
    }

    /// `getDefaultBackgroundCustomEmojiStickers` answered: keep the custom
    /// emoji stickers the editor offers as reply and profile icons.
    pub(crate) fn accept_background_emoji_icons(&mut self, stickers: Vec<StickerItem>) {
        self.users_state
            .background_emoji_icons
            .succeed(custom_emoji_only(stickers));
    }
}
