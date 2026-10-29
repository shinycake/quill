#![deny(clippy::indexing_slicing)]

use crate::state::{RequestPurpose, Session};
use crate::telegram::envelope::StickersPayload;
use crate::telegram::envelope::{
    EnvelopePayload, MessageContent, ParsedFile, StickerItem, StickerSetInfo,
};
use crate::telegram::envelope_emoji::{EmojiCategory, EmojiStatusItem};
use crate::text::TextEntityKind;
use std::collections::{HashMap, HashSet};

mod custom_stickers;
pub use custom_stickers::CustomEmojiStickers;
pub mod keyword_lookup;
pub use keyword_lookup::KeywordLookups;

/// Most resolved custom emoji kept in `EmojiPanel::custom_emoji_stickers`.
pub const CUSTOM_EMOJI_CACHE_CAP: usize = 4000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EmojiSetTab {
    #[default]
    Installed,
    Trending,
    Search,
}

/// Slice S10: custom-emoji backend (packs + statuses + picker search).
/// Mirrors `StickerPanel`'s shape but stays separate — emoji sets, statuses
/// and picker answers are a different domain from regular stickers, and the
/// two panels must never share a slot.
#[derive(Debug, Clone, Default)]
pub struct EmojiPanel {
    pub mutating_set: Option<(i64, bool)>,
    /// Sets whose cover previews were asked for, so a repaint never asks twice.
    pub covers_requested: HashSet<i64>,
    pub pending_status_emoji: Option<i64>,
    pub outdated_packs: std::collections::HashSet<i64>,
    pub status_open: bool,
    pub status_note: Option<String>,
    pub status_resolution_attempted: std::collections::HashSet<i64>,
    pub open: bool,
    pub failed: bool,
    pub mutation_failed: bool,
    pub tab: EmojiSetTab,
    pub search_query: String,
    pub trending_offset: i32,
    pub trending_next_offset: i32,
    pub trending_total: i32,
    /// `updateTrendingStickerSets` for custom emoji arrived.
    pub trending_stale: bool,
    pub trending_has_more: bool,
    /// Slice S10: installed emoji sets (`getInstalledStickerSets` with
    /// `stickerTypeCustomEmoji`) — the "Emoji Sets" settings list.
    pub installed_sets: Vec<StickerSetInfo>,
    /// Slice S10: archived emoji sets (`getArchivedStickerSets`, paged).
    pub archived_sets: Vec<StickerSetInfo>,
    /// Slice S10: trending/discover emoji sets + premium-row flag.
    pub trending_sets: Vec<StickerSetInfo>,
    pub trending_is_premium: bool,
    /// Slice S10: `searchStickerSets` results (emoji type).
    pub found_sets: Vec<StickerSetInfo>,
    /// Slice S10: recent emoji statuses (`getRecentEmojiStatuses`).
    pub recent_statuses: Vec<EmojiStatusItem>,
    /// Slice S10: picker's "trending" statuses (`getThemedEmojiStatuses`).
    pub themed_status_ids: Vec<i64>,
    /// Slice S10: default statuses (`getDefaultEmojiStatuses`).
    pub default_status_ids: Vec<i64>,
    /// `getDefaultChatEmojiStatuses`: the statuses of channels and groups.
    pub chat_default_status_ids: Vec<i64>,
    /// The Appearance box of a chat is open and shows those statuses.
    pub chat_status_open: bool,
    /// Slice S10: upgraded-gift statuses (`getUpgradedGiftEmojiStatuses`).
    pub upgraded_gift_statuses: Vec<EmojiStatusItem>,
    /// Slice S10: last `getAnimatedEmoji` answer for the composer's
    /// "suggest animated emoji".
    pub animated_emoji: Option<StickerItem>,
    /// Suggest-animated-emoji: the emoji the last `getAnimatedEmoji`
    /// request went out for; dedupes requests across keystrokes.
    pub animated_emoji_for: Option<String>,
    /// Slice S10: last `getCustomEmojiStickers` answer.
    pub custom_emoji_stickers: CustomEmojiStickers,
    /// `searchEmojis` answers for the composer's emoji suggestions.
    pub keyword_lookups: KeywordLookups,
    /// B11: `getKeywordEmojis` answer for the panel's emoji search.
    pub keyword_emojis: Vec<String>,
    /// Slice S10: `getEmojiCategories` rows for the picker.
    pub categories: Vec<EmojiCategory>,
    /// `getEmojiCategories` for the sticker panel (`GetStickerCategories`).
    pub sticker_categories: Vec<EmojiCategory>,
    /// The sticker categories were asked for.
    pub sticker_categories_asked: bool,
}

/// B11: input-language codes for `getKeywordEmojis`: the script of what
/// was typed (tdesktop searches the keyword packs of every keyboard
/// language in use), then the system language, then English.
pub fn keyword_language_codes(query: &str, locale: Option<&str>) -> Vec<String> {
    fn push(out: &mut Vec<String>, code: &str) {
        if !code.is_empty() && !out.iter().any(|c| c == code) {
            out.push(code.to_string());
        }
    }
    let mut out = Vec::new();
    for c in query.chars() {
        let codes: &[&str] = match c as u32 {
            0x0400..=0x052F => &["ru", "uk"],
            0x0590..=0x05FF => &["he"],
            0x0600..=0x06FF | 0x0750..=0x077F => &["ar", "fa"],
            0x0370..=0x03FF => &["el"],
            0x0E00..=0x0E7F => &["th"],
            0x0900..=0x097F => &["hi"],
            0x3040..=0x30FF => &["ja"],
            0xAC00..=0xD7AF | 0x1100..=0x11FF => &["ko"],
            0x4E00..=0x9FFF => &["zh", "ja"],
            _ => &[],
        };
        for code in codes {
            push(&mut out, code);
        }
    }
    if let Some(locale) = locale {
        let lang = locale
            .split(['_', '-', '.', '@'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if lang.len() == 2 || lang.len() == 3 {
            push(&mut out, &lang);
        }
    }
    push(&mut out, "en");
    out
}

/// The system language for [`keyword_language_codes`] (`LC_ALL`,
/// `LC_MESSAGES`, `LANG`; unset on Windows, where English is the fallback).
pub fn system_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
}

/// Custom emoji ids in the caption of a photo, video or animation: the
/// media viewer renders captions with them.
fn viewer_caption_custom_emoji(content: &MessageContent) -> Vec<i64> {
    let entities = match content {
        MessageContent::Photo(photo) => &photo.caption_entities,
        MessageContent::Video(video) => &video.caption_entities,
        MessageContent::Animation(animation) => &animation.caption_entities,
        _ => return Vec::new(),
    };
    entities
        .iter()
        .filter_map(|entity| match entity.kind {
            TextEntityKind::CustomEmoji { custom_emoji_id } => Some(custom_emoji_id),
            _ => None,
        })
        .collect()
}

impl Session {
    pub fn remember_emoji_pack_usage(&mut self, custom_emoji_ids: &[i64]) -> bool {
        for id in custom_emoji_ids.iter().filter(|id| **id > 0) {
            self.settings
                .media_prefs
                .recent_custom_emoji_ids
                .retain(|old| old != id);
            self.settings
                .media_prefs
                .recent_custom_emoji_ids
                .insert(0, *id);
            self.settings
                .media_prefs
                .recent_custom_emoji_ids
                .truncate(128);
        }
        let mut packs = Vec::new();
        // ponytail: 128 recents; stable vector scans, index the cache if this limit grows.
        for id in &self.settings.media_prefs.recent_custom_emoji_ids {
            if let Some(set_id) = self
                .stickers
                .emoji
                .custom_emoji_stickers
                .iter()
                .find(|item| item.custom_emoji_id == Some(*id))
                .map(|item| item.set_id)
                .filter(|id| *id > 0)
                && !packs.contains(&set_id)
            {
                packs.push(set_id);
            }
        }
        for id in &self.settings.media_prefs.recent_emoji_packs {
            if !packs.contains(id) {
                packs.push(*id);
            }
        }
        packs.truncate(128);
        let changed = packs != self.settings.media_prefs.recent_emoji_packs;
        self.settings.media_prefs.recent_emoji_packs = packs;
        changed
    }

    pub fn ordered_emoji_packs(&self) -> Vec<&StickerSetInfo> {
        let mut sets: Vec<_> = self.stickers.emoji.installed_sets.iter().collect();
        if self.settings.media_prefs.dynamic_emoji_pack_order {
            sets.sort_by_key(|set| {
                self.settings
                    .media_prefs
                    .recent_emoji_packs
                    .iter()
                    .position(|id| *id == set.id)
                    .unwrap_or(usize::MAX)
            });
        }
        sets
    }

    fn remember_cover_files(&mut self, sets: &[StickerSetInfo]) {
        for set in sets {
            self.remember_files(&set.cover_files);
        }
    }

    /// Slice S10: store the installed emoji sets.
    pub fn accept_installed_emoji_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.remember_cover_files(&sets);
        let installed: std::collections::HashSet<_> = sets.iter().map(|set| set.id).collect();
        for set in self
            .stickers
            .emoji
            .found_sets
            .iter_mut()
            .chain(self.stickers.emoji.trending_sets.iter_mut())
        {
            set.is_installed = installed.contains(&set.id);
        }
        if self.stickers.emoji.tab == EmojiSetTab::Installed {
            self.stickers.emoji.failed = false;
        }
        self.stickers.emoji.installed_sets = sets;
    }

    /// Slice S10: store an archived-emoji-sets page. First page replaces,
    /// later pages append (the purpose carries `first_page`).
    pub fn accept_archived_emoji_sets(&mut self, sets: Vec<StickerSetInfo>, first_page: bool) {
        if first_page {
            self.stickers.emoji.archived_sets = sets;
        } else {
            self.stickers.emoji.archived_sets.extend(sets);
        }
    }

    /// Slice S10: store the trending emoji sets (single-page replace, like S8).
    pub fn accept_trending_emoji_sets(&mut self, sets: Vec<StickerSetInfo>, is_premium: bool) {
        self.remember_cover_files(&sets);
        if self.stickers.emoji.tab == EmojiSetTab::Trending {
            self.stickers.emoji.failed = false;
        }
        let raw_count = sets.len() as i32;
        if self.stickers.emoji.trending_offset == 0 {
            self.stickers.emoji.trending_sets.clear();
        }
        for set in sets {
            if !self
                .stickers
                .emoji
                .trending_sets
                .iter()
                .any(|old| old.id == set.id)
            {
                self.stickers.emoji.trending_sets.push(set);
            }
        }
        self.stickers.emoji.trending_next_offset = self
            .stickers
            .emoji
            .trending_offset
            .saturating_add(raw_count);
        self.stickers.emoji.trending_has_more = raw_count > 0
            && self.stickers.emoji.trending_next_offset < self.stickers.emoji.trending_total;
        self.stickers.emoji.trending_is_premium = is_premium;
    }

    /// Slice S10: store a `searchStickerSets` answer (emoji type).
    pub fn accept_found_emoji_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.remember_cover_files(&sets);
        if self.stickers.emoji.tab == EmojiSetTab::Search {
            self.stickers.emoji.failed = false;
        }
        self.stickers.emoji.found_sets = sets;
    }

    /// Slice S10: an emoji-set mutation (`changeStickerSet` /
    /// `reorderInstalledStickerSets` with `stickerTypeCustomEmoji`) succeeded —
    /// drop the installed-emoji-sets cache so the settings screen
    /// refetches the authoritative list.
    pub fn invalidate_installed_emoji_sets(&mut self) {
        self.stickers.emoji.installed_sets.clear();
    }

    /// Slice S10: purpose-gated dispatch for the emoji payloads. Stray
    /// answers (no matching purpose) are ignored, like the original arms.
    pub fn dispatch_emoji_payload(
        &mut self,
        purpose: Option<RequestPurpose>,
        payload: EnvelopePayload,
    ) {
        match payload {
            EnvelopePayload::Stickers(StickersPayload::EmojiStatuses { statuses }) => {
                self.accept_emoji_statuses(purpose, statuses);
            }
            EnvelopePayload::Stickers(StickersPayload::EmojiStatusCustomEmojis {
                custom_emoji_ids,
            }) => {
                self.accept_emoji_status_ids(purpose, custom_emoji_ids);
            }
            EnvelopePayload::Stickers(StickersPayload::AnimatedEmoji { sticker, files }) => {
                self.accept_animated_emoji(purpose, sticker, files);
            }
            EnvelopePayload::Stickers(StickersPayload::Emojis { emojis }) => {
                if purpose == Some(RequestPurpose::GetKeywordEmojis) {
                    self.stickers.emoji.keyword_emojis = emojis;
                }
            }
            EnvelopePayload::Stickers(StickersPayload::EmojiCategories { categories, files }) => {
                self.accept_emoji_categories(purpose, categories, files);
            }
            _ => {}
        }
    }

    /// Slice S10: store `emojiStatuses` under the matching status purpose.
    pub fn accept_emoji_statuses(
        &mut self,
        purpose: Option<RequestPurpose>,
        statuses: Vec<EmojiStatusItem>,
    ) {
        if purpose == Some(RequestPurpose::GetRecentEmojiStatuses) {
            self.stickers.emoji.recent_statuses = statuses;
        } else if purpose == Some(RequestPurpose::GetUpgradedGiftEmojiStatuses) {
            self.stickers.emoji.upgraded_gift_statuses = statuses;
        }
    }

    /// Slice S10: store `emojiStatusCustomEmojis` under the matching purpose.
    pub fn accept_emoji_status_ids(
        &mut self,
        purpose: Option<RequestPurpose>,
        custom_emoji_ids: Vec<i64>,
    ) {
        if purpose == Some(RequestPurpose::GetThemedEmojiStatuses) {
            self.stickers.emoji.themed_status_ids = custom_emoji_ids;
        } else if purpose == Some(RequestPurpose::GetDefaultEmojiStatuses) {
            self.stickers.emoji.default_status_ids = custom_emoji_ids;
        } else if purpose == Some(RequestPurpose::GetDefaultChatEmojiStatuses) {
            self.stickers.emoji.chat_default_status_ids = custom_emoji_ids;
        }
    }

    /// Slice S10: store `animatedEmoji` under `GetAnimatedEmoji`.
    pub fn accept_animated_emoji(
        &mut self,
        purpose: Option<RequestPurpose>,
        sticker: Option<StickerItem>,
        files: Vec<ParsedFile>,
    ) {
        if purpose == Some(RequestPurpose::GetAnimatedEmoji) {
            self.remember_files(&files);
            self.stickers.emoji.animated_emoji = sticker;
        }
    }

    /// Slice S10: store `emojiCategories` under `GetEmojiCategories`.
    pub fn accept_emoji_categories(
        &mut self,
        purpose: Option<RequestPurpose>,
        categories: Vec<EmojiCategory>,
        files: Vec<ParsedFile>,
    ) {
        if purpose == Some(RequestPurpose::GetEmojiCategories) {
            self.remember_files(&files);
            self.stickers.emoji.categories = categories;
        } else if purpose == Some(RequestPurpose::GetStickerCategories) {
            self.remember_files(&files);
            self.stickers.emoji.sticker_categories = categories;
        }
    }

    /// Slice S10: store a `getCustomEmojiStickers` answer in the emoji panel.
    ///
    /// A re-sent sticker replaces the old entry and moves to the end. Past
    /// [`CUSTOM_EMOJI_CACHE_CAP`] entries the oldest are dropped, except ids
    /// [`Self::open_chat_custom_emoji_ids`] says are on screen; a dropped
    /// id is resolved again the next time it shows (its attempt mark is
    /// cleared so the request goes out).
    pub fn accept_custom_emoji_stickers(&mut self, stickers: Vec<StickerItem>) {
        let emoji = &mut self.stickers.emoji;
        // The last occurrence of an id wins, like the old retain + push.
        let mut newest: HashMap<(Option<i64>, i64), usize> = HashMap::new();
        for (ix, sticker) in stickers.iter().enumerate() {
            newest.insert(Self::custom_emoji_key(sticker), ix);
        }
        emoji.custom_emoji_stickers.update(|list| {
            list.retain(|old| !newest.contains_key(&Self::custom_emoji_key(old)));
            list.extend(
                stickers
                    .into_iter()
                    .enumerate()
                    .filter(|(ix, sticker)| {
                        newest.get(&Self::custom_emoji_key(sticker)) == Some(ix)
                    })
                    .map(|(_, sticker)| sticker),
            );
        });
        let mut excess = self
            .stickers
            .emoji
            .custom_emoji_stickers
            .len()
            .saturating_sub(CUSTOM_EMOJI_CACHE_CAP);
        if excess == 0 {
            return;
        }
        // What the screen shows right now is never evicted; derived from
        // state each time, so there is no second list to keep in sync.
        let wanted: HashSet<i64> = self.open_chat_custom_emoji_ids().into_iter().collect();
        let emoji = &mut self.stickers.emoji;
        let mut dropped_ids = Vec::new();
        emoji.custom_emoji_stickers.update(|list| {
            list.retain(|sticker| {
                let visible = sticker
                    .custom_emoji_id
                    .is_some_and(|id| wanted.contains(&id));
                if excess == 0 || visible {
                    return true;
                }
                excess -= 1;
                dropped_ids.extend(sticker.custom_emoji_id);
                false
            });
        });
        for id in dropped_ids {
            emoji.status_resolution_attempted.remove(&id);
        }
    }

    /// Identity of a resolved custom emoji: its id, or (for items without
    /// one) the sticker's file.
    fn custom_emoji_key(sticker: &StickerItem) -> (Option<i64>, i64) {
        match sticker.custom_emoji_id {
            Some(id) => (Some(id), 0),
            None => (None, i64::from(sticker.file_id.0)),
        }
    }

    /// Custom emoji the open chat shows: ids in message text and style link
    /// cards, custom-emoji reactions under messages, and those the open
    /// reaction picker offers. Sorted and deduplicated.
    pub fn open_chat_custom_emoji_ids(&self) -> Vec<i64> {
        use crate::telegram::envelope::{LinkPreviewKind, ReactionType};
        let mut ids = Vec::new();
        for message in self
            .open_chat
            .and_then(|id| self.histories.get(&id.0))
            .into_iter()
            .flat_map(|history| history.messages.values())
        {
            if let Some(reactions) = message
                .interaction_info
                .as_ref()
                .and_then(|info| info.reactions.as_ref())
            {
                ids.extend(reactions.reactions.iter().filter_map(|reaction| {
                    match reaction.reaction_type {
                        ReactionType::CustomEmoji { custom_emoji_id } => Some(custom_emoji_id),
                        _ => None,
                    }
                }));
            }
            if let MessageContent::Text(text) = &message.content {
                ids.extend(text.entities.iter().filter_map(|entity| match entity.kind {
                    TextEntityKind::CustomEmoji { custom_emoji_id } => Some(custom_emoji_id),
                    _ => None,
                }));
                if let Some(LinkPreviewKind::TextCompositionStyle { custom_emoji_id }) =
                    text.link_preview.as_ref().map(|preview| &preview.kind)
                    && *custom_emoji_id != 0
                {
                    ids.push(*custom_emoji_id);
                }
            }
            // The custom emoji in the text of a poll's options.
            if let MessageContent::Poll(poll) = &message.content {
                for option in &poll.poll.options {
                    ids.extend(
                        option
                            .entities
                            .iter()
                            .filter_map(|entity| match entity.kind {
                                TextEntityKind::CustomEmoji { custom_emoji_id } => {
                                    Some(custom_emoji_id)
                                }
                                _ => None,
                            }),
                    );
                }
            }
            ids.extend(viewer_caption_custom_emoji(&message.content));
            // The status after a sender's name in a group.
            ids.extend(self.sender_status_id(message));
        }
        // The status next to the name in the open profile panel.
        if let Some(crate::state::InfoPanelTarget::User(user_id)) = self.users_state.open_info_panel
            && let Some(user) = self.user(user_id)
            && user.is_premium
        {
            ids.push(user.emoji_status_id);
        }
        // The emoji repeated behind reply strips.
        ids.extend(self.reply_background_emoji_ids());
        // Captions of the Shared Media lists the viewer pages over.
        for item in self
            .media
            .shared_media
            .tabs
            .iter()
            .flat_map(|tab| tab.items.iter())
        {
            if let Some(message) = &item.message {
                ids.extend(viewer_caption_custom_emoji(&message.content));
            }
        }
        // Custom emoji in chat-list previews.
        for chat in self.chats.values() {
            ids.extend(chat.last_preview_style.entities.iter().filter_map(
                |entity| match entity.kind {
                    TextEntityKind::CustomEmoji { custom_emoji_id } => Some(custom_emoji_id),
                    _ => None,
                },
            ));
            // Premium emoji statuses after row titles.
            if let Some(crate::peer_badge::TitleBadge::EmojiStatus(id)) =
                self.chat_title_badge(chat)
            {
                ids.push(id);
            }
            // The verifying bot's icon before the title.
            ids.extend(self.chat_bot_verification_icon(chat));
        }
        // Custom emoji icons of the topics listed on forum rows.
        for topics in self.threads.forum_topics.values() {
            ids.extend(topics.iter().map(|t| t.icon_custom_emoji_id));
        }
        if let Some(options) = self.stickers.message_reaction_options.ready() {
            ids.extend(options.all().into_iter().filter_map(|choice| match choice {
                crate::state::ReactionChoice::CustomEmoji(id) => Some(id),
                crate::state::ReactionChoice::Emoji(_) => None,
            }));
        }
        ids.retain(|id| *id > 0);
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// [`Self::open_chat_custom_emoji_ids`] that are neither resolved
    /// (`custom_emoji_stickers`) nor already attempted, capped at the
    /// server's 200-ids-per-call limit.
    pub fn message_custom_emoji_ids_to_resolve(&self) -> Vec<i64> {
        self.unresolved_custom_emoji_ids(self.open_chat_custom_emoji_ids())
    }

    /// `ids` minus the resolved and already attempted ones, capped at 200.
    pub fn unresolved_custom_emoji_ids(&self, ids: Vec<i64>) -> Vec<i64> {
        self.unresolved_custom_emoji_ids_in(ids, &self.resolved_custom_emoji_ids())
    }

    /// [`Self::unresolved_custom_emoji_ids`] with the resolved set already
    /// built by the caller.
    pub fn unresolved_custom_emoji_ids_in(
        &self,
        mut ids: Vec<i64>,
        resolved: &HashSet<i64>,
    ) -> Vec<i64> {
        ids.retain(|id| {
            !self.stickers.emoji.status_resolution_attempted.contains(id) && !resolved.contains(id)
        });
        ids.truncate(200);
        ids
    }

    /// Custom emoji ids `custom_emoji_stickers` already holds.
    pub fn resolved_custom_emoji_ids(&self) -> HashSet<i64> {
        self.stickers
            .emoji
            .custom_emoji_stickers
            .iter()
            .filter_map(|s| s.custom_emoji_id)
            .collect()
    }

    /// Image files of the open chat's resolved custom emoji that still need
    /// a download (the renderer shows the fallback emoji until they land).
    pub fn open_chat_custom_emoji_files(&self) -> Vec<crate::ids::FileId> {
        self.custom_emoji_files_for(&self.open_chat_custom_emoji_ids())
    }

    /// [`Self::open_chat_custom_emoji_files`] for `ids` (sorted ascending,
    /// as [`Self::open_chat_custom_emoji_ids`] returns them).
    pub fn custom_emoji_files_for(&self, ids: &[i64]) -> Vec<crate::ids::FileId> {
        self.stickers
            .emoji
            .custom_emoji_stickers
            .iter()
            .filter(|s| {
                s.custom_emoji_id
                    .is_some_and(|id| ids.binary_search(&id).is_ok())
            })
            .filter_map(|s| s.display_file_id())
            .filter(|file| self.should_download(*file))
            .collect()
    }

    /// Slice S10: an emoji-status or emoji-set mutation succeeded — drop the
    /// affected cache so the next fetch shows the server-confirmed state.
    pub fn invalidate_emoji_caches(&mut self, purpose: Option<RequestPurpose>) {
        if purpose == Some(RequestPurpose::ChangeEmojiSet) {
            self.stickers.emoji.mutation_failed = false;
            self.stickers.emoji.mutating_set = None;
        }
        if matches!(
            purpose,
            Some(RequestPurpose::SetEmojiStatus | RequestPurpose::ClearRecentEmojiStatuses)
        ) {
            if purpose == Some(RequestPurpose::SetEmojiStatus)
                && let Some(id) = self.stickers.emoji.pending_status_emoji.take()
            {
                self.remember_emoji_pack_usage(&[id]);
            }
            self.stickers.emoji.recent_statuses.clear();
            drop(
                self.requests
                    .take_purpose(RequestPurpose::GetRecentEmojiStatuses),
            );
            self.stickers.emoji.status_note = Some(
                if purpose == Some(RequestPurpose::SetEmojiStatus) {
                    "Emoji status updated."
                } else {
                    "Recent emoji statuses cleared."
                }
                .into(),
            );
        }
        if matches!(
            purpose,
            Some(RequestPurpose::ChangeEmojiSet | RequestPurpose::ReorderInstalledEmojiSets)
        ) {
            self.invalidate_installed_emoji_sets();
        }
    }
}

#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod keyword_language_tests;
