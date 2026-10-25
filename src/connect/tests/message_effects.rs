//! Connect-driver tests: message effects (`getMessageEffect`, the send
//! menu's picker loading and a send with an effect).
use super::super::*;
use super::*;
use crate::composer::{ComposerSnapshot, SendOptions};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, FileId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

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
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"permissions":{"@type":"chatPermissions","can_send_basic_messages":true}}}"#,
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

    fn sent(&self, kind: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter(|json| json.contains(&format!("\"@type\":\"{kind}\"")))
            .map(|json| serde_json::from_str(&json).unwrap())
            .collect()
    }

    /// Answer the `getMessageEffect` request `request` with an emoji
    /// reaction effect whose static icon is file `icon`.
    fn answer_effect(&mut self, request: &Value, icon: i32) {
        let extra = request["@extra"].as_str().unwrap().to_string();
        let id = request["effect_id"].as_str().unwrap().to_string();
        let sticker = |file: i32, format: &str| {
            format!(
                r#"{{"@type":"sticker","id":"{file}","set_id":"0","width":100,"height":100,"emoji":"x","format":{{"@type":"{format}"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{{"@type":"file","id":{file},"size":9,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false}},"remote":{{"@type":"remoteFile","id":"r{file}","unique_id":"u{file}"}}}}}}"#
            )
        };
        let json = format!(
            r#"{{"@type":"messageEffect","@extra":"{extra}","id":"{id}","emoji":"🎉","is_premium":false,"static_icon":{},"type":{{"@type":"messageEffectTypeEmojiReaction","select_animation":{},"effect_animation":{}}}}}"#,
            sticker(icon, "stickerFormatWebp"),
            sticker(icon + 1, "stickerFormatTgs"),
            sticker(icon + 2, "stickerFormatTgs"),
        );
        self.ingest(&json);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const EFFECT: i64 = 5_104_841_245_755_180_586;

#[test]
fn a_message_with_an_effect_asks_for_it_once() {
    let mut f = Fixture::new();
    f.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":50,"chat_id":7,"is_outgoing":false,"date":1,"effect_id":"{EFFECT}","content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}}}"#
    ));
    f.driver.maybe_download_open_thumbs().unwrap();
    let asked = f.sent("getMessageEffect");
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["effect_id"], EFFECT.to_string());
    // Asked once while the answer is on its way.
    f.driver.maybe_download_open_thumbs().unwrap();
    assert_eq!(f.sent("getMessageEffect").len(), 1);

    f.answer_effect(&asked[0], 600);
    let effect = f
        .driver
        .session
        .stickers
        .effects
        .get(EFFECT)
        .expect("known");
    assert_eq!(effect.icon_file(), Some(FileId(600)));
    // The badge's icon and the effect's animation download with the
    // chat's thumbnails, so the effect is ready when the message shows
    // (tdesktop `preloadEffect`); the select animation is not needed.
    f.driver.maybe_download_open_thumbs().unwrap();
    let downloads = f.sent("downloadFile");
    let asked_for = |file: i32| downloads.iter().any(|request| request["file_id"] == file);
    assert!(asked_for(600));
    assert!(asked_for(602));
    assert!(!asked_for(601));
    assert_eq!(f.sent("getMessageEffect").len(), 1);
}

#[test]
fn an_effect_left_to_play_follows_the_message_to_its_final_id() {
    let mut f = Fixture::new();
    let new_message = |id: i64, outgoing: bool, state: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            message_json(id, outgoing, state)
        )
    };
    let unwatched = |f: &Fixture, id: i64| {
        f.driver
            .session
            .stickers
            .effects
            .watch
            .is_unwatched((7, id))
    };
    // Sent from here: the pending message has an effect to play, and keeps
    // it under its final id.
    let pending = r#","sending_state":{"@type":"messageSendingStatePending","sending_id":0}"#;
    f.ingest(&new_message(-3, true, pending));
    assert!(unwatched(&f, -3));
    f.ingest(&format!(
        r#"{{"@type":"updateMessageSendSucceeded","old_message_id":-3,"message":{}}}"#,
        message_json(70, true, "")
    ));
    assert!(!unwatched(&f, -3));
    assert!(unwatched(&f, 70));
    // Sent from another device: it played there.
    f.ingest(&new_message(71, true, ""));
    assert!(!unwatched(&f, 71));
    // Incoming and unread: still to play after it is read elsewhere.
    f.ingest(&new_message(72, false, ""));
    f.ingest(
        r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":72,"unread_count":0}"#,
    );
    f.ingest(&new_message(72, false, ""));
    assert!(unwatched(&f, 72));
    // Incoming and already read when it arrives.
    f.ingest(&new_message(60, false, ""));
    assert!(!unwatched(&f, 60));
}

fn message_json(id: i64, outgoing: bool, sending_state: &str) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":7,"is_outgoing":{outgoing},"date":1,"effect_id":"{EFFECT}"{sending_state},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}"#
    )
}

/// `updateNewMessage` for an incoming message sent with `effect`.
fn live_message(chat: i64, id: i64, effect: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"is_outgoing":false,"date":1,"effect_id":"{effect}","content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}}}"#
    )
}

/// The effect asked for with `getMessageEffect`, if it was.
fn asked_for(f: &Fixture, effect: i64) -> Option<Value> {
    f.sent("getMessageEffect").into_iter().find(|request| {
        request["effect_id"].as_str().and_then(|id| id.parse().ok()) == Some(effect)
    })
}

#[test]
fn a_live_message_gets_its_effect_ready_in_the_main_window_and_a_chat_window() {
    let mut f = Fixture::new();
    f.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0,"permissions":{"@type":"chatPermissions","can_send_basic_messages":true}}}"#,
    );
    f.driver.open_chat_window(ChatId(8)).unwrap();
    let in_window: i64 = 5_107_584_321_108_051_014;
    for (chat, id, effect, icon) in [(7, 90, EFFECT, 800), (8, 91, in_window, 810)] {
        f.ingest(&live_message(chat, id, effect));
        assert!(
            f.driver
                .session
                .stickers
                .effects
                .watch
                .is_unwatched((chat, id))
        );
        let request = asked_for(&f, effect).expect("the effect is described");
        f.answer_effect(&request, icon);
        f.ingest(&format!(
            r#"{{"@type":"updateFile","file":{{"@type":"file","id":{icon},"size":9,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false}},"remote":{{"@type":"remoteFile","id":"r{icon}","unique_id":"u{icon}"}}}}}}"#
        ));
        let downloads = f.sent("downloadFile");
        for file in [icon, icon + 2] {
            assert!(
                downloads.iter().any(|request| request["file_id"] == file),
                "chat {chat}: file {file} downloads"
            );
        }
    }
}

#[test]
fn an_effect_that_is_gone_is_not_asked_for_again() {
    let mut f = Fixture::new();
    f.driver.load_message_effects([77]).unwrap();
    let asked = f.sent("getMessageEffect");
    assert_eq!(asked.len(), 1);
    f.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":404,"message":"Not Found"}}"#,
        asked[0]["@extra"].as_str().unwrap()
    ));
    assert!(f.driver.session.stickers.effects.is_gone(77));
    f.driver.load_message_effects([77]).unwrap();
    assert_eq!(f.sent("getMessageEffect").len(), 1);
}

#[test]
fn the_picker_describes_every_offered_effect_then_fetches_their_icons() {
    let mut f = Fixture::new();
    f.ingest(
        r#"{"@type":"updateAvailableMessageEffects","reaction_effect_ids":["11","12"],"sticker_effect_ids":["21"]}"#,
    );
    f.driver.load_effect_picker().unwrap();
    let asked = f.sent("getMessageEffect");
    let ids: Vec<&str> = asked
        .iter()
        .map(|request| request["effect_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["11", "12", "21"]);
    for (request, icon) in asked.iter().zip([700, 710, 720]) {
        f.answer_effect(request, icon);
    }
    f.driver.load_effect_picker().unwrap();
    // No second description request; each icon is fetched once.
    assert_eq!(f.sent("getMessageEffect").len(), 3);
    let downloads: Vec<Value> = f.sent("downloadFile");
    for icon in [700, 710, 720] {
        assert_eq!(
            downloads
                .iter()
                .filter(|request| request["file_id"] == icon)
                .count(),
            1
        );
    }
    f.driver.load_effect_picker().unwrap();
    assert_eq!(f.sent("downloadFile").len(), downloads.len());
}

#[test]
fn a_send_with_an_effect_carries_its_id() {
    let mut f = Fixture::new();
    let snapshot = ComposerSnapshot::capture(ChatId(7), f.driver.session.view_generation, "boo")
        .with_send_options(SendOptions {
            effect_id: EFFECT,
            ..SendOptions::default()
        });
    f.driver.send_text_snapshot(&snapshot).unwrap();
    let sent = f.sent("sendMessage");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_id"], 7);
    assert_eq!(sent[0]["options"]["effect_id"], EFFECT.to_string());
    assert_eq!(sent[0]["input_message_content"]["text"]["text"], "boo");
}
