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
    let loaded = &h.driver.session.media.media_library.set_stickers[&1];
    assert_eq!(loaded.len(), 1);
    assert!(h.driver.session.media.files.contains_key(&900));

    let second = Harness::extra_of(&requests[1]);
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"STICKERSET_INVALID"}}"#,
        second.0
    ));
    assert!(h.driver.session.media.media_library.failed.contains(&2));
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

fn set_ids_asked(h: &Harness, set_id: i64) -> usize {
    h.sent("getStickerSet")
        .iter()
        .filter(|v| v["set_id"].as_str() == Some(set_id.to_string().as_str()))
        .count()
}

fn fail_set_one(h: &mut Harness) {
    h.driver.ensure_library_sets(&[1]).unwrap();
    let request = h.sent("getStickerSet").pop().unwrap();
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":420,"message":"FLOOD_WAIT_3"}}"#,
        Harness::extra_of(&request).0
    ));
    assert!(h.driver.session.media.media_library.failed.contains(&1));
    h.driver.ensure_library_sets(&[1]).unwrap();
}

/// A set whose load failed once (a FLOOD_WAIT while scrolling) is asked
/// for again when the panel opens next, instead of staying blank.
#[test]
fn a_failed_set_loads_again_when_the_panel_reopens() {
    let mut h = Harness::ready();
    fail_set_one(&mut h);
    assert_eq!(set_ids_asked(&h, 1), 1, "not while the panel stays open");
    h.driver.open_media_panel().unwrap();
    h.driver.ensure_library_sets(&[1]).unwrap();
    assert_eq!(set_ids_asked(&h, 1), 2);
}

/// Coming back online retries it too.
#[test]
fn a_failed_set_loads_again_after_a_reconnect() {
    let mut h = Harness::ready();
    fail_set_one(&mut h);
    h.ingest(r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateConnecting"}}"#);
    h.ingest(r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#);
    h.driver.ensure_library_sets(&[1]).unwrap();
    assert_eq!(set_ids_asked(&h, 1), 2);
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
    let options = h
        .driver
        .session
        .stickers
        .message_reaction_options
        .ready()
        .cloned()
        .unwrap();
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
    assert!(h.driver.session.stories.available_reactions.is_none());

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

/// TDLib downloads stickers, thumbnails and profile photos into the
/// database directory; the display sandbox must accept them there.
#[test]
fn media_roots_cover_tdlib_database_caches() {
    let h = Harness::ready();
    let roots = h.driver.tdlib_media_roots();
    let database = h.driver.paths.tdlib_database.clone();
    let stickers = database.join("stickers");
    std::fs::create_dir_all(&stickers).unwrap();
    let sticker = stickers.join("1.webp");
    std::fs::write(&sticker, b"RIFF").unwrap();
    let shown = crate::local_path::sandboxed_display_path(sticker.to_str().unwrap(), &roots);
    assert!(
        shown.is_some(),
        "sticker under the database dir is displayable"
    );
    // The database itself stays outside the sandbox.
    std::fs::create_dir_all(&database).unwrap();
    let db = database.join("db.sqlite");
    std::fs::write(&db, b"x").unwrap();
    assert!(crate::local_path::sandboxed_display_path(db.to_str().unwrap(), &roots).is_none());
    assert!(roots.contains(&h.driver.tdlib_files().to_path_buf()));
}

/// TDLib keeps story photos and clips in `<database>/stories`
/// (`FileType::VideoStory` is a secure-dir type). A downloaded story clip
/// must pass the display sandbox, or the viewer never opens its player.
#[test]
fn media_roots_cover_downloaded_stories() {
    let h = Harness::ready();
    let roots = h.driver.tdlib_media_roots();
    let stories = h.driver.paths.tdlib_database.join("stories");
    std::fs::create_dir_all(&stories).unwrap();
    let clip = stories.join("5150154155750328346.mp4");
    std::fs::write(&clip, b"\0\0\0\x18ftypmp42").unwrap();
    let shown = crate::local_path::sandboxed_display_path(clip.to_str().unwrap(), &roots);
    assert!(shown.is_some(), "story clip under the database dir plays");
}

/// TDLib announces `updateInstalledStickerSets` (custom emoji) while it
/// loads the list the panel asked for. Refreshing the catalog must not
/// leave the panel without its custom emoji packs.
#[test]
fn custom_emoji_packs_survive_the_installed_sets_update() {
    let mut h = Harness::ready();
    h.driver.open_media_panel().unwrap();
    h.ingest(r#"{"@type":"updateInstalledStickerSets","sticker_type":{"@type":"stickerTypeCustomEmoji"},"sticker_set_ids":["7"]}"#);
    let custom: Vec<Value> = h
        .sent("getInstalledStickerSets")
        .into_iter()
        .filter(|v| v["sticker_type"]["@type"] == "stickerTypeCustomEmoji")
        .collect();
    assert_eq!(custom.len(), 2, "the dropped request is sent again");
    let latest = Harness::extra_of(custom.last().unwrap());
    h.ingest(&format!(
        r#"{{"@type":"stickerSets","@extra":"{}","total_count":1,"sets":[{{"@type":"stickerSetInfo","id":"7","title":"Retro Font","name":"retro","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":false,"sticker_type":{{"@type":"stickerTypeCustomEmoji"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":60,"covers":[]}}]}}"#,
        latest.0
    ));
    let sets = &h.driver.session.stickers.emoji.installed_sets;
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].title, "Retro Font");
}

fn menu_harness() -> Harness {
    let mut h = Harness::ready();
    h.ingest(r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#);
    h.ingest(r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#);
    h
}

#[test]
fn message_reaction_options_wait_while_the_request_is_in_flight() {
    let mut h = menu_harness();
    h.driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap();
    let state = &h.driver.session.stickers.message_reaction_options;
    assert!(state.is_loading_for(ChatId(7), MessageId(50)));
    assert!(state.ready().is_none());
}

#[test]
fn failed_message_reaction_request_marks_the_options_unavailable() {
    let mut h = menu_harness();
    let extra = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"Message not found"}}"#,
        extra.0
    ));
    let state = &h.driver.session.stickers.message_reaction_options;
    assert!(state.is_unavailable_for(ChatId(7), MessageId(50)));
    assert!(!state.is_loading_for(ChatId(7), MessageId(50)));
}

#[test]
fn empty_message_reaction_answer_marks_the_options_unavailable() {
    let mut h = menu_harness();
    let extra = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    h.ingest(&format!(
        r#"{{"@type":"availableReactions","@extra":"{}","top_reactions":[],"recent_reactions":[],"popular_reactions":[],"allow_custom_emoji":false,"are_tags":false,"unavailability_reason":null}}"#,
        extra.0
    ));
    let state = &h.driver.session.stickers.message_reaction_options;
    assert!(state.is_unavailable_for(ChatId(7), MessageId(50)));
    assert!(state.ready().is_none());
}

#[test]
fn late_error_for_an_earlier_message_keeps_the_new_menu_loading() {
    let mut h = menu_harness();
    let first = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    h.driver
        .fetch_message_reactions(ChatId(7), MessageId(51))
        .unwrap();
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"Message not found"}}"#,
        first.0
    ));
    assert!(
        h.driver
            .session
            .stickers
            .message_reaction_options
            .is_loading_for(ChatId(7), MessageId(51))
    );
}

fn reactions_answer(extra: &str, top: &str) -> String {
    format!(
        r#"{{"@type":"availableReactions","@extra":"{extra}","top_reactions":[{top}],"recent_reactions":[],"popular_reactions":[],"allow_custom_emoji":false,"are_tags":false,"unavailability_reason":null}}"#
    )
}

const THUMB: &str = r#"{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"needs_premium":false}"#;
const STAR: &str =
    r#"{"@type":"availableReaction","type":{"@type":"reactionTypePaid"},"needs_premium":false}"#;

#[test]
fn late_answer_for_an_earlier_message_leaves_the_loading_menu_alone() {
    let mut h = menu_harness();
    let first = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    let second = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(51))
        .unwrap()
        .expect("request");
    h.ingest(&reactions_answer(
        &first.0.to_string(),
        &format!("{THUMB},{STAR}"),
    ));
    let state = &h.driver.session.stickers.message_reaction_options;
    assert!(state.is_loading_for(ChatId(7), MessageId(51)));
    assert!(!h.driver.session.payments.paid.offered_for(7, 50));
    h.ingest(&reactions_answer(&second.0.to_string(), THUMB));
    assert!(
        h.driver
            .session
            .stickers
            .message_reaction_options
            .ready_for(ChatId(7), MessageId(51))
            .is_some()
    );
}

#[test]
fn late_answers_after_b_answered_keep_bs_strip() {
    for late in [format!("{THUMB},{STAR}"), String::new()] {
        let mut h = menu_harness();
        let first = h
            .driver
            .fetch_message_reactions(ChatId(7), MessageId(50))
            .unwrap()
            .expect("request");
        let second = h
            .driver
            .fetch_message_reactions(ChatId(7), MessageId(51))
            .unwrap()
            .expect("request");
        h.ingest(&reactions_answer(&second.0.to_string(), THUMB));
        h.ingest(&reactions_answer(&first.0.to_string(), &late));
        let state = &h.driver.session.stickers.message_reaction_options;
        assert!(state.ready_for(ChatId(7), MessageId(51)).is_some());
        assert!(!state.is_unavailable_for(ChatId(7), MessageId(50)));
        assert!(!h.driver.session.payments.paid.offered_for(7, 50));
    }
}

#[test]
fn reopening_the_menu_after_an_error_asks_again() {
    let mut h = menu_harness();
    let extra = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"Timeout"}}"#,
        extra.0
    ));
    assert!(
        h.driver
            .session
            .stickers
            .message_reaction_options
            .is_unavailable_for(ChatId(7), MessageId(50))
    );
    let again = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap();
    assert!(again.is_some());
    assert!(
        h.driver
            .session
            .stickers
            .message_reaction_options
            .is_loading_for(ChatId(7), MessageId(50))
    );
}

#[test]
fn reopening_while_the_request_is_pending_sends_nothing() {
    let mut h = menu_harness();
    h.driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    let again = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap();
    assert!(again.is_none());
    assert_eq!(h.sent("getMessageAvailableReactions").len(), 1);
}

#[test]
fn reopening_after_a_lost_reply_asks_again() {
    let mut h = menu_harness();
    let lost = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    // The menu closed without a reset and the reply never came.
    h.driver.session.requests.take(lost);
    let again = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("a new request");
    assert_ne!(again, lost);
    assert_eq!(h.sent("getMessageAvailableReactions").len(), 2);
    h.ingest(&reactions_answer(&again.0.to_string(), THUMB));
    assert!(
        h.driver
            .session
            .stickers
            .message_reaction_options
            .ready_for(ChatId(7), MessageId(50))
            .is_some()
    );
}

#[test]
fn an_answer_offering_only_stars_still_shows_the_strip() {
    let mut h = menu_harness();
    let extra = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    h.ingest(&reactions_answer(&extra.0.to_string(), STAR));
    let state = &h.driver.session.stickers.message_reaction_options;
    assert!(state.ready_for(ChatId(7), MessageId(50)).is_some());
    assert!(h.driver.session.payments.paid.offered_for(7, 50));
}

#[test]
fn closing_the_menu_resets_the_options_and_ignores_the_late_answer() {
    let mut h = menu_harness();
    let extra = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap()
        .expect("request");
    h.driver.session.reset_message_reactions();
    h.ingest(&reactions_answer(&extra.0.to_string(), THUMB));
    assert_eq!(
        h.driver.session.stickers.message_reaction_options,
        crate::state::MessageReactionsState::Idle
    );
}

fn chat_with_message(chat_type: &str) -> Harness {
    let mut h = Harness::ready();
    h.ingest(&format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":7,"title":"Alice","type":{chat_type},"unread_count":0}}}}"#
    ));
    h.ingest(r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#);
    h.ingest(r#"{"@type":"updateNewMessage","message":{"@type":"message","id":50,"chat_id":7,"is_outgoing":false,"date":1700000000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#);
    h
}

const SECRET_CHAT: &str = r#"{"@type":"chatTypeSecret","secret_chat_id":9,"user_id":7}"#;
const PRIVATE_CHAT: &str = r#"{"@type":"chatTypePrivate","user_id":7}"#;

fn shows_reaction(h: &Harness, emoji: &str) -> bool {
    let message = &h.driver.session.histories[&7].messages[&50];
    message.chosen_reaction(&ReactionChoice::Emoji(emoji.into()))
}

#[test]
fn a_secret_chat_never_waits_for_reactions_it_cannot_have() {
    let mut h = chat_with_message(SECRET_CHAT);
    let asked = h
        .driver
        .fetch_message_reactions(ChatId(7), MessageId(50))
        .unwrap();
    assert!(asked.is_none());
    assert!(h.sent("getMessageAvailableReactions").is_empty());
    let state = &h.driver.session.stickers.message_reaction_options;
    assert!(!state.is_loading_for(ChatId(7), MessageId(50)));
}

#[test]
fn a_secret_chat_refuses_a_reaction_before_it_shows_on_the_bubble() {
    let mut h = chat_with_message(SECRET_CHAT);
    let choice = ReactionChoice::Emoji("👍".into());
    assert!(
        h.driver
            .toggle_reaction_choice(ChatId(7), MessageId(50), &choice)
            .is_err()
    );
    assert!(
        h.driver
            .toggle_message_reaction(ChatId(7), MessageId(50), "👍")
            .is_err()
    );
    assert!(h.sent("addMessageReaction").is_empty());
    assert!(!shows_reaction(&h, "👍"));
}

#[test]
fn a_reaction_telegram_refuses_leaves_the_bubble() {
    let mut h = chat_with_message(PRIVATE_CHAT);
    let choice = ReactionChoice::Emoji("👍".into());
    let extra = h
        .driver
        .toggle_reaction_choice(ChatId(7), MessageId(50), &choice)
        .unwrap();
    assert!(shows_reaction(&h, "👍"));
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"The reaction isn't available for the message"}}"#,
        extra.0
    ));
    assert!(!shows_reaction(&h, "👍"));
}

#[test]
fn an_accepted_reaction_stays_on_the_bubble() {
    let mut h = chat_with_message(PRIVATE_CHAT);
    let choice = ReactionChoice::Emoji("👍".into());
    let extra = h
        .driver
        .toggle_reaction_choice(ChatId(7), MessageId(50), &choice)
        .unwrap();
    h.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0));
    assert!(shows_reaction(&h, "👍"));
}
