//! The resolved custom emoji stickers, with a lookup by custom emoji id.
//!
//! The list is read as a slice everywhere (`Deref`), and changed only through
//! [`CustomEmojiStickers::update`], which rebuilds the id index after the
//! change, so the index cannot drift from the list.

use crate::telegram::envelope::StickerItem;
use std::collections::HashMap;
use std::ops::Deref;

#[derive(Debug, Clone, Default)]
pub struct CustomEmojiStickers {
    items: Vec<StickerItem>,
    /// Custom emoji id to position in `items` (the first item holding it).
    index: HashMap<i64, usize>,
}

impl CustomEmojiStickers {
    pub fn as_slice(&self) -> &[StickerItem] {
        &self.items
    }

    /// The sticker resolved for `custom_emoji_id`: what scanning the list
    /// for the first item with that id returns.
    pub fn lookup(&self, custom_emoji_id: i64) -> Option<&StickerItem> {
        self.index
            .get(&custom_emoji_id)
            .and_then(|&ix| self.items.get(ix))
    }

    pub fn push(&mut self, sticker: StickerItem) {
        self.update(|list| list.push(sticker));
    }

    /// Change the list, then rebuild the id index.
    pub fn update<R>(&mut self, change: impl FnOnce(&mut Vec<StickerItem>) -> R) -> R {
        let out = change(&mut self.items);
        self.index.clear();
        for (ix, sticker) in self.items.iter().enumerate() {
            if let Some(id) = sticker.custom_emoji_id {
                self.index.entry(id).or_insert(ix);
            }
        }
        out
    }
}

impl Deref for CustomEmojiStickers {
    type Target = [StickerItem];

    fn deref(&self) -> &[StickerItem] {
        &self.items
    }
}

impl<'a> IntoIterator for &'a CustomEmojiStickers {
    type Item = &'a StickerItem;
    type IntoIter = std::slice::Iter<'a, StickerItem>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl From<Vec<StickerItem>> for CustomEmojiStickers {
    fn from(items: Vec<StickerItem>) -> Self {
        let mut stickers = Self::default();
        stickers.update(|list| *list = items);
        stickers
    }
}

impl FromIterator<StickerItem> for CustomEmojiStickers {
    fn from_iter<I: IntoIterator<Item = StickerItem>>(iter: I) -> Self {
        iter.into_iter().collect::<Vec<_>>().into()
    }
}
