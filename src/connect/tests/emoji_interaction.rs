//! Connect-driver tests: animated emoji taps and the dice list.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const STICKER: &str = r#"{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"👍","format":{"@type":"stickerFormatTgs"},"full_type":{"@type":"stickerFullTypeRegular","premium_animation":null},"thumbnail":null,"sticker":{"@type":"file","id":42,"size":7,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false},"remote":{"@type":"remoteFile","id":"r","unique_id":"u"}}}"#;

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
        for json in [
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":-100,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"unread_count":0}}"#,
        ] {
            fixture.ingest(json);
        }
        fixture.driver.select_chat(ChatId(7)).unwrap();
        fixture
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn last(&self, kind: &str) -> Option<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains(&format!("\"{kind}\"")))
            .map(|json| serde_json::from_str(&json).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn tapping_in_a_private_chat_asks_for_the_effect_and_queues_the_answer() {
    let mut f = Fixture::new();
    let extra = f
        .driver
        .click_animated_emoji(ChatId(7), MessageId(4_194_304))
        .unwrap()
        .expect("sent");
    let sent = f.last("clickAnimatedEmojiMessage").expect("request");
    assert_eq!(sent["chat_id"], 7);
    assert_eq!(sent["message_id"], 4_194_304);
    let answer = STICKER.replacen(
        "\"sticker\",",
        &format!("\"sticker\",\"@extra\":\"{}\",", extra.as_extra()),
        1,
    );
    f.ingest(&answer);
    let effects = f.driver.session.stickers.interactions.take_pending();
    assert_eq!(effects.len(), 1);
    assert!(!effects[0].incoming);
    assert_eq!(effects[0].message_id, MessageId(4_194_304));
}

#[test]
fn a_404_answer_leaves_the_usual_animation() {
    let mut f = Fixture::new();
    let extra = f
        .driver
        .click_animated_emoji(ChatId(7), MessageId(8_388_608))
        .unwrap()
        .unwrap();
    f.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":404,"message":"Not Found"}}"#,
        extra.as_extra()
    ));
    assert!(
        f.driver
            .session
            .stickers
            .interactions
            .take_pending()
            .is_empty()
    );
}

#[test]
fn groups_and_unsent_messages_do_not_ask() {
    let mut f = Fixture::new();
    assert_eq!(
        f.driver
            .click_animated_emoji(ChatId(-100), MessageId(1_048_576))
            .unwrap(),
        None
    );
    assert!(
        f.driver
            .click_animated_emoji(ChatId(7), MessageId(0))
            .is_err()
    );
    assert!(f.last("clickAnimatedEmojiMessage").is_none());
}

#[test]
fn an_incoming_effect_tells_the_partner_once_a_second() {
    let mut f = Fixture::new();
    assert!(
        f.driver
            .send_watching_animations(ChatId(7), "👍", 10)
            .unwrap()
            .is_some()
    );
    let sent = f.last("sendChatAction").expect("action");
    assert_eq!(sent["action"]["@type"], "chatActionWatchingAnimations");
    assert_eq!(sent["action"]["emoji"], "👍");
    assert_eq!(
        f.driver
            .send_watching_animations(ChatId(7), "👍", 600)
            .unwrap(),
        None
    );
}

#[test]
fn only_emoji_in_the_server_dice_list_roll() {
    let mut f = Fixture::new();
    f.ingest(r#"{"@type":"updateDiceEmojis","emojis":["🎲","🎳"]}"#);
    let send = |f: &mut Fixture, text: &str| {
        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(7),
            f.driver.session.view_generation,
            text,
        );
        f.driver.send_text_snapshot(&snap).unwrap();
        f.last("sendMessage").unwrap()["input_message_content"].clone()
    };
    let bowling = send(&mut f, "🎳");
    assert_eq!(bowling["@type"], "inputMessageDice");
    assert_eq!(bowling["emoji"], "🎳");
    // The server dropped football from its list.
    let football = send(&mut f, "⚽");
    assert_eq!(football["@type"], "inputMessageText");
}
