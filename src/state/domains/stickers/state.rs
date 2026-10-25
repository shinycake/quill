//! Stickers, custom emoji, GIFs and reactions: the `stickers` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct StickersState {
    /// Reaction options for the message whose reaction picker is open.
    pub message_reaction_options: MessageReactionsState,
    /// `updateActiveEmojiReactions`: the emoji reactions Telegram offers.
    pub active_reactions: Vec<String>,
    /// `updateDefaultReactionType`: the quick reaction (double-click).
    pub default_reaction: Option<ReactionChoice>,
    /// The reaction options of the open picker are out of date.
    pub reaction_options_stale: bool,
    /// The app config's per-user reaction limits.
    pub reaction_limits: crate::reaction_limit::ReactionLimits,
    /// The sticker set the message menu's "View Sticker Set" opened.
    pub sticker_set_view: Option<StickerSetView>,
    /// A one-shot toast after editing an owned set ("Sticker added.",
    /// the error), whether or not the set box is open.
    pub creator_note: Option<String>,
    /// An edit of an owned set changed a set's title, contents or
    /// existence: the driver fetches both installed lists again so menus
    /// and the panel show the server's sets.
    pub creator_refetch_sets: bool,
    /// The pack of the custom emoji the user just tapped in a message.
    pub custom_emoji_preview: Option<CustomEmojiPreview>,
    /// Titles of the emoji packs a message uses, by set id, for the menu's
    /// "This message contains emoji from X pack" footer.
    pub emoji_pack_titles: HashMap<i64, String>,
    /// Installed regular sticker sets + the loaded `stickerSet` for the picker.
    pub stickers: StickerPanel,
    pub emoji: EmojiPanel,
    /// Mask sticker sets: the manage dialog and the photo editor's strip.
    pub masks: MasksPanel,
    /// The stickers manager box: saved changes and thumbnail downloads.
    pub manager: StickersManager,
    /// Saved animations (`getSavedAnimations`) for the GIF picker.
    pub gifs: GifPanel,
    /// B7: `updateActiveEmojiReactions` — the emoji usable as reactions.
    pub active_emoji_reactions: Vec<String>,
    /// Animated emoji effects waiting for the window to play them.
    pub interactions: crate::emoji_interaction::EmojiInteractions,
    /// Message effects: the available list and the ones looked up.
    pub effects: crate::message_effects::MessageEffects,
}

impl StickersState {
    pub(crate) fn new() -> Self {
        Self {
            message_reaction_options: MessageReactionsState::Idle,
            active_reactions: Vec::new(),
            default_reaction: None,
            reaction_options_stale: false,
            reaction_limits: Default::default(),
            sticker_set_view: None,
            creator_note: None,
            creator_refetch_sets: false,
            custom_emoji_preview: None,
            emoji_pack_titles: HashMap::new(),
            stickers: StickerPanel::default(),
            emoji: EmojiPanel::default(),
            masks: MasksPanel::default(),
            manager: StickersManager::default(),
            gifs: GifPanel::default(),
            active_emoji_reactions: Vec::new(),
            interactions: Default::default(),
            effects: Default::default(),
        }
    }
}
