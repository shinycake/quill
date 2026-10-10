use crate::state::{RequestPurpose, Session};
use crate::telegram::envelope::StickersPayload;
use crate::telegram::envelope::{
    EnvelopePayload, MessageContent, ParsedFile, StickerItem, StickerSetInfo,
};
use crate::telegram::envelope_emoji::{EmojiCategory, EmojiKeyword, EmojiStatusItem};
use crate::text::TextEntityKind;

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
    pub pending_status_emoji: Option<i64>,
    pub pack_files: std::collections::HashMap<i64, Vec<crate::ids::FileId>>,
    pub outdated_packs: std::collections::HashSet<i64>,
    pub status_open: bool,
    pub status_duration_secs: i32,
    pub status_note: Option<String>,
    pub status_resolution_attempted: std::collections::HashSet<i64>,
    pub open: bool,
    pub failed: bool,
    pub mutation_failed: bool,
    pub tab: EmojiSetTab,
    pub selected_set_id: Option<i64>,
    pub preview_title: String,
    pub preview: Vec<StickerItem>,
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
    /// Slice S10: upgraded-gift statuses (`getUpgradedGiftEmojiStatuses`).
    pub upgraded_gift_statuses: Vec<EmojiStatusItem>,
    /// Slice S10: last `getAnimatedEmoji` answer for the composer's
    /// "suggest animated emoji".
    pub animated_emoji: Option<StickerItem>,
    /// Suggest-animated-emoji: the emoji the last `getAnimatedEmoji`
    /// request went out for; dedupes requests across keystrokes.
    pub animated_emoji_for: Option<String>,
    /// Slice S10: last `getCustomEmojiStickers` answer.
    pub custom_emoji_stickers: Vec<StickerItem>,
    /// Slice S10: `searchEmojis` results for the picker.
    pub keyword_results: Vec<EmojiKeyword>,
    /// B11: `getKeywordEmojis` answer for the panel's emoji search.
    pub keyword_emojis: Vec<String>,
    /// Slice S10: `getEmojiCategories` rows for the picker.
    pub categories: Vec<EmojiCategory>,
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
            self.media_prefs
                .recent_custom_emoji_ids
                .retain(|old| old != id);
            self.media_prefs.recent_custom_emoji_ids.insert(0, *id);
            self.media_prefs.recent_custom_emoji_ids.truncate(128);
        }
        let mut packs = Vec::new();
        // ponytail: 128 recents; stable vector scans, index the cache if this limit grows.
        for id in &self.media_prefs.recent_custom_emoji_ids {
            if let Some(set_id) = self
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
        for id in &self.media_prefs.recent_emoji_packs {
            if !packs.contains(id) {
                packs.push(*id);
            }
        }
        packs.truncate(128);
        let changed = packs != self.media_prefs.recent_emoji_packs;
        self.media_prefs.recent_emoji_packs = packs;
        changed
    }

    pub fn ordered_emoji_packs(&self) -> Vec<&StickerSetInfo> {
        let mut sets: Vec<_> = self.emoji.installed_sets.iter().collect();
        if self.media_prefs.dynamic_emoji_pack_order {
            sets.sort_by_key(|set| {
                self.media_prefs
                    .recent_emoji_packs
                    .iter()
                    .position(|id| *id == set.id)
                    .unwrap_or(usize::MAX)
            });
        }
        sets
    }

    pub fn emoji_pack_download_state(&self, id: i64) -> &'static str {
        if let Some((set_id, installed)) = self.emoji.mutating_set
            && set_id == id
        {
            return if installed {
                "Installing…"
            } else {
                "Removing…"
            };
        }
        if self.emoji.outdated_packs.contains(&id) {
            return "Update needed";
        }
        let Some(ids) = self.emoji.pack_files.get(&id) else {
            return "Not downloaded";
        };
        if !ids.is_empty()
            && ids.iter().all(|id| {
                self.files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .is_some()
            })
        {
            return "Downloaded";
        }
        if ids.iter().any(|id| {
            self.downloading.contains(&id.0)
                || self
                    .files
                    .get(&id.0)
                    .is_some_and(|file| file.local.is_downloading_active)
        }) {
            return "Downloading…";
        }
        "Not downloaded"
    }
    /// Slice S10: store the installed emoji sets.
    pub fn accept_installed_emoji_sets(&mut self, sets: Vec<StickerSetInfo>) {
        let installed: std::collections::HashSet<_> = sets.iter().map(|set| set.id).collect();
        for set in self
            .emoji
            .found_sets
            .iter_mut()
            .chain(self.emoji.trending_sets.iter_mut())
        {
            set.is_installed = installed.contains(&set.id);
        }
        if self.emoji.tab == EmojiSetTab::Installed {
            self.emoji.failed = false;
        }
        self.emoji.installed_sets = sets;
    }

    /// Slice S10: store an archived-emoji-sets page. First page replaces,
    /// later pages append (the purpose carries `first_page`).
    pub fn accept_archived_emoji_sets(&mut self, sets: Vec<StickerSetInfo>, first_page: bool) {
        if first_page {
            self.emoji.archived_sets = sets;
        } else {
            self.emoji.archived_sets.extend(sets);
        }
    }

    /// Slice S10: store the trending emoji sets (single-page replace, like S8).
    pub fn accept_trending_emoji_sets(&mut self, sets: Vec<StickerSetInfo>, is_premium: bool) {
        if self.emoji.tab == EmojiSetTab::Trending {
            self.emoji.failed = false;
        }
        let raw_count = sets.len() as i32;
        if self.emoji.trending_offset == 0 {
            self.emoji.trending_sets.clear();
        }
        for set in sets {
            if !self.emoji.trending_sets.iter().any(|old| old.id == set.id) {
                self.emoji.trending_sets.push(set);
            }
        }
        self.emoji.trending_next_offset = self.emoji.trending_offset.saturating_add(raw_count);
        self.emoji.trending_has_more =
            raw_count > 0 && self.emoji.trending_next_offset < self.emoji.trending_total;
        self.emoji.trending_is_premium = is_premium;
    }

    /// Slice S10: store a `searchStickerSets` answer (emoji type).
    pub fn accept_found_emoji_sets(&mut self, sets: Vec<StickerSetInfo>) {
        if self.emoji.tab == EmojiSetTab::Search {
            self.emoji.failed = false;
        }
        self.emoji.found_sets = sets;
    }

    /// Slice S10: an emoji-set mutation (`changeStickerSet` /
    /// `reorderInstalledStickerSets` with `stickerTypeCustomEmoji`) succeeded —
    /// drop the installed-emoji-sets cache so the settings screen
    /// refetches the authoritative list.
    pub fn invalidate_installed_emoji_sets(&mut self) {
        self.emoji.installed_sets.clear();
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
            EnvelopePayload::Stickers(StickersPayload::EmojiKeywords { keywords }) => {
                self.accept_emoji_keywords(purpose, keywords);
            }
            EnvelopePayload::Stickers(StickersPayload::Emojis { emojis }) => {
                if purpose == Some(RequestPurpose::GetKeywordEmojis) {
                    self.emoji.keyword_emojis = emojis;
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
            self.emoji.recent_statuses = statuses;
        } else if purpose == Some(RequestPurpose::GetUpgradedGiftEmojiStatuses) {
            self.emoji.upgraded_gift_statuses = statuses;
        }
    }

    /// Slice S10: store `emojiStatusCustomEmojis` under the matching purpose.
    pub fn accept_emoji_status_ids(
        &mut self,
        purpose: Option<RequestPurpose>,
        custom_emoji_ids: Vec<i64>,
    ) {
        if purpose == Some(RequestPurpose::GetThemedEmojiStatuses) {
            self.emoji.themed_status_ids = custom_emoji_ids;
        } else if purpose == Some(RequestPurpose::GetDefaultEmojiStatuses) {
            self.emoji.default_status_ids = custom_emoji_ids;
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
            self.emoji.animated_emoji = sticker;
        }
    }

    /// Slice S10: store `emojiKeywords` under `SearchEmojis`.
    pub fn accept_emoji_keywords(
        &mut self,
        purpose: Option<RequestPurpose>,
        keywords: Vec<EmojiKeyword>,
    ) {
        if purpose == Some(RequestPurpose::SearchEmojis) {
            self.emoji.keyword_results = keywords;
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
            self.emoji.categories = categories;
        }
    }

    /// Slice S10: store a `getCustomEmojiStickers` answer in the emoji panel.
    pub fn accept_custom_emoji_stickers(&mut self, stickers: Vec<StickerItem>) {
        // ponytail: linear merge of batches capped at 200; index by custom emoji ID if cache size grows.
        for sticker in stickers {
            self.emoji.custom_emoji_stickers.retain(|old| {
                old.custom_emoji_id != sticker.custom_emoji_id
                    || (old.custom_emoji_id.is_none() && old.file_id != sticker.file_id)
            });
            self.emoji.custom_emoji_stickers.push(sticker);
        }
    }

    /// Custom emoji the open chat shows: ids in message text, custom-emoji
    /// reactions under messages, and those the open reaction picker offers.
    /// Sorted and deduplicated.
    pub fn open_chat_custom_emoji_ids(&self) -> Vec<i64> {
        use crate::telegram::envelope::ReactionType;
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
            }
            ids.extend(viewer_caption_custom_emoji(&message.content));
        }
        // The emoji repeated behind reply strips.
        ids.extend(self.reply_background_emoji_ids());
        // Captions of the Shared Media lists the viewer pages over.
        for item in self
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
        }
        if let Some(options) = &self.message_reaction_options {
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
        let mut ids = self.open_chat_custom_emoji_ids();
        ids.retain(|id| {
            !self.emoji.status_resolution_attempted.contains(id)
                && !self
                    .emoji
                    .custom_emoji_stickers
                    .iter()
                    .any(|s| s.custom_emoji_id == Some(*id))
        });
        ids.truncate(200);
        ids
    }

    /// Image files of the open chat's resolved custom emoji that still need
    /// a download (the renderer shows the fallback emoji until they land).
    pub fn open_chat_custom_emoji_files(&self) -> Vec<crate::ids::FileId> {
        let ids = self.open_chat_custom_emoji_ids();
        self.emoji
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
            self.emoji.mutation_failed = false;
            self.emoji.mutating_set = None;
        }
        if matches!(
            purpose,
            Some(RequestPurpose::SetEmojiStatus | RequestPurpose::ClearRecentEmojiStatuses)
        ) {
            if purpose == Some(RequestPurpose::SetEmojiStatus)
                && let Some(id) = self.emoji.pending_status_emoji.take()
            {
                self.remember_emoji_pack_usage(&[id]);
            }
            self.emoji.recent_statuses.clear();
            drop(
                self.requests
                    .take_purpose(RequestPurpose::GetRecentEmojiStatuses),
            );
            self.emoji.status_note = Some(
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
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::{AccountKey, ChatId};
    use crate::state::StickersPurpose;
    use crate::telegram::client::copy_and_parse;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn pack_order_preserves_usage_when_emoji_resolve_out_of_order() {
        let (mut session, _) = session();
        let sticker = |id, set_id| StickerItem {
            custom_emoji_id: Some(id),
            id,
            set_id,
            emoji: "😀".into(),
            width: 64,
            height: 64,
            format: crate::telegram::envelope::StickerFormat::Webp,
            file_id: crate::ids::FileId(1),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            requires_premium: false,
        };
        let pack = |id| StickerSetInfo {
            id,
            title: id.to_string(),
            name: id.to_string(),
            size: 1,
            is_installed: true,
            is_official: false,
        };
        session.emoji.installed_sets = vec![pack(1), pack(2), pack(3)];
        session.remember_emoji_pack_usage(&[10]);
        session.emoji.custom_emoji_stickers.push(sticker(20, 2));
        session.remember_emoji_pack_usage(&[20]);
        assert_eq!(session.media_prefs.recent_emoji_packs, vec![2]);
        session.emoji.custom_emoji_stickers.push(sticker(10, 1));
        session.remember_emoji_pack_usage(&[]);
        assert_eq!(
            session
                .ordered_emoji_packs()
                .iter()
                .map(|set| set.id)
                .collect::<Vec<_>>(),
            vec![2, 1, 3]
        );
        session.media_prefs.dynamic_emoji_pack_order = false;
        assert_eq!(
            session
                .ordered_emoji_packs()
                .iter()
                .map(|set| set.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        session.media_prefs.dynamic_emoji_pack_order = true;
        session.remember_emoji_pack_usage(&[10, 10, -1]);
        assert_eq!(session.media_prefs.recent_emoji_packs, vec![1, 2]);
        assert_eq!(session.media_prefs.recent_custom_emoji_ids, vec![10, 20]);
    }

    fn session() -> (Session, Arc<MemorySink>) {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        (Session::new(AccountKey::primary(), dyn_sink), sink)
    }

    fn apply_json(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
        session.apply(owned);
    }

    /// Custom emoji ids in the open chat's message text are collected for
    /// resolution: deduped, skipped when nothing is open, and excluded once
    /// attempted or already resolved.
    #[test]
    fn message_custom_emoji_ids_to_resolve_scans_open_chat() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let json = r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":4,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi 😀 bye","entities":[{"@type":"textEntity","offset":3,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"12345"}},{"@type":"textEntity","offset":3,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"12345"}}]}}}}"#;
        apply_json(&mut session, &seq, &sink, json);
        // Chat 4 isn't open: nothing to resolve.
        assert!(session.message_custom_emoji_ids_to_resolve().is_empty());
        session.open_chat = Some(ChatId(4));
        // Deduped even though the entity repeats.
        assert_eq!(session.message_custom_emoji_ids_to_resolve(), vec![12345]);
        // Attempted ids are excluded.
        session.emoji.status_resolution_attempted.insert(12345);
        assert!(session.message_custom_emoji_ids_to_resolve().is_empty());
    }

    /// Captions of photos, videos and animations contribute custom emoji
    /// too: the media viewer renders its caption with them.
    #[test]
    fn viewer_caption_custom_emoji_covers_media_captions() {
        use crate::telegram::envelope::{AnimationContent, PhotoContent};
        use crate::text::TextEntity;
        let entity = TextEntity {
            utf8_start: 0,
            utf8_end: 4,
            kind: TextEntityKind::CustomEmoji {
                custom_emoji_id: 777,
            },
        };
        let photo = MessageContent::Photo(PhotoContent {
            has_stickers: false,
            caption: "x".into(),
            caption_entities: vec![entity.clone()],
            show_caption_above_media: false,
            sizes: Vec::new(),
            is_secret: false,
            has_spoiler: false,
            minithumbnail: None,
        });
        assert_eq!(viewer_caption_custom_emoji(&photo), vec![777]);
        let gif = MessageContent::Animation(AnimationContent {
            duration: 1,
            width: 1,
            height: 1,
            file_name: String::new(),
            mime_type: String::new(),
            caption: "x".into(),
            caption_entities: vec![entity],
            show_caption_above_media: false,
            has_spoiler: false,
            is_secret: false,
            file_id: crate::ids::FileId(1),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
        });
        assert_eq!(viewer_caption_custom_emoji(&gif), vec![777]);
        assert!(
            viewer_caption_custom_emoji(&MessageContent::Unsupported {
                type_name: "x".into()
            })
            .is_empty()
        );
    }

    /// Custom-emoji reactions count as open-chat custom emoji, and resolved
    /// ones queue their image file once (not while it downloads).
    #[test]
    fn open_chat_custom_emoji_files_cover_text_and_reactions() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":4,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi 😀","entities":[{"@type":"textEntity","offset":3,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"12345"}}]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageInteractionInfo","chat_id":4,"message_id":9,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"777"},"total_count":2,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
        );
        session.open_chat = Some(ChatId(4));
        assert_eq!(session.open_chat_custom_emoji_ids(), vec![777, 12345]);
        let message = &session.histories[&4].messages[&9];
        assert_eq!(message.reaction_chips().len(), 1);
        assert!(message.emoji_reaction_chips().is_empty());
        assert!(message.chosen_reaction(&crate::state::ReactionChoice::CustomEmoji(777)));
        session.emoji.custom_emoji_stickers.push(StickerItem {
            custom_emoji_id: Some(777),
            id: 1,
            set_id: 2,
            emoji: "🔥".into(),
            width: 100,
            height: 100,
            format: crate::telegram::envelope::StickerFormat::Webp,
            file_id: crate::ids::FileId(55),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            requires_premium: false,
        });
        assert_eq!(
            session.open_chat_custom_emoji_files(),
            vec![crate::ids::FileId(55)]
        );
        session.begin_download(crate::ids::FileId(55));
        assert!(session.open_chat_custom_emoji_files().is_empty());
    }

    /// Slice S10: emoji-backend answers are stored only under a matching
    /// request purpose (stray answers ignored, never landing in the regular
    /// sticker panel), and mutation `ok`s invalidate the affected caches.
    #[test]
    fn s10_emoji_backend_purpose_gated_dispatch() {
        let (mut with_purpose, sink) = session();
        let seq = AtomicU64::new(0);

        // Recent statuses land in the emoji panel under
        // GetRecentEmojiStatuses.
        let extra = with_purpose.request(RequestPurpose::GetRecentEmojiStatuses, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"emojiStatuses","emoji_statuses":[{{"@type":"emojiStatus","type":{{"@type":"emojiStatusTypeCustomEmoji","custom_emoji_id":"12345"}},"expiration_date":3600}}],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.recent_statuses.len(), 1);
        assert_eq!(with_purpose.emoji.recent_statuses[0].custom_emoji_id, 12345);
        assert_eq!(with_purpose.emoji.recent_statuses[0].expiration_date, 3600);

        // A stray emojiStatuses (no matching purpose) is ignored.
        let (mut without_purpose, sink2) = session();
        let seq2 = AtomicU64::new(0);
        apply_json(
            &mut without_purpose,
            &seq2,
            &sink2,
            r#"{"@type":"emojiStatuses","emoji_statuses":[]}"#,
        );
        assert!(without_purpose.emoji.recent_statuses.is_empty());

        // Themed/default ids land in their own slots.
        let extra = with_purpose.request(RequestPurpose::GetThemedEmojiStatuses, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"emojiStatusCustomEmojis","custom_emoji_ids":["11","22"],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.themed_status_ids, vec![11, 22]);
        let extra = with_purpose.request(RequestPurpose::GetDefaultEmojiStatuses, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"emojiStatusCustomEmojis","custom_emoji_ids":["33"],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.default_status_ids, vec![33]);

        // getAnimatedEmoji's animatedEmoji lands under GetAnimatedEmoji.
        let extra = with_purpose.request(RequestPurpose::GetAnimatedEmoji, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"animatedEmoji","sticker":{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"🔥","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":null}},"sticker_width":512,"sticker_height":512,"fitzpatrick_type":0,"sound":null,"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            with_purpose
                .emoji
                .animated_emoji
                .as_ref()
                .expect("animated")
                .emoji,
            "🔥"
        );

        // getCustomEmojiStickers answers with bare `stickers` under
        // GetCustomEmojiStickers — the emoji panel's slot, never the
        // regular sticker panel's favorites.
        let extra = with_purpose.request(RequestPurpose::GetCustomEmojiStickers, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"stickers","stickers":[{{"@type":"sticker","id":"9002","set_id":"78","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":null}}],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.custom_emoji_stickers.len(), 1);
        assert!(with_purpose.stickers.favorites.is_empty());

        // searchEmojis answers land under SearchEmojis.
        let extra = with_purpose.request(RequestPurpose::SearchEmojis, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"emojiKeywords","emoji_keywords":[{{"@type":"emojiKeyword","emoji":"🔥","keyword":"fire"}}],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.keyword_results.len(), 1);
        assert_eq!(with_purpose.emoji.keyword_results[0].keyword, "fire");

        // getEmojiCategories answers land under GetEmojiCategories.
        let extra = with_purpose.request(RequestPurpose::GetEmojiCategories, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"emojiCategories","categories":[{{"@type":"emojiCategory","name":"Smileys","icon":null,"source":{{"@type":"emojiCategorySourcePremium"}},"is_greeting":false}}],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.categories.len(), 1);
        assert_eq!(with_purpose.emoji.categories[0].name, "Smileys");

        // Emoji-pack sets: installed / search / archived (first page
        // replaces, second appends) / trending — none touches the regular
        // sticker panel's slots.
        let set_json = |id: i64, title: &str| {
            format!(
                r#"{{"@type":"stickerSetInfo","id":"{id}","title":"{title}","name":"{title}Sets","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeCustomEmoji"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":false,"size":3,"covers":[]}}"#
            )
        };
        let extra = with_purpose.request(RequestPurpose::GetInstalledEmojiSets, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"stickerSets","total_count":1,"sets":[{}],"@extra":"{}"}}"#,
                set_json(77, "Blob"),
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.installed_sets.len(), 1);
        assert_eq!(with_purpose.emoji.installed_sets[0].id, 77);
        assert!(with_purpose.stickers.sets.is_empty());

        let extra = with_purpose.request(RequestPurpose::SearchEmojiSets, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"stickerSets","total_count":1,"sets":[{}],"@extra":"{}"}}"#,
                set_json(78, "Found"),
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.found_sets.len(), 1);
        assert!(with_purpose.stickers.found_sets.is_empty());

        let extra = with_purpose.request(
            RequestPurpose::Stickers(StickersPurpose::GetArchivedEmojiSets { first_page: true }),
            None,
        );
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"stickerSets","total_count":2,"sets":[{}],"@extra":"{}"}}"#,
                set_json(79, "Old"),
                extra.0
            ),
        );
        let extra = with_purpose.request(
            RequestPurpose::Stickers(StickersPurpose::GetArchivedEmojiSets { first_page: false }),
            None,
        );
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"stickerSets","total_count":2,"sets":[{}],"@extra":"{}"}}"#,
                set_json(80, "Older"),
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.archived_sets.len(), 2);

        let extra = with_purpose.request(RequestPurpose::GetTrendingEmojiSets, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"trendingStickerSets","total_count":1,"is_premium":true,"sets":[{}],"@extra":"{}"}}"#,
                set_json(81, "Trending"),
                extra.0
            ),
        );
        assert_eq!(with_purpose.emoji.trending_sets.len(), 1);
        assert!(with_purpose.emoji.trending_is_premium);
        assert!(with_purpose.stickers.trending.is_empty());

        // A clearRecentEmojiStatuses `ok` drops the recent statuses.
        let extra = with_purpose.request(RequestPurpose::ClearRecentEmojiStatuses, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(with_purpose.emoji.recent_statuses.is_empty());

        // A setEmojiStatus `ok` likewise invalidates recent statuses.
        with_purpose.emoji.recent_statuses = vec![EmojiStatusItem {
            custom_emoji_id: 1,
            expiration_date: 0,
            gift: None,
        }];
        let extra = with_purpose.request(RequestPurpose::SetEmojiStatus, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(with_purpose.emoji.recent_statuses.is_empty());

        // A changeStickerSet (emoji) `ok` drops the installed emoji sets.
        let extra = with_purpose.request(RequestPurpose::ChangeEmojiSet, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(with_purpose.emoji.installed_sets.is_empty());

        // A reorderInstalledStickerSets (emoji) `ok` does the same.
        with_purpose.emoji.installed_sets = vec![with_purpose.emoji.found_sets[0].clone()];
        let extra = with_purpose.request(RequestPurpose::ReorderInstalledEmojiSets, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(with_purpose.emoji.installed_sets.is_empty());
    }
}

#[cfg(test)]
mod keyword_language_tests {
    use super::keyword_language_codes;

    #[test]
    fn typed_script_then_system_language_then_english() {
        assert_eq!(keyword_language_codes("fire", None), ["en"]);
        assert_eq!(
            keyword_language_codes("fire", Some("de_DE.UTF-8")),
            ["de", "en"]
        );
        assert_eq!(
            keyword_language_codes("огонь", Some("en_US.UTF-8")),
            ["ru", "uk", "en"]
        );
        assert_eq!(keyword_language_codes("אש", None), ["he", "en"]);
        assert_eq!(
            keyword_language_codes("火", Some("zh_CN")),
            ["zh", "ja", "en"]
        );
    }

    #[test]
    fn mixed_scripts_ask_for_every_language_once() {
        let codes = keyword_language_codes("fire огонь שלום", Some("ru_RU"));
        assert_eq!(codes, ["ru", "uk", "he", "en"]);
    }
}
