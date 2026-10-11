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
    session.stickers.emoji.installed_sets = vec![pack(1), pack(2), pack(3)];
    session.remember_emoji_pack_usage(&[10]);
    session
        .stickers
        .emoji
        .custom_emoji_stickers
        .push(sticker(20, 2));
    session.remember_emoji_pack_usage(&[20]);
    assert_eq!(session.settings.media_prefs.recent_emoji_packs, vec![2]);
    session
        .stickers
        .emoji
        .custom_emoji_stickers
        .push(sticker(10, 1));
    session.remember_emoji_pack_usage(&[]);
    assert_eq!(
        session
            .ordered_emoji_packs()
            .iter()
            .map(|set| set.id)
            .collect::<Vec<_>>(),
        vec![2, 1, 3]
    );
    session.settings.media_prefs.dynamic_emoji_pack_order = false;
    assert_eq!(
        session
            .ordered_emoji_packs()
            .iter()
            .map(|set| set.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    session.settings.media_prefs.dynamic_emoji_pack_order = true;
    session.remember_emoji_pack_usage(&[10, 10, -1]);
    assert_eq!(session.settings.media_prefs.recent_emoji_packs, vec![1, 2]);
    assert_eq!(
        session.settings.media_prefs.recent_custom_emoji_ids,
        vec![10, 20]
    );
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
    session
        .stickers
        .emoji
        .status_resolution_attempted
        .insert(12345);
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
    session
        .stickers
        .emoji
        .custom_emoji_stickers
        .push(StickerItem {
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
    assert_eq!(with_purpose.stickers.emoji.recent_statuses.len(), 1);
    assert_eq!(
        with_purpose.stickers.emoji.recent_statuses[0].custom_emoji_id,
        12345
    );
    assert_eq!(
        with_purpose.stickers.emoji.recent_statuses[0].expiration_date,
        3600
    );

    // A stray emojiStatuses (no matching purpose) is ignored.
    let (mut without_purpose, sink2) = session();
    let seq2 = AtomicU64::new(0);
    apply_json(
        &mut without_purpose,
        &seq2,
        &sink2,
        r#"{"@type":"emojiStatuses","emoji_statuses":[]}"#,
    );
    assert!(without_purpose.stickers.emoji.recent_statuses.is_empty());

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
    assert_eq!(with_purpose.stickers.emoji.themed_status_ids, vec![11, 22]);
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
    assert_eq!(with_purpose.stickers.emoji.default_status_ids, vec![33]);

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
            .stickers
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
    assert_eq!(with_purpose.stickers.emoji.custom_emoji_stickers.len(), 1);
    assert!(with_purpose.stickers.stickers.favorites.is_empty());

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
    assert_eq!(with_purpose.stickers.emoji.keyword_results.len(), 1);
    assert_eq!(
        with_purpose.stickers.emoji.keyword_results[0].keyword,
        "fire"
    );

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
    assert_eq!(with_purpose.stickers.emoji.categories.len(), 1);
    assert_eq!(with_purpose.stickers.emoji.categories[0].name, "Smileys");

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
    assert_eq!(with_purpose.stickers.emoji.installed_sets.len(), 1);
    assert_eq!(with_purpose.stickers.emoji.installed_sets[0].id, 77);
    assert!(with_purpose.stickers.stickers.sets.is_empty());

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
    assert_eq!(with_purpose.stickers.emoji.found_sets.len(), 1);
    assert!(with_purpose.stickers.stickers.found_sets.is_empty());

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
    assert_eq!(with_purpose.stickers.emoji.archived_sets.len(), 2);

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
    assert_eq!(with_purpose.stickers.emoji.trending_sets.len(), 1);
    assert!(with_purpose.stickers.emoji.trending_is_premium);
    assert!(with_purpose.stickers.stickers.trending.is_empty());

    // A clearRecentEmojiStatuses `ok` drops the recent statuses.
    let extra = with_purpose.request(RequestPurpose::ClearRecentEmojiStatuses, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(with_purpose.stickers.emoji.recent_statuses.is_empty());

    // A setEmojiStatus `ok` likewise invalidates recent statuses.
    with_purpose.stickers.emoji.recent_statuses = vec![EmojiStatusItem {
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
    assert!(with_purpose.stickers.emoji.recent_statuses.is_empty());

    // A changeStickerSet (emoji) `ok` drops the installed emoji sets.
    let extra = with_purpose.request(RequestPurpose::ChangeEmojiSet, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(with_purpose.stickers.emoji.installed_sets.is_empty());

    // A reorderInstalledStickerSets (emoji) `ok` does the same.
    with_purpose.stickers.emoji.installed_sets =
        vec![with_purpose.stickers.emoji.found_sets[0].clone()];
    let extra = with_purpose.request(RequestPurpose::ReorderInstalledEmojiSets, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(with_purpose.stickers.emoji.installed_sets.is_empty());
}
