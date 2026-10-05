//! The composer's emoji/sticker panel library: the contents of every
//! installed sticker and custom-emoji set (loaded lazily as each section
//! comes into view), plus the reaction options of the message whose
//! reaction picker is open.
use super::*;
use crate::telegram::envelope::{StoryAvailableReactionKind, StoryAvailableReactionView};

/// At most this many `getStickerSet` requests for the library in flight.
pub const MAX_LIBRARY_LOADS: usize = 4;

#[derive(Debug, Clone, Default)]
pub struct MediaLibrary {
    /// Loaded set contents, keyed by set id.
    pub set_stickers: HashMap<i64, Vec<StickerItem>>,
    pub loading: HashSet<i64>,
    /// Sets whose load failed this session (not retried automatically).
    pub failed: HashSet<i64>,
}

/// One reaction the picker can offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactionChoice {
    Emoji(String),
    CustomEmoji(i64),
}

impl ReactionChoice {
    fn from_view(view: StoryAvailableReactionView) -> Option<Self> {
        match view.kind {
            StoryAvailableReactionKind::Emoji(emoji) => Some(Self::Emoji(emoji)),
            StoryAvailableReactionKind::CustomEmoji(id) => Some(Self::CustomEmoji(id)),
            // Paid (star) reactions are a separate flow.
            StoryAvailableReactionKind::Paid => None,
        }
    }
}

/// `getMessageAvailableReactions` for one message: what its picker shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReactionOptions {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    /// The quick strip (TDLib orders them; `row_size` sized the first row).
    pub top: Vec<ReactionChoice>,
    pub recent: Vec<ReactionChoice>,
    pub popular: Vec<ReactionChoice>,
    /// Custom emoji from the user's packs may be used too.
    pub allow_custom_emoji: bool,
}

impl MessageReactionOptions {
    /// Every offered reaction, deduplicated: top, then recent, then popular.
    pub fn all(&self) -> Vec<ReactionChoice> {
        let mut out: Vec<ReactionChoice> = Vec::new();
        for choice in self.top.iter().chain(&self.recent).chain(&self.popular) {
            if !out.contains(choice) {
                out.push(choice.clone());
            }
        }
        out
    }
}

impl Session {
    pub(crate) fn accept_library_set(&mut self, set_id: i64, stickers: Vec<StickerItem>) {
        self.media_library.loading.remove(&set_id);
        self.media_library.failed.remove(&set_id);
        self.media_library.set_stickers.insert(set_id, stickers);
    }

    pub(crate) fn fail_library_set(&mut self, set_id: i64) {
        self.media_library.loading.remove(&set_id);
        self.media_library.failed.insert(set_id);
    }

    /// Installed sets (regular, then custom emoji) whose contents aren't
    /// loaded, loading or failed, among `wanted`, capped by the free
    /// in-flight slots.
    pub fn library_sets_to_load(&self, wanted: &[i64]) -> Vec<i64> {
        let free = MAX_LIBRARY_LOADS.saturating_sub(self.media_library.loading.len());
        let mut out = Vec::new();
        for id in wanted {
            if out.len() >= free {
                break;
            }
            if *id != 0
                && !self.media_library.set_stickers.contains_key(id)
                && !self.media_library.loading.contains(id)
                && !self.media_library.failed.contains(id)
                && !out.contains(id)
            {
                out.push(*id);
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn accept_message_reaction_options(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        top: Vec<StoryAvailableReactionView>,
        recent: Vec<StoryAvailableReactionView>,
        popular: Vec<StoryAvailableReactionView>,
        allow_custom_emoji: bool,
    ) {
        let convert = |views: Vec<StoryAvailableReactionView>| -> Vec<ReactionChoice> {
            views
                .into_iter()
                .filter_map(ReactionChoice::from_view)
                .collect()
        };
        self.message_reaction_options = Some(MessageReactionOptions {
            chat_id,
            message_id,
            top: convert(top),
            recent: convert(recent),
            popular: convert(popular),
            allow_custom_emoji,
        });
    }
}
