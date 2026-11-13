//! Stickers, custom emoji, GIFs and reactions: the `stickers` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

/// The title of an emoji pack a message uses, for the message menu's
/// footer. `Asking` leaves on the answer or the error of its own request;
/// an ask whose request is gone is asked again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackTitle {
    /// `getStickerSet` is out as this request.
    Asking(RequestId),
    Known(String),
}

impl PackTitle {
    pub fn known(&self) -> Option<&str> {
        match self {
            Self::Known(title) => Some(title),
            Self::Asking(_) => None,
        }
    }
}

pub struct StickersState {
    /// Reaction options of every message a menu is open on.
    pub message_reaction_options: MessageReactionBook,
    /// `updateActiveEmojiReactions`: the emoji reactions Telegram offers (also
    /// the emoji usable as reactions in a group).
    pub active_reactions: Vec<String>,
    /// `updateDefaultReactionType`: the quick reaction (double-click).
    pub default_reaction: Option<ReactionChoice>,
    /// The app config's per-user reaction limits.
    pub reaction_limits: crate::reaction_limit::ReactionLimits,
    /// The sticker set the message menu's "View Sticker Set" opened.
    pub sticker_set_view: Option<StickerSetView>,
    /// The toasts after editing an owned set ("Sticker added.", the error),
    /// oldest first, whether or not the set box is open: two edits answered
    /// between polls both show.
    pub creator_notes: std::collections::VecDeque<String>,
    /// The packs of the custom emoji the user tapped in messages, by the
    /// request that asked.
    pub custom_emoji_answers: CustomEmojiAnswers,
    /// Titles of the emoji packs a message uses, by set id, for the menu's
    /// "This message contains emoji from X pack" footer.
    pub emoji_pack_titles: HashMap<i64, PackTitle>,
    /// Installed regular sticker sets + the loaded `stickerSet` for the picker.
    pub stickers: StickerPanel,
    pub emoji: EmojiPanel,
    /// Mask sticker sets: the manage dialog and the photo editor's strip.
    pub masks: MasksPanel,
    /// The stickers manager box: saved changes and thumbnail downloads.
    pub manager: StickersManager,
    /// Saved animations (`getSavedAnimations`) for the GIF picker.
    pub gifs: GifPanel,
    /// Animated emoji effects waiting for the window to play them.
    pub interactions: crate::emoji_interaction::EmojiInteractions,
    /// The interaction effects pack, and which read emoji played one.
    pub animations: crate::emoji_animations::EmojiAnimations,
    /// The set changes and reorders out (`manageStickerSet`, a reorder), by
    /// their own requests: one at a time.
    pub edits: crate::state::held::Holds<()>,
    /// Message effects: the available list and the ones looked up.
    pub effects: crate::message_effects::MessageEffects,
}

impl StickersState {
    pub(crate) fn new() -> Self {
        Self {
            edits: Default::default(),
            message_reaction_options: MessageReactionBook::default(),
            active_reactions: Vec::new(),
            default_reaction: None,
            reaction_limits: Default::default(),
            sticker_set_view: None,
            creator_notes: Default::default(),
            custom_emoji_answers: Default::default(),
            emoji_pack_titles: HashMap::new(),
            stickers: StickerPanel::default(),
            emoji: EmojiPanel::default(),
            masks: MasksPanel::default(),
            manager: StickersManager::default(),
            gifs: GifPanel::default(),
            interactions: Default::default(),
            animations: Default::default(),
            effects: Default::default(),
        }
    }
}
