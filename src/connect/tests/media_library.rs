//! Connect-driver tests: the emoji/sticker panel library (installed lists,
//! lazy set contents, cell downloads) and message reaction options.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, FileId, MessageId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{MAX_LIBRARY_LOADS, ReactionChoice, Session};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Harness {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Harness {
    fn ready() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut h = Harness {
            driver,
            recorder,
            seq: AtomicU64::new(0),
            sink,
            _dir: dir,
        };
        h.ingest(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#);
        h
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self, ty: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains(&format!("\"@type\":\"{ty}\"")))
            .map(|j| serde_json::from_str(&j).unwrap())
            .collect()
    }

    fn extra_of(value: &Value) -> RequestId {
        RequestId(value["@extra"].as_str().unwrap().parse().unwrap())
    }
}

fn sticker_json(id: i64, set_id: i64, file: i64) -> String {
    format!(
        r#"{{"@type":"sticker","id":"{id}","set_id":"{set_id}","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular"}},"thumbnail":null,"sticker":{{"@type":"file","id":{file},"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r{file}","unique_id":"u{file}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}}}}"#
    )
}

#[test]
fn opening_the_panel_requests_both_set_lists_recent_and_favorites_once() {
    let mut h = Harness::ready();
    h.driver.open_media_panel().unwrap();
    h.driver.open_media_panel().unwrap();
    let installed = h.sent("getInstalledStickerSets");
    assert_eq!(installed.len(), 2, "regular + custom emoji, no duplicates");
    assert!(
        installed
            .iter()
            .any(|v| v["sticker_type"]["@type"] == "stickerTypeCustomEmoji")
    );
    assert_eq!(h.sent("getRecentStickers").len(), 1);
    assert_eq!(h.sent("getFavoriteStickers").len(), 1);
}

#[test]
fn library_sets_load_lazily_with_a_cap_and_failures_are_not_retried() {
    let mut h = Harness::ready();
    let wanted: Vec<i64> = (1..=6).collect();
    h.driver.ensure_library_sets(&wanted).unwrap();
    let requests = h.sent("getStickerSet");
    assert_eq!(requests.len(), MAX_LIBRARY_LOADS);
    // Asking again sends nothing while the slots are full.
    h.driver.ensure_library_sets(&wanted).unwrap();
    assert_eq!(h.sent("getStickerSet").len(), MAX_LIBRARY_LOADS);

    let first = Harness::extra_of(&requests[0]);
    h.ingest(&format!(
        r#"{{"@type":"stickerSet","@extra":"{}","id":"1","title":"One","name":"one","stickers":[{}],"emojis":[]}}"#,
        first.0,
        sticker_json(100, 1, 900)
    ));
    let loaded = &h.driver.session.media_library.set_stickers[&1];
    assert_eq!(loaded.len(), 1);
    assert!(h.driver.session.files.contains_key(&900));

    let second = Harness::extra_of(&requests[1]);
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"STICKERSET_INVALID"}}"#,
        second.0
    ));
    assert!(h.driver.session.media_library.failed.contains(&2));
    // Two slots freed: sets 5 and 6 go out; 1 (loaded) and 2 (failed) don't.
    h.driver.ensure_library_sets(&wanted).unwrap();
    let ids: Vec<String> = h
        .sent("getStickerSet")
        .iter()
        .map(|v| v["set_id"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(ids.len(), MAX_LIBRARY_LOADS + 2);
    assert!(ids.ends_with(&["5".to_string(), "6".to_string()]));

    // Visible cells download their display files once.
    h.driver.ensure_media_files(&[FileId(900)]).unwrap();
    h.driver.ensure_media_files(&[FileId(900)]).unwrap();
    assert_eq!(h.sent("downloadFile").len(), 1);
}

#[test]
fn message_reaction_options_parse_and_custom_emoji_reactions_toggle() {
    let mut h = Harness::ready();
    h.ingest(r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#);
    h.ingest(r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#);
    h.ingest(r#"{"@type":"updateNewMessage","message":{"@type":"message","id":50,"chat_id":7,"is_outgoing":false,"date":1700000000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#);

    let extra = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    let sent = &h.sent("getMessageAvailableReactions")[0];
    assert_eq!(sent["row_size"], REACTION_STRIP_SIZE);
    h.ingest(&format!(
        r#"{{"@type":"availableReactions","@extra":"{}","top_reactions":[{{"@type":"availableReaction","type":{{"@type":"reactionTypeEmoji","emoji":"👍"}},"needs_premium":false}},{{"@type":"availableReaction","type":{{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"77"}},"needs_premium":true}}],"recent_reactions":[{{"@type":"availableReaction","type":{{"@type":"reactionTypeEmoji","emoji":"👍"}},"needs_premium":false}}],"popular_reactions":[{{"@type":"availableReaction","type":{{"@type":"reactionTypeEmoji","emoji":"🔥"}},"needs_premium":false}}],"allow_custom_emoji":true,"are_tags":false,"unavailability_reason":null}}"#,
        extra.0
    ));
    let options = h.driver.session.message_reaction_options.clone().unwrap();
    assert_eq!(options.message_id, MessageId(50));
    assert!(options.allow_custom_emoji);
    assert_eq!(
        options.all(),
        vec![
            ReactionChoice::Emoji("👍".into()),
            ReactionChoice::CustomEmoji(77),
            ReactionChoice::Emoji("🔥".into()),
        ]
    );
    assert!(h.driver.session.story_available_reactions.is_none());

    h.driver
        .toggle_reaction_choice(ChatId(7), MessageId(50), &ReactionChoice::CustomEmoji(77))
        .unwrap();
    let add = h.sent("addMessageReaction");
    assert_eq!(add[0]["reaction_type"]["@type"], "reactionTypeCustomEmoji");
    assert_eq!(add[0]["reaction_type"]["custom_emoji_id"], "77");

    // Once it's ours, the same choice removes it.
    h.ingest(r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"77"},"total_count":1,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#);
    h.driver
        .toggle_reaction_choice(ChatId(7), MessageId(50), &ReactionChoice::CustomEmoji(77))
        .unwrap();
    assert_eq!(h.sent("removeMessageReaction").len(), 1);
}
