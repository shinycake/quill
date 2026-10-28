//! Connect-driver tests: an incoming lone emoji plays its interaction
//! effect when read, from the "EmojiAnimations" pack, without telling the
//! partner.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::emoji_animations::REFRESH_MS;
use crate::emoji_interaction::EmojiEffect;
use crate::ids::{AccountKey, ChatId, MessageId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ALICE: i64 = 7;
const GROUP: i64 = -100;
/// Alice's chat is read up to this message.
const READ_TILL: i64 = 1_048_576;

fn sticker_json(file: i32, emoji: &str) -> String {
    format!(
        r#"{{"@type":"sticker","id":"{file}","set_id":"77","width":512,"height":512,"emoji":"{emoji}","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{{"@type":"file","id":{file},"size":7,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false}},"remote":{{"@type":"remoteFile","id":"r{file}","unique_id":"u{file}"}}}}}}"#
    )
}

/// The pack as TDLib answers `searchStickerSet`: two thumbs-up effects and
/// one red heart effect, tagged with their indexes in `emojis`.
fn pack_json(extra: RequestId) -> String {
    let stickers = [
        sticker_json(901, "👍"),
        sticker_json(902, "👍"),
        sticker_json(903, "❤"),
    ];
    let emojis = [r#"["👍","1⃣"]"#, r#"["👍","2⃣"]"#, r#"["❤","1⃣"]"#]
        .map(|list| format!(r#"{{"@type":"emojis","emojis":{list}}}"#));
    format!(
        r#"{{"@type":"stickerSet","@extra":"{}","id":"77","title":"Animated Emoji Interactions","name":"EmojiAnimations","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":false,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{}],"emojis":[{}]}}"#,
        extra.as_extra(),
        stickers.join(","),
        emojis.join(",")
    )
}

/// `messageAnimatedEmoji` for `emoji` in `chat`.
fn emoji_message(chat: i64, id: i64, outgoing: bool, emoji: &str) -> String {
    let sticker = sticker_json(500, emoji);
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"date":1760000000,"is_outgoing":{outgoing},"content":{{"@type":"messageAnimatedEmoji","emoji":"{emoji}","animated_emoji":{{"@type":"animatedEmoji","sticker":{sticker},"sticker_width":512,"sticker_height":512,"fitzpatrick_type":0,"sound":null,"is_regular_emoji":true}}}}}}}}"#
    )
}

fn text_message(chat: i64, id: i64, text: &str) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"date":1760000000,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#
    )
}

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut fixture = Self {
            driver,
            recorder,
            sink,
            seq: AtomicU64::new(0),
            dir,
        };
        fixture.ingest(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#);
        fixture.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{ALICE},"title":"Alice","type":{{"@type":"chatTypePrivate","user_id":7}},"unread_count":3,"last_read_inbox_message_id":{READ_TILL}}}}}"#
        ));
        fixture.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Group","type":{{"@type":"chatTypeBasicGroup","basic_group_id":3}},"unread_count":1,"last_read_inbox_message_id":0}}}}"#
        ));
        fixture.driver.select_chat(ChatId(ALICE)).unwrap();
        fixture
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self, kind: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter(|json| json.contains(&format!("\"{kind}\"")))
            .map(|json| serde_json::from_str(&json).unwrap())
            .collect()
    }

    /// Show `ids` of `chat` as read; returns the pack request, if any.
    fn show(&mut self, chat: i64, ids: &[i64], paused: bool, now_ms: u64) -> Option<RequestId> {
        let ids: Vec<MessageId> = ids.iter().copied().map(MessageId).collect();
        self.driver
            .autoplay_emoji_effects(ChatId(chat), &ids, paused, now_ms)
            .unwrap()
    }

    /// Show nothing yet, then answer the pack request it makes.
    fn load_pack(&mut self) {
        let extra = self.show(ALICE, &[], false, 1_000).expect("pack asked for");
        self.ingest(&pack_json(extra));
    }

    fn effects(&mut self) -> Vec<EmojiEffect> {
        self.driver.session.stickers.interactions.take_pending()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_private_chat_on_screen_loads_the_pack_once() {
    let mut f = Fixture::new();
    let extra = f.show(ALICE, &[], false, 1_000).expect("asked");
    let asked = f.sent("searchStickerSet");
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["name"], "EmojiAnimations");
    assert_eq!(asked[0]["@extra"], extra.as_extra());
    // In flight, then answered: not asked again until the refresh.
    assert_eq!(f.show(ALICE, &[], false, 2_000), None);
    f.ingest(&pack_json(extra));
    assert!(f.driver.session.stickers.animations.is_ready());
    assert_eq!(f.show(ALICE, &[], false, 1_000 + REFRESH_MS - 1), None);
    assert!(f.show(ALICE, &[], false, 1_000 + REFRESH_MS).is_some());
    assert_eq!(f.sent("searchStickerSet").len(), 2);
}

#[test]
fn groups_do_not_load_or_play() {
    let mut f = Fixture::new();
    f.ingest(&emoji_message(GROUP, 3_145_728, false, "👍"));
    assert_eq!(f.show(GROUP, &[3_145_728], false, 1_000), None);
    assert!(f.sent("searchStickerSet").is_empty());
    assert!(f.effects().is_empty());
}

#[test]
fn an_unread_incoming_emoji_plays_an_effect_once_and_tells_no_one() {
    let mut f = Fixture::new();
    f.load_pack();
    let id = READ_TILL + 1_048_576;
    f.ingest(&emoji_message(ALICE, id, false, "👍"));
    f.show(ALICE, &[id], false, 5_000);
    let effects = f.effects();
    assert_eq!(effects.len(), 1);
    let effect = &effects[0];
    assert_eq!(effect.chat_id, ChatId(ALICE));
    assert_eq!(effect.message_id, MessageId(id));
    assert!([901, 902].contains(&effect.sticker.file_id.0));
    // Not the partner's tap: no "watching" goes back for it.
    assert!(!effect.incoming);
    // Shown again: once a session.
    f.show(ALICE, &[id], false, 6_000);
    assert!(f.effects().is_empty());
    assert!(f.sent("clickAnimatedEmojiMessage").is_empty());
    assert!(f.sent("sendChatAction").is_empty());
}

#[test]
fn a_skin_tone_or_another_heart_plays_the_base_effect() {
    let mut f = Fixture::new();
    f.load_pack();
    let tone = READ_TILL + 1_048_576;
    let heart = READ_TILL + 2 * 1_048_576;
    f.ingest(&emoji_message(ALICE, tone, false, "👍🏽"));
    f.ingest(&emoji_message(ALICE, heart, false, "💜"));
    f.show(ALICE, &[tone, heart], false, 5_000);
    let effects = f.effects();
    let by_message = |id: i64| {
        effects
            .iter()
            .find(|e| e.message_id == MessageId(id))
            .map(|e| e.sticker.file_id.0)
    };
    assert!(matches!(by_message(tone), Some(901 | 902)));
    assert_eq!(by_message(heart), Some(903));
}

#[test]
fn read_outgoing_or_non_emoji_messages_do_not_play() {
    let mut f = Fixture::new();
    f.load_pack();
    let read = READ_TILL;
    let outgoing = READ_TILL + 1_048_576;
    let text = READ_TILL + 2 * 1_048_576;
    let no_effect = READ_TILL + 3 * 1_048_576;
    f.ingest(&emoji_message(ALICE, read, false, "👍"));
    f.ingest(&emoji_message(ALICE, outgoing, true, "👍"));
    f.ingest(&text_message(ALICE, text, "👍 nice"));
    f.ingest(&emoji_message(ALICE, no_effect, false, "🎉"));
    f.show(ALICE, &[read, outgoing, text, no_effect], false, 5_000);
    assert!(f.effects().is_empty());
}

#[test]
fn power_saving_keeps_it_still_and_it_can_play_later() {
    let mut f = Fixture::new();
    f.load_pack();
    let id = READ_TILL + 1_048_576;
    f.ingest(&emoji_message(ALICE, id, false, "❤"));
    f.show(ALICE, &[id], true, 5_000);
    assert!(f.effects().is_empty());
    // tdesktop skips the message without marking it played.
    f.show(ALICE, &[id], false, 6_000);
    assert_eq!(f.effects().len(), 1);
}

#[test]
fn large_emoji_off_shows_text_and_plays_nothing() {
    let mut f = Fixture::new();
    f.load_pack();
    f.driver.session.settings.media_prefs.big_emoji = false;
    let id = READ_TILL + 1_048_576;
    f.ingest(&emoji_message(ALICE, id, false, "👍"));
    f.show(ALICE, &[id], false, 5_000);
    assert!(f.effects().is_empty());
}

#[test]
fn an_emoji_read_before_the_pack_arrives_plays_when_it_does() {
    let mut f = Fixture::new();
    let id = READ_TILL + 1_048_576;
    f.ingest(&emoji_message(ALICE, id, false, "👍"));
    let extra = f.show(ALICE, &[id], false, 1_000).expect("pack asked for");
    assert!(f.effects().is_empty());
    f.ingest(&pack_json(extra));
    let effects = f.effects();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].message_id, MessageId(id));
    assert!(f.sent("clickAnimatedEmojiMessage").is_empty());
}

#[test]
fn a_failed_pack_request_drops_the_wait_and_retries_later() {
    let mut f = Fixture::new();
    let id = READ_TILL + 1_048_576;
    f.ingest(&emoji_message(ALICE, id, false, "👍"));
    let extra = f.show(ALICE, &[id], false, 1_000).expect("pack asked for");
    f.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"STICKERSET_INVALID"}}"#,
        extra.as_extra()
    ));
    assert!(!f.driver.session.stickers.animations.is_ready());
    assert_eq!(f.show(ALICE, &[], false, 2_000), None);
    assert!(f.show(ALICE, &[], false, 1_000 + REFRESH_MS).is_some());
    assert!(f.effects().is_empty());
}
