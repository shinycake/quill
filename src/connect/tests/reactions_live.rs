//! B11: live sticker and reaction updates, quick reaction, reactor tabs,
//! keyword emoji search and greeting stickers (recorded TDLib JSON).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, FileId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::StickersPurpose;
use crate::state::{Audience, ReactionChoice, RequestPurpose, StickerTab};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ReactionType;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn driver() -> (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (driver, recorder, sink, seq)
}

fn feed(
    driver: &mut ConnectDriver<Recorder>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

fn count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| serde_json::from_str::<Value>(json).is_ok_and(|v| v["@type"] == type_name))
        .count()
}

fn sticker(file: i32) -> crate::telegram::envelope::StickerItem {
    crate::telegram::envelope::StickerItem {
        custom_emoji_id: None,
        id: i64::from(file),
        set_id: 1,
        emoji: String::new(),
        width: 512,
        height: 512,
        format: crate::telegram::envelope::StickerFormat::Webp,
        file_id: FileId(file),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
    }
}

#[test]
fn other_device_changes_refetch_the_loaded_lists() {
    let (mut driver, recorder, sink, seq) = driver();
    // Nothing loaded and no panel open: updates fetch nothing.
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateRecentStickers","is_attached":false,"sticker_ids":[1]}"#,
    );
    assert_eq!(count(&recorder, "getRecentStickers"), 0);

    driver.session.stickers.stickers.recent = vec![sticker(1)];
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateRecentStickers","is_attached":false,"sticker_ids":[1,2]}"#,
    );
    assert_eq!(count(&recorder, "getRecentStickers"), 1);
    // Attached-sticker recents are not shown in the panel.
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateRecentStickers","is_attached":true,"sticker_ids":[]}"#,
    );
    assert_eq!(count(&recorder, "getRecentStickers"), 1);

    driver.session.stickers.stickers.favorites = vec![sticker(3)];
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateFavoriteStickers","sticker_ids":[3,4]}"#,
    );
    assert_eq!(count(&recorder, "getFavoriteStickers"), 1);

    // Trending refetches only while its tab is showing.
    let regular = r#"{"@type":"updateTrendingStickerSets","sticker_type":{"@type":"stickerTypeRegular"},"sticker_sets":{"@type":"trendingStickerSets","total_count":0,"sets":[],"is_premium":false}}"#;
    feed(&mut driver, &seq, &sink, regular);
    assert_eq!(count(&recorder, "getTrendingStickerSets"), 0);
    driver.session.stickers.stickers.open = true;
    driver.session.stickers.stickers.tab = StickerTab::Trending;
    feed(&mut driver, &seq, &sink, regular);
    assert_eq!(count(&recorder, "getTrendingStickerSets"), 1);
}

#[test]
fn default_reaction_is_kept_and_set() {
    let (mut driver, recorder, sink, seq) = driver();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateDefaultReactionType","reaction_type":{"@type":"reactionTypeEmoji","emoji":"❤"}}"#,
    );
    assert_eq!(
        driver.session.stickers.default_reaction,
        Some(ReactionChoice::Emoji("❤".into()))
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateActiveEmojiReactions","emojis":["👍","❤"]}"#,
    );
    assert_eq!(driver.session.stickers.active_reactions, vec!["👍", "❤"]);

    driver
        .set_default_reaction(&ReactionChoice::Emoji("🔥".into()))
        .unwrap();
    let sent = sent_request(&recorder, "setDefaultReactionType");
    assert_eq!(sent["reaction_type"]["emoji"], "🔥");
    assert_eq!(
        driver.session.stickers.default_reaction,
        Some(ReactionChoice::Emoji("🔥".into()))
    );
}

#[test]
fn picker_options_are_refetched_when_reactions_change() {
    let (mut driver, recorder, sink, seq) = driver();
    let chat = ChatId(11);
    let message = MessageId(101);
    driver.session.stickers.message_reaction_options = Some(crate::state::MessageReactionOptions {
        chat_id: chat,
        message_id: message,
        top: vec![ReactionChoice::Emoji("👍".into())],
        recent: Vec::new(),
        popular: Vec::new(),
        allow_custom_emoji: false,
    });
    // Another chat's change leaves the options alone.
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateChatAvailableReactions","chat_id":99,"available_reactions":{"@type":"chatAvailableReactionsAll","max_reaction_count":1}}"#,
    );
    assert_eq!(count(&recorder, "getMessageAvailableReactions"), 0);
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateChatAvailableReactions","chat_id":11,"available_reactions":{"@type":"chatAvailableReactionsAll","max_reaction_count":1}}"#,
    );
    let sent = sent_request(&recorder, "getMessageAvailableReactions");
    assert_eq!(sent["message_id"], 101);
    assert!(driver.session.stickers.message_reaction_options.is_none());
}

#[test]
fn reactor_tabs_page_per_reaction() {
    let (mut driver, recorder, sink, seq) = driver();
    let chat = ChatId(11);
    let message = MessageId(101);
    driver.session.begin_message_audience(chat, message);
    let heart = ReactionType::emoji("❤");
    driver
        .fetch_reactors_tab(chat, message, Some(&heart), false)
        .unwrap();
    let sent = sent_request(&recorder, "getMessageAddedReactions");
    assert_eq!(sent["reaction_type"]["emoji"], "❤");
    assert_eq!(sent["offset"], "");
    // Asking again while loading sends nothing.
    driver
        .fetch_reactors_tab(chat, message, Some(&heart), false)
        .unwrap();
    assert_eq!(count(&recorder, "getMessageAddedReactions"), 1);

    let page = |offset: &str, extra: &str| {
        format!(
            r#"{{"@type":"addedReactions","total_count":3,"reactions":[{{"@type":"addedReaction","type":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"sender_id":{{"@type":"messageSenderUser","user_id":7}},"is_outgoing":false,"date":5}}],"next_offset":"{offset}","@extra":"{extra}"}}"#
        )
    };
    feed(
        &mut driver,
        &seq,
        &sink,
        &page("o1", sent["@extra"].as_str().unwrap()),
    );
    let key = crate::state::reaction_filter_key(&heart);
    let audience = driver.session.messages.message_audience.as_ref().unwrap();
    assert_eq!(audience.filtered[&key].ready().unwrap().reactions.len(), 1);

    // The next page appends and carries the new offset.
    driver
        .fetch_reactors_tab(chat, message, Some(&heart), true)
        .unwrap();
    let more = sent_request(&recorder, "getMessageAddedReactions");
    assert_eq!(more["offset"], "o1");
    feed(
        &mut driver,
        &seq,
        &sink,
        &page("", more["@extra"].as_str().unwrap()),
    );
    let audience = driver.session.messages.message_audience.as_ref().unwrap();
    let ready = audience.filtered[&key].ready().unwrap();
    assert_eq!(ready.reactions.len(), 2);
    assert!(ready.next_offset.is_empty());
    assert!(audience.more_loading.is_empty());
    assert!(matches!(audience.reactions, Audience::NotAsked));
    // Nothing more to load.
    driver
        .fetch_reactors_tab(chat, message, Some(&heart), true)
        .unwrap();
    assert_eq!(count(&recorder, "getMessageAddedReactions"), 2);
}

#[test]
fn removing_a_recent_sticker_drops_it_at_once() {
    let (mut driver, recorder, _sink, _seq) = driver();
    driver.session.stickers.stickers.recent = vec![sticker(1), sticker(2)];
    driver.remove_recent_sticker(FileId(1)).unwrap();
    let sent = sent_request(&recorder, "removeRecentSticker");
    assert_eq!(sent["sticker"]["id"], 1);
    assert_eq!(sent["is_attached"], false);
    assert_eq!(driver.session.stickers.stickers.recent.len(), 1);
    assert!(driver.remove_recent_sticker(FileId(0)).is_err());
}

#[test]
fn keyword_emoji_search_uses_the_typed_language() {
    let (mut driver, recorder, sink, seq) = driver();
    driver.search_keyword_emojis("огонь").unwrap();
    let sent = sent_request(&recorder, "getKeywordEmojis");
    assert_eq!(sent["text"], "огонь");
    let codes: Vec<&str> = sent["input_language_codes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(&codes[..2], ["ru", "uk"]);
    assert!(codes.contains(&"en"));
    let body = format!(
        r#"{{"@type":"emojis","emojis":["🔥"],"@extra":"{}"}}"#,
        sent["@extra"].as_str().unwrap()
    );
    feed(&mut driver, &seq, &sink, &body);
    assert_eq!(driver.session.stickers.emoji.keyword_emojis, vec!["🔥"]);
    // A cleared query forgets the matches.
    driver.search_keyword_emojis("").unwrap();
    assert!(driver.session.stickers.emoji.keyword_emojis.is_empty());
}

#[test]
fn greeting_and_attached_stickers_load() {
    let (mut driver, recorder, sink, seq) = driver();
    driver.fetch_greeting_stickers().unwrap();
    // Asked once per session, whatever the answer.
    driver.fetch_greeting_stickers().unwrap();
    assert_eq!(count(&recorder, "getGreetingStickers"), 1);
    let sent = sent_request(&recorder, "getGreetingStickers");
    let body = format!(
        r#"{{"@type":"stickers","stickers":[{{"@type":"sticker","id":"9","set_id":"5","width":512,"height":512,"emoji":"👋","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":null}}],"@extra":"{}"}}"#,
        sent["@extra"].as_str().unwrap()
    );
    feed(&mut driver, &seq, &sink, &body);
    assert_eq!(driver.session.stickers.stickers.greeting.len(), 1);

    driver.fetch_attached_sticker_sets(FileId(42)).unwrap();
    let sent = sent_request(&recorder, "getAttachedStickerSets");
    assert_eq!(sent["file_id"], 42);
    let body = format!(
        r#"{{"@type":"stickerSets","total_count":1,"sets":[{{"@type":"stickerSetInfo","id":"77","title":"Fun","name":"fun","size":2}}],"@extra":"{}"}}"#,
        sent["@extra"].as_str().unwrap()
    );
    feed(&mut driver, &seq, &sink, &body);
    // The first attached set opens in the sticker set dialog.
    assert_eq!(sent_request(&recorder, "getStickerSet")["set_id"], "77");
    assert!(
        driver
            .session
            .requests
            .has_purpose(RequestPurpose::Stickers(StickersPurpose::ViewStickerSet {
                set_id: 77
            }))
    );
}
