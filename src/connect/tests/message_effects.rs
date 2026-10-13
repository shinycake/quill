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
    // The badge's icon downloads with the chat's thumbnails.
    f.driver.maybe_download_open_thumbs().unwrap();
    assert!(
        f.sent("downloadFile")
            .iter()
            .any(|request| request["file_id"] == 600)
    );
    assert_eq!(f.sent("getMessageEffect").len(), 1);
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
