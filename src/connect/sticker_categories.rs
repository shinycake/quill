//! Connect driver: the sticker panel's category strip (tdesktop
//! `SearchWithGroups` over `Data::EmojiStatuses::stickerGroupsValue`).
use super::*;
use crate::state::RequestPurpose;
use crate::telegram::EmojiCategorySource;
use crate::telegram::requests::{get_premium_stickers, get_sticker_categories, search_stickers};

/// Stickers asked for when a category is chosen (the TDLib maximum).
const CATEGORY_LIMIT: i32 = 100;

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask for the category strip, once per session.
    pub fn fetch_sticker_categories(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.stickers.emoji.sticker_categories_asked {
            return Ok(());
        }
        self.session.stickers.emoji.sticker_categories_asked = true;
        let sent = self.sticker_request(RequestPurpose::GetStickerCategories, |id| {
            get_sticker_categories(id)
        });
        if sent.is_err() {
            self.session.stickers.emoji.sticker_categories_asked = false;
        }
        sent.map(|_| ())
    }

    /// Show the stickers of a category as the panel's found stickers:
    /// `searchStickers` for the category's emoji, `getPremiumStickers` for
    /// the Premium category.
    pub fn search_sticker_category(
        &mut self,
        source: &EmojiCategorySource,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let emojis = match source {
            EmojiCategorySource::Search(emojis) if !emojis.is_empty() => emojis.join(" "),
            EmojiCategorySource::Premium => String::new(),
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        // A chosen category replaces whatever the typed search found.
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::SearchStickers),
        );
        let stickers = &mut self.session.stickers.stickers;
        stickers.search_offset = 0;
        stickers.search_has_more = false;
        stickers.search_query.clear();
        stickers.found_stickers.clear();
        self.sticker_request(RequestPurpose::SearchStickers, |id| {
            if matches!(source, EmojiCategorySource::Premium) {
                get_premium_stickers(id, CATEGORY_LIMIT)
            } else {
                search_stickers(id, &emojis, "", 0, CATEGORY_LIMIT)
            }
        })
        .map(|_| ())
    }
}
