//! Stickers, custom emoji, GIFs and reactions: the `stickers` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct StickersState {
    /// Reaction options for the message whose reaction picker is open.
    pub message_reaction_options: Option<MessageReactionOptions>,
    /// `updateActiveEmojiReactions`: the emoji reactions Telegram offers.
    pub active_reactions: Vec<String>,
    /// `updateDefaultReactionType`: the quick reaction (double-click).
    pub default_reaction: Option<ReactionChoice>,
    /// The reaction options of the open picker are out of date.
    pub reaction_options_stale: bool,
    /// The sticker set the message menu's "View Sticker Set" opened.
    pub sticker_set_view: Option<StickerSetView>,
    /// The pack of the custom emoji the user just tapped in a message.
    pub custom_emoji_preview: Option<CustomEmojiPreview>,
    /// Titles of the emoji packs a message uses, by set id, for the menu's
    /// "This message contains emoji from X pack" footer.
    pub emoji_pack_titles: HashMap<i64, String>,
    /// Installed regular sticker sets + the loaded `stickerSet` for the picker.
    pub stickers: StickerPanel,
    pub emoji: EmojiPanel,
    /// Saved animations (`getSavedAnimations`) for the GIF picker.
    pub gifs: GifPanel,
    /// B7: `updateActiveEmojiReactions` — the emoji usable as reactions.
    pub active_emoji_reactions: Vec<String>,
}

impl StickersState {
    pub(crate) fn new() -> Self {
        Self {
            message_reaction_options: None,
            active_reactions: Vec::new(),
            default_reaction: None,
            reaction_options_stale: false,
            sticker_set_view: None,
            custom_emoji_preview: None,
            emoji_pack_titles: HashMap::new(),
            stickers: StickerPanel::default(),
            emoji: EmojiPanel::default(),
            gifs: GifPanel::default(),
            active_emoji_reactions: Vec::new(),
        }
    }
}
