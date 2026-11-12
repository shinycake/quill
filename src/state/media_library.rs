//! The composer's emoji/sticker panel library: the contents of every
//! installed sticker and custom-emoji set (loaded lazily as each section
//! comes into view), plus the reaction options of the message whose
//! reaction picker is open.
use super::*;
use crate::premium_hub::PremiumFeature;
use crate::telegram::envelope::{
    ReactionType, StoryAvailableReactionKind, StoryAvailableReactionView,
};
use std::collections::HashMap;

/// At most this many `getStickerSet` requests for the library in flight.
pub const MAX_LIBRARY_LOADS: usize = 4;

/// Where one set's contents stand (no entry: not asked for).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetLoad {
    /// `getStickerSet` went out as this request.
    Loading(RequestId),
    Ready(Vec<StickerItem>),
    /// The load failed this session (not retried automatically; the panel
    /// opening again asks anew).
    Failed,
}

#[derive(Debug, Clone, Default)]
pub struct MediaLibrary {
    sets: HashMap<i64, SetLoad>,
}

impl MediaLibrary {
    /// The loaded contents of a set.
    pub fn get(&self, set_id: &i64) -> Option<&Vec<StickerItem>> {
        match self.sets.get(set_id) {
            Some(SetLoad::Ready(items)) => Some(items),
            _ => None,
        }
    }

    pub fn contains_key(&self, set_id: &i64) -> bool {
        self.get(set_id).is_some()
    }

    /// The contents of every loaded set.
    pub fn values(&self) -> impl Iterator<Item = &Vec<StickerItem>> {
        self.sets.values().filter_map(|load| match load {
            SetLoad::Ready(items) => Some(items),
            _ => None,
        })
    }

    /// The set arrived (or is shown from elsewhere).
    pub fn insert(&mut self, set_id: i64, items: Vec<StickerItem>) {
        self.sets.insert(set_id, SetLoad::Ready(items));
    }

    /// The set's contents go: it loads again when wanted.
    pub fn remove(&mut self, set_id: &i64) {
        self.sets.remove(set_id);
    }

    /// Whether the set's load failed.
    pub fn is_failed(&self, set_id: i64) -> bool {
        self.sets.get(&set_id) == Some(&SetLoad::Failed)
    }

    /// `getStickerSet` for the set went out as `request`.
    pub fn begin_load(&mut self, set_id: i64, request: RequestId) {
        self.sets.insert(set_id, SetLoad::Loading(request));
    }

    /// The load failed (a set that is not loading keeps what it has).
    pub fn fail(&mut self, set_id: i64) {
        if matches!(self.sets.get(&set_id), Some(SetLoad::Loading(_))) {
            self.sets.insert(set_id, SetLoad::Failed);
        }
    }

    /// Failed sets are asked for again: the panel opened, or the
    /// connection came back.
    pub fn forget_failures(&mut self) {
        self.sets.retain(|_, load| *load != SetLoad::Failed);
    }

    fn loading_count(&self) -> usize {
        self.sets
            .values()
            .filter(|load| matches!(load, SetLoad::Loading(_)))
            .count()
    }
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
    /// The picker also offers the Star reaction (its own flow,
    /// `paid_reaction`).
    pub paid_offered: bool,
}

/// Where one message's reaction options stand. tdesktop reads them from its
/// local cache and skips the strip when there are none (`AttachSelectorToMenu`,
/// `reactions.recent.empty()`); Quill asks TDLib, so the wait and a failed or
/// empty answer are states of their own. A message with no entry in the
/// [`MessageReactionBook`] was not asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageReactionsState {
    /// `getMessageAvailableReactions` is in flight; only that request's
    /// answer or failure settles it.
    Loading {
        request: RequestId,
    },
    Ready(MessageReactionOptions),
    /// The options shown are out of date (the chat's or the account's
    /// reactions changed): the driver asks again and the strip stays until
    /// the new answer replaces it.
    Stale(MessageReactionOptions),
    /// The request failed or the answer held no reaction: no strip.
    Unavailable,
}

/// The reaction options of every message a menu is open on, keyed by
/// message: the main window and each chat window ask for their own, so a
/// second menu never replaces the first's state. A menu closing forgets its
/// message's entry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageReactionBook {
    by_message: HashMap<(ChatId, MessageId), MessageReactionsState>,
}

impl MessageReactionBook {
    /// `request` was sent for this message: it waits for that request.
    pub fn begin(&mut self, chat_id: ChatId, message_id: MessageId, request: RequestId) {
        self.by_message.insert(
            (chat_id, message_id),
            MessageReactionsState::Loading { request },
        );
    }

    /// Put a settled state in place (a fixture or a restored answer).
    pub fn set(&mut self, chat_id: ChatId, message_id: MessageId, state: MessageReactionsState) {
        self.by_message.insert((chat_id, message_id), state);
    }

    /// The menu of this message closed: its entry goes, and an answer still
    /// on its way is ignored.
    pub fn forget(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.by_message.remove(&(chat_id, message_id));
    }

    pub fn state(&self, chat_id: ChatId, message_id: MessageId) -> Option<&MessageReactionsState> {
        self.by_message.get(&(chat_id, message_id))
    }

    /// Every message's options that are loaded.
    pub fn ready_options(&self) -> impl Iterator<Item = &MessageReactionOptions> {
        self.by_message.values().filter_map(|state| match state {
            MessageReactionsState::Ready(options) | MessageReactionsState::Stale(options) => {
                Some(options)
            }
            _ => None,
        })
    }

    /// Something changed the options of open menus (for the messages `of`
    /// accepts): they are asked for again.
    pub fn mark_stale(&mut self, mut of: impl FnMut(ChatId, MessageId) -> bool) {
        for ((chat_id, message_id), state) in &mut self.by_message {
            if let MessageReactionsState::Ready(options) = state
                && of(*chat_id, *message_id)
            {
                *state = MessageReactionsState::Stale(options.clone());
            }
        }
    }

    /// The messages whose options are out of date.
    pub fn stale_messages(&self) -> Vec<(ChatId, MessageId)> {
        self.by_message
            .iter()
            .filter(|(_, state)| matches!(state, MessageReactionsState::Stale(_)))
            .map(|(key, _)| *key)
            .collect()
    }

    /// The loaded options of this message, to edit (fixtures).
    pub fn ready_mut(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<&mut MessageReactionOptions> {
        match self.by_message.get_mut(&(chat_id, message_id))? {
            MessageReactionsState::Ready(options) | MessageReactionsState::Stale(options) => {
                Some(options)
            }
            _ => None,
        }
    }

    /// The options when they belong to this message.
    pub fn ready_for(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<&MessageReactionOptions> {
        match self.state(chat_id, message_id)? {
            MessageReactionsState::Ready(options) | MessageReactionsState::Stale(options) => {
                Some(options)
            }
            _ => None,
        }
    }

    /// The picker of this message offers the Star reaction.
    pub fn paid_offered_for(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.ready_for(chat_id, message_id)
            .is_some_and(|options| options.paid_offered)
    }

    pub fn is_loading_for(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.loading_request(chat_id, message_id).is_some()
    }

    /// The request this message's entry waits for.
    pub fn loading_request(&self, chat_id: ChatId, message_id: MessageId) -> Option<RequestId> {
        match self.state(chat_id, message_id)? {
            MessageReactionsState::Loading { request } => Some(*request),
            _ => None,
        }
    }

    pub fn is_unavailable_for(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        matches!(
            self.state(chat_id, message_id),
            Some(MessageReactionsState::Unavailable)
        )
    }

    /// The book already holds the answer for this message. A wait does not
    /// count (its request may be gone), nor does a failed or empty answer:
    /// reopening the menu asks again unless a request is really pending.
    pub fn covers(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        matches!(
            self.state(chat_id, message_id),
            Some(MessageReactionsState::Ready(_))
        )
    }

    /// `request` settles this message's wait: true while the entry still
    /// waits for exactly that request.
    fn waits_for(&self, chat_id: ChatId, message_id: MessageId, request: RequestId) -> bool {
        matches!(
            self.state(chat_id, message_id),
            Some(MessageReactionsState::Loading { request: waiting }) if *waiting == request
        )
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
        self.media.media_library.insert(set_id, stickers);
    }

    pub(crate) fn fail_library_set(&mut self, set_id: i64) {
        self.media.media_library.fail(set_id);
    }

    /// Installed sets (regular, then custom emoji) whose contents aren't
    /// loaded, loading or failed, among `wanted`, capped by the free
    /// in-flight slots.
    pub fn library_sets_to_load(&self, wanted: &[i64]) -> Vec<i64> {
        let free = MAX_LIBRARY_LOADS.saturating_sub(self.media.media_library.loading_count());
        let mut out = Vec::new();
        for id in wanted {
            if out.len() >= free {
                break;
            }
            if *id != 0 && !self.media.media_library.sets.contains_key(id) && !out.contains(id) {
                out.push(*id);
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn accept_message_reaction_options(
        &mut self,
        request: RequestId,
        chat_id: ChatId,
        message_id: MessageId,
        top: Vec<StoryAvailableReactionView>,
        recent: Vec<StoryAvailableReactionView>,
        popular: Vec<StoryAvailableReactionView>,
        allow_custom_emoji: bool,
    ) {
        // Only the answer of the request this message waits for counts; a
        // late one of an older ask, or for a menu that closed, is dropped.
        if !self
            .stickers
            .message_reaction_options
            .waits_for(chat_id, message_id, request)
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
            paid_offered,
        };
        // tdesktop adds the paid reaction to `recent` when paid is enabled
        // (data_message_reactions.cpp), so an offer of Stars alone still
        // earns the strip.
        let state = if options.all().is_empty() && !paid_offered {
            MessageReactionsState::Unavailable
        } else {
            MessageReactionsState::Ready(options)
        };
        self.stickers
            .message_reaction_options
            .set(chat_id, message_id, state);
    }

    /// Whether the picker for this message offers the Star reaction.
    pub fn paid_reaction_offered(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.stickers
            .message_reaction_options
            .ready_for(chat_id, message_id)
            .is_some_and(|options| options.paid_offered)
    }

    /// The message menu on this message closed: nothing is asked for, and an
    /// answer still on its way is ignored.
    pub fn reset_message_reactions(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.stickers
            .message_reaction_options
            .forget(chat_id, message_id);
    }

    /// `getMessageAvailableReactions` failed: the strip stays away, unless
    /// the menu closed or asked again (another request) meanwhile.
    pub(crate) fn fail_message_reaction_options(
        &mut self,
        request: RequestId,
        chat_id: ChatId,
        message_id: MessageId,
    ) {
        if self
            .stickers
            .message_reaction_options
            .waits_for(chat_id, message_id, request)
        {
            self.stickers.message_reaction_options.set(
                chat_id,
                message_id,
                MessageReactionsState::Unavailable,
            );
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

    /// [`Self::set_own_reaction`] for the request `extra`, undone if TDLib
    /// refuses it: no answer but `updateMessageInteractionInfo` would ever
    /// take the reaction off the bubble again.
    pub(crate) fn set_own_reaction_for(
        &mut self,
        extra: RequestId,
        chat_id: ChatId,
        message_id: MessageId,
        reaction: &ReactionType,
        add: bool,
    ) {
        let before = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .map(|message| message.interaction_info.clone());
        self.set_own_reaction(chat_id, message_id, reaction, add);
        if let Some(before) = before {
            let after = self
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.get(&message_id.0))
                .and_then(|message| message.interaction_info.clone());
            self.messages.reaction_undo.insert(
                extra,
                ReactionUndo {
                    chat_id,
                    message_id,
                    before,
                    after,
                    reaction: reaction.clone(),
                    add,
                },
            );
        }
    }

    /// TDLib answered a reaction request: on a refusal the bubble goes back
    /// to what it showed, or, when a later reaction change sits on top of
    /// this one, only this change is taken back.
    pub(crate) fn settle_own_reaction(&mut self, extra: Option<RequestId>, refused: bool) {
        let Some(undo) = extra.and_then(|extra| self.messages.reaction_undo.remove(&extra)) else {
            return;
        };
        if !refused {
            return;
        }
        let limit = self.reaction_limit();
        self.edit_loaded_message(undo.chat_id, undo.message_id, |message| {
            message.interaction_info = if message.interaction_info == undo.after {
                undo.before.clone()
            } else {
                Some(crate::reaction_limit::with_own_reaction(
                    message.interaction_info.as_ref(),
                    &undo.reaction,
                    !undo.add,
                    limit,
                ))
            };
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
    ) -> Option<PremiumFeature> {
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
            return Some(PremiumFeature::SavedMessagesTags);
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
        (locked || (custom && !broadcast)).then_some(PremiumFeature::UniqueReactions)
    }
}
