use crate::state::{RequestPurpose, Session};
use crate::telegram::envelope::{EnvelopePayload, ParsedFile, StickerItem, StickerSetInfo};
use crate::telegram::envelope_emoji::{EmojiCategory, EmojiKeyword, EmojiStatusItem};

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
    /// Slice S10: last `getCustomEmojiStickers` answer.
    pub custom_emoji_stickers: Vec<StickerItem>,
    /// Slice S10: `searchEmojis` results for the picker.
    pub keyword_results: Vec<EmojiKeyword>,
    /// Slice S10: `getEmojiCategories` rows for the picker.
    pub categories: Vec<EmojiCategory>,
}

impl Session {
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
            EnvelopePayload::EmojiStatuses { statuses } => {
                self.accept_emoji_statuses(purpose, statuses);
            }
            EnvelopePayload::EmojiStatusCustomEmojis { custom_emoji_ids } => {
                self.accept_emoji_status_ids(purpose, custom_emoji_ids);
            }
            EnvelopePayload::AnimatedEmoji { sticker, files } => {
                self.accept_animated_emoji(purpose, sticker, files);
            }
            EnvelopePayload::EmojiKeywords { keywords } => {
                self.accept_emoji_keywords(purpose, keywords);
            }
            EnvelopePayload::EmojiCategories { categories, files } => {
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
        self.emoji.custom_emoji_stickers = stickers;
    }

    /// Slice S10: an emoji-status or emoji-set mutation succeeded — drop the
    /// affected cache so the next fetch shows the server-confirmed state.
    pub fn invalidate_emoji_caches(&mut self, purpose: Option<RequestPurpose>) {
        if purpose == Some(RequestPurpose::ChangeEmojiSet) {
            self.emoji.mutation_failed = false;
        }
        if matches!(
            purpose,
            Some(RequestPurpose::SetEmojiStatus | RequestPurpose::ClearRecentEmojiStatuses)
        ) {
            self.emoji.recent_statuses.clear();
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
    use crate::ids::AccountKey;
    use crate::telegram::client::copy_and_parse;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

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
            RequestPurpose::GetArchivedEmojiSets { first_page: true },
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
            RequestPurpose::GetArchivedEmojiSets { first_page: false },
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
