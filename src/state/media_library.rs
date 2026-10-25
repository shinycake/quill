//! The composer's emoji/sticker panel library: the contents of every
//! installed sticker and custom-emoji set (loaded lazily as each section
//! comes into view), plus the reaction options of the message whose
//! reaction picker is open.
use super::*;
use crate::telegram::envelope::{
    ReactionType, StoryAvailableReactionKind, StoryAvailableReactionView,
};

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
    /// The choice a reaction under a message stands for (paid and unknown
    /// reactions have none).
    pub fn from_type(reaction_type: &ReactionType) -> Option<Self> {
        match reaction_type {
            ReactionType::Emoji { emoji } => Some(Self::Emoji(emoji.clone())),
            ReactionType::CustomEmoji { custom_emoji_id } => {
                Some(Self::CustomEmoji(*custom_emoji_id))
            }
            ReactionType::Paid | ReactionType::Unknown => None,
        }
    }

    pub fn reaction_type(&self) -> ReactionType {
        match self {
            Self::Emoji(emoji) => ReactionType::emoji(emoji.clone()),
            Self::CustomEmoji(id) => ReactionType::CustomEmoji {
                custom_emoji_id: *id,
            },
        }
    }

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
    /// Offered reactions TDLib marks `needs_premium` for this account.
    pub premium_locked: Vec<ReactionChoice>,
}

/// The Premium feature a reaction tap shows instead of reacting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactPremiumFeature {
    /// tdesktop `PremiumFeature::InfiniteReactions`.
    UniqueReactions,
    /// tdesktop `PremiumFeature::TagsForMessages`.
    SavedMessagesTags,
}

/// Where the open message menu's reaction options stand. tdesktop reads
/// them from its local cache and skips the strip when there are none
/// (`AttachSelectorToMenu`, `reactions.recent.empty()`); Quill asks TDLib, so
/// the wait and a failed or empty answer are states of their own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MessageReactionsState {
    /// Nothing was asked for.
    #[default]
    Idle,
    /// `getMessageAvailableReactions` is in flight for this message.
    Loading {
        chat_id: ChatId,
        message_id: MessageId,
    },
    Ready(MessageReactionOptions),
    /// The request failed or the answer held no reaction: no strip.
    Unavailable {
        chat_id: ChatId,
        message_id: MessageId,
    },
}

impl MessageReactionsState {
    /// The options, whichever message they belong to.
    pub fn ready(&self) -> Option<&MessageReactionOptions> {
        match self {
            Self::Ready(options) => Some(options),
            _ => None,
        }
    }

    /// The options when they belong to this message.
    pub fn ready_for(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<&MessageReactionOptions> {
        self.ready()
            .filter(|o| o.chat_id == chat_id && o.message_id == message_id)
    }

    pub fn is_loading_for(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        matches!(self, Self::Loading { chat_id: c, message_id: m } if *c == chat_id && *m == message_id)
    }

    pub fn is_unavailable_for(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        matches!(self, Self::Unavailable { chat_id: c, message_id: m } if *c == chat_id && *m == message_id)
    }

    /// The message the state is about, if any.
    pub fn target(&self) -> Option<(ChatId, MessageId)> {
        match self {
            Self::Idle => None,
            Self::Loading {
                chat_id,
                message_id,
            }
            | Self::Unavailable {
                chat_id,
                message_id,
            } => Some((*chat_id, *message_id)),
            Self::Ready(o) => Some((o.chat_id, o.message_id)),
        }
    }

    /// The state already holds the answer for this message. A wait does not
    /// count (its request may be gone), nor does a failed or empty answer:
    /// reopening the menu asks again unless a request is really pending.
    pub fn covers(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.ready_for(chat_id, message_id).is_some()
    }
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
        self.media.media_library.loading.remove(&set_id);
        self.media.media_library.failed.remove(&set_id);
        self.media
            .media_library
            .set_stickers
            .insert(set_id, stickers);
    }

    pub(crate) fn fail_library_set(&mut self, set_id: i64) {
        self.media.media_library.loading.remove(&set_id);
        self.media.media_library.failed.insert(set_id);
    }

    /// Installed sets (regular, then custom emoji) whose contents aren't
    /// loaded, loading or failed, among `wanted`, capped by the free
    /// in-flight slots.
    pub fn library_sets_to_load(&self, wanted: &[i64]) -> Vec<i64> {
        let free = MAX_LIBRARY_LOADS.saturating_sub(self.media.media_library.loading.len());
        let mut out = Vec::new();
        for id in wanted {
            if out.len() >= free {
                break;
            }
            if *id != 0
                && !self.media.media_library.set_stickers.contains_key(id)
                && !self.media.media_library.loading.contains(id)
                && !self.media.media_library.failed.contains(id)
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
        // Only the answer this menu waits for counts; a late one for a
        // message the menu left must not touch the current state.
        if !self
            .stickers
            .message_reaction_options
            .is_loading_for(chat_id, message_id)
        {
            return;
        }
        // The Star reaction is its own flow (`paid_reaction`): note whether
        // this message's picker offers it.
        let paid_offered = top
            .iter()
            .chain(&recent)
            .chain(&popular)
            .any(|view| matches!(view.kind, StoryAvailableReactionKind::Paid));
        self.payments.paid.offered = paid_offered.then_some((chat_id.0, message_id.0));
        let premium_locked: Vec<ReactionChoice> = top
            .iter()
            .chain(&recent)
            .chain(&popular)
            .filter(|view| view.needs_premium)
            .filter_map(|view| ReactionChoice::from_view(view.clone()))
            .collect();
        let convert = |views: Vec<StoryAvailableReactionView>| -> Vec<ReactionChoice> {
            views
                .into_iter()
                .filter_map(ReactionChoice::from_view)
                .collect()
        };
        let options = MessageReactionOptions {
            chat_id,
            message_id,
            top: convert(top),
            recent: convert(recent),
            popular: convert(popular),
            allow_custom_emoji,
            premium_locked,
        };
        // tdesktop adds the paid reaction to `recent` when paid is enabled
        // (data_message_reactions.cpp), so an offer of Stars alone still
        // earns the strip.
        self.stickers.message_reaction_options = if options.all().is_empty() && !paid_offered {
            MessageReactionsState::Unavailable {
                chat_id,
                message_id,
            }
        } else {
            MessageReactionsState::Ready(options)
        };
    }

    /// The message menu closed: nothing is asked for, and an answer still on
    /// its way is ignored.
    pub fn reset_message_reactions(&mut self) {
        self.stickers.message_reaction_options = MessageReactionsState::Idle;
        self.payments.paid.offered = None;
    }

    /// `getMessageAvailableReactions` failed: the strip stays away, unless
    /// the menu moved on to another message meanwhile.
    pub(crate) fn fail_message_reaction_options(&mut self, chat_id: ChatId, message_id: MessageId) {
        if self
            .stickers
            .message_reaction_options
            .is_loading_for(chat_id, message_id)
        {
            self.stickers.message_reaction_options = MessageReactionsState::Unavailable {
                chat_id,
                message_id,
            };
        }
    }

    /// How many reactions this account may leave on one message, once the
    /// app config is in.
    pub fn reaction_limit(&self) -> Option<usize> {
        self.stickers
            .reaction_limits
            .for_account(self.my_is_premium())
    }

    /// The user's own `reaction` added (`add`) or taken back, shown before
    /// TDLib answers (tdesktop `MessageReactions::add` / `remove`); TDLib's
    /// next `updateMessageInteractionInfo` replaces it.
    pub fn set_own_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        reaction: &ReactionType,
        add: bool,
    ) {
        let limit = self.reaction_limit();
        self.edit_loaded_message(chat_id, message_id, |message| {
            message.interaction_info = Some(crate::reaction_limit::with_own_reaction(
                message.interaction_info.as_ref(),
                reaction,
                add,
                limit,
            ));
        });
    }

    /// The Premium feature to show instead of reacting with `choice`, or
    /// `None` when the reaction may be sent (tdesktop
    /// `Window::ShowReactPremiumError`). A reaction already on the message
    /// is free, so is any reaction in a channel; otherwise TDLib's
    /// `needs_premium` and custom emoji need Premium.
    pub fn react_premium_feature(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        choice: &ReactionChoice,
    ) -> Option<ReactPremiumFeature> {
        if self.my_is_premium() {
            return None;
        }
        let reactions = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| message.interaction_info.as_ref())
            .and_then(|info| info.reactions.as_ref());
        if reactions.is_some_and(|r| r.are_tags) {
            return Some(ReactPremiumFeature::SavedMessagesTags);
        }
        let on_message = reactions.is_some_and(|r| {
            r.reactions
                .iter()
                .any(|one| ReactionChoice::from_type(&one.reaction_type).as_ref() == Some(choice))
        });
        if on_message {
            return None;
        }
        let locked = self
            .stickers
            .message_reaction_options
            .ready_for(chat_id, message_id)
            .is_some_and(|options| options.premium_locked.contains(choice));
        let broadcast = self
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_channel());
        let custom = matches!(choice, ReactionChoice::CustomEmoji(_));
        (locked || (custom && !broadcast)).then_some(ReactPremiumFeature::UniqueReactions)
    }
}
