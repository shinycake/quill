//! Connect driver: the sticker panel's category strip (tdesktop
//! `SearchWithGroups` over `Data::EmojiStatuses::stickerGroupsValue`).
use super::*;
use crate::state::{RequestPurpose, StickerSearch};
use crate::telegram::EmojiCategorySource;
use crate::telegram::requests::{get_premium_stickers, get_sticker_categories, search_stickers};

/// Stickers asked for when a category is chosen (the TDLib maximum).
const CATEGORY_LIMIT: i32 = 100;

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask for the category strip while nothing was asked yet. A failed
    /// ask stays `Failed` until the panel opens again (`reopen_sticker_categories`),
    /// so a render never re-asks.
    pub fn fetch_sticker_categories(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.stickers.emoji.sticker_categories.is_idle() {
            return Ok(());
        }
        let sent = self.sticker_request(RequestPurpose::GetStickerCategories, |id| {
            get_sticker_categories(id)
        });
        match sent {
            Ok(_) => {
                self.session.stickers.emoji.sticker_categories.begin();
                Ok(())
            }
            Err(err) => Err(err),
        }
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
        stickers.searched = StickerSearch::Category(source.clone());
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
