use super::super::{ConnectDriver, RecordingSender};
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::emoji::EmojiSetTab;
use crate::ids::{FileId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::RequestPurpose;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::AuthorizationState;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn custom_emoji_pack_search_paging_previews_and_mutation_races() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: serde_json::Value| {
        driver
            .ingest(copy_and_parse(&payload.to_string(), &seq, &sink).unwrap())
            .unwrap();
    };
    let set = |id: i64, installed: bool| json!({"@type":"stickerSetInfo","id":id.to_string(),"title":format!("Pack {id}"),"name":format!("Pack{id}"),"size":1,"is_installed":installed});
    let installed = driver.open_emoji_sets().unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "getInstalledStickerSets")["sticker_type"]["@type"],
        "stickerTypeCustomEmoji"
    );
    ingest(
        &mut driver,
        json!({"@type":"stickerSets","@extra":installed.as_extra(),"sets":[set(1,true)]}),
    );
    assert_eq!(driver.session.emoji.installed_sets.len(), 1);
    assert!(driver.session.stickers.sets.is_empty());
    let old = driver.search_emoji_packs("old").unwrap().unwrap();
    let current = driver.search_emoji_packs("new").unwrap().unwrap();
    ingest(
        &mut driver,
        json!({"@type":"stickerSets","@extra":old.as_extra(),"sets":[set(9,false)]}),
    );
    assert!(driver.session.emoji.found_sets.is_empty());
    ingest(
        &mut driver,
        json!({"@type":"stickerSets","@extra":current.as_extra(),"sets":[set(2,false)]}),
    );
    assert_eq!(driver.session.emoji.found_sets[0].id, 2);
    let first = driver
        .select_emoji_set_tab(EmojiSetTab::Trending)
        .unwrap()
        .unwrap();
    ingest(
        &mut driver,
        json!({"@type":"trendingStickerSets","@extra":first.as_extra(),"total_count":3,"sets":[set(1,true),set(2,false)],"is_premium":false}),
    );
    assert!(driver.session.emoji.trending_has_more);
    assert_eq!(
        sent_request(&recorder, "viewTrendingStickerSets")["sticker_set_ids"],
        json!([1, 2])
    );
    let more = driver.more_trending_emoji_packs().unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "getTrendingStickerSets")["offset"],
        2
    );
    ingest(
        &mut driver,
        json!({"@type":"trendingStickerSets","@extra":more.as_extra(),"total_count":3,"sets":[set(2,false)]}),
    );
    assert_eq!(driver.session.emoji.trending_sets.len(), 2);
    assert!(!driver.session.emoji.trending_has_more); // Raw page length, not deduped count.
    let old = driver.preview_emoji_pack(1).unwrap().unwrap();
    let preview = driver.preview_emoji_pack(2).unwrap().unwrap();
    ingest(
        &mut driver,
        json!({"@type":"stickerSet","@extra":old.as_extra(),"id":"1","title":"Old","stickers":[]}),
    );
    assert!(driver.session.emoji.preview_title.is_empty());
    ingest(
        &mut driver,
        json!({"@type":"stickerSet","@extra":preview.as_extra(),"id":"2","title":"New","stickers":[
        {"@type":"sticker","set_id":"2","width":64,"height":64,"emoji":"😀","format":{"@type":"stickerFormatWebp"},"sticker":{
            "@type":"file","id":109,"size":5,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false}}}]}),
    );
    assert_eq!(driver.session.emoji.preview_title, "New");
    assert_eq!(driver.session.emoji.preview[0].file_id, FileId(109));
    assert_eq!(sent_request(&recorder, "downloadFile")["file_id"], 109); // Preview works without a chat.
    assert!(driver.session.stickers.stickers.is_empty());
    let remove = driver.set_emoji_pack_installed(1, false).unwrap().unwrap();
    assert!(driver.set_emoji_pack_installed(2, true).unwrap().is_none());
    ingest(
        &mut driver,
        json!({"@type":"error","@extra":remove.as_extra(),"code":500,"message":"test"}),
    );
    assert_eq!(driver.session.emoji.installed_sets.len(), 1);
    assert!(driver.session.emoji.mutation_failed);
    let remove = driver.set_emoji_pack_installed(1, false).unwrap().unwrap();
    // An installed list issued before mutation success must not restore removed entries later.
    let stale = driver
        .select_emoji_set_tab(EmojiSetTab::Installed)
        .unwrap()
        .unwrap();
    ingest(
        &mut driver,
        json!({"@type":"ok","@extra":remove.as_extra()}),
    );
    assert!(!driver.session.emoji.mutation_failed);
    assert!(driver.session.emoji.installed_sets.is_empty());
    ingest(
        &mut driver,
        json!({"@type":"stickerSets","@extra":stale.as_extra(),"sets":[set(1,true)]}),
    );
    assert!(driver.session.emoji.installed_sets.is_empty());
    assert!(
        driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetInstalledEmojiSets)
    );
    let fresh = RequestId(
        sent_request(&recorder, "getInstalledStickerSets")["@extra"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    );
    ingest(
        &mut driver,
        json!({"@type":"stickerSets","@extra":fresh.as_extra(),"sets":[]}),
    );
    assert!(
        driver
            .session
            .emoji
            .trending_sets
            .iter()
            .all(|set| !set.is_installed)
    );
    assert!(driver.preview_emoji_pack(-1).is_err());
    assert!(driver.set_emoji_pack_installed(0, true).is_err());
    driver.session.auth = AuthorizationState::WaitPhoneNumber;
    assert!(driver.open_emoji_sets().is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn emoji_status_choices_resolution_timing_and_confirmed_clear() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: serde_json::Value| {
        driver
            .ingest(copy_and_parse(&payload.to_string(), &seq, &sink).unwrap())
            .unwrap();
    };
    let request_id = |kind| {
        RequestId(
            sent_request(&recorder, kind)["@extra"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        )
    };
    assert!(driver.change_emoji_status(Some(91), 3600).is_err());
    driver.session.emoji.open = true;
    driver.load_emoji_status_choices().unwrap();
    let recent = request_id("getRecentEmojiStatuses");
    let themed = request_id("getThemedEmojiStatuses");
    let default = request_id("getDefaultEmojiStatuses");
    ingest(
        &mut driver,
        json!({"@type":"emojiStatuses","@extra":recent.as_extra(),"emoji_statuses":[{"@type":"emojiStatus","type":{"@type":"emojiStatusTypeCustomEmoji","custom_emoji_id":"91"},"expiration_date":0}]}),
    );
    let resolve = request_id("getCustomEmojiStickers");
    assert_eq!(
        sent_request(&recorder, "getCustomEmojiStickers")["custom_emoji_ids"],
        json!(["91"])
    );
    ingest(
        &mut driver,
        json!({"@type":"emojiStatusCustomEmojis","@extra":themed.as_extra(),"custom_emoji_ids":["92","92","0"]}),
    );
    ingest(
        &mut driver,
        json!({"@type":"emojiStatusCustomEmojis","@extra":default.as_extra(),"custom_emoji_ids":["93"]}),
    );
    let sticker = |id: i64, file| json!({"@type":"sticker","full_type":{"@type":"stickerFullTypeCustomEmoji","custom_emoji_id":id.to_string()},"set_id":"1","width":64,"height":64,"emoji":"😀","format":{"@type":"stickerFormatWebp"},"sticker":{"@type":"file","id":file,"size":5,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false}}});
    ingest(
        &mut driver,
        json!({"@type":"stickers","@extra":resolve.as_extra(),"stickers":[sticker(91,109)]}),
    );
    assert_eq!(sent_request(&recorder, "downloadFile")["file_id"], 109);
    assert_eq!(
        sent_request(&recorder, "getCustomEmojiStickers")["custom_emoji_ids"],
        json!(["92", "93"])
    );
    let resolve = request_id("getCustomEmojiStickers");
    ingest(
        &mut driver,
        json!({"@type":"stickers","@extra":resolve.as_extra(),"stickers":[sticker(92,110),sticker(93,111)]}),
    );
    assert_eq!(driver.session.emoji.custom_emoji_stickers.len(), 3); // Sticker.id is absent; distinct custom IDs survive batching.
    driver.session.my_user_id = Some(7);
    ingest(
        &mut driver,
        json!({"@type":"updateUser","user":{"@type":"user","id":7,"first_name":"Test","last_name":"","phone_number":"","is_premium":true,"type":{"@type":"userTypeRegular"}}}),
    );
    assert!(driver.change_emoji_status(Some(0), 0).is_err());
    assert!(driver.change_emoji_status(Some(91), -1).is_err());
    assert!(driver.change_emoji_status(Some(91), i32::MAX).is_err());
    let before = crate::state::unix_ms_now() / 1000;
    let set = driver.change_emoji_status(Some(91), 3600).unwrap().unwrap();
    let sent = sent_request(&recorder, "setEmojiStatus");
    assert_eq!(sent["emoji_status"]["type"]["custom_emoji_id"], "91");
    let expiry = sent["emoji_status"]["expiration_date"].as_u64().unwrap();
    assert!(expiry >= before + 3600 && expiry <= crate::state::unix_ms_now() / 1000 + 3600);
    assert!(driver.clear_recent_emoji_statuses().unwrap().is_none());
    assert_eq!(driver.session.emoji.recent_statuses.len(), 1);
    ingest(
        &mut driver,
        json!({"@type":"error","@extra":set.as_extra(),"code":400,"message":"PRIVATE_DETAIL"}),
    );
    assert_eq!(driver.session.emoji.recent_statuses.len(), 1);
    assert!(
        !driver
            .session
            .emoji
            .status_note
            .as_ref()
            .unwrap()
            .contains("PRIVATE_DETAIL")
    );
    let set = driver.change_emoji_status(Some(92), 0).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "setEmojiStatus")["emoji_status"]["expiration_date"],
        0
    );
    ingest(&mut driver, json!({"@type":"ok","@extra":set.as_extra()}));
    assert_eq!(
        driver.session.emoji.status_note.as_deref(),
        Some("Emoji status updated.")
    );
    let remove = driver.change_emoji_status(None, 3600).unwrap().unwrap();
    assert!(sent_request(&recorder, "setEmojiStatus")["emoji_status"].is_null());
    ingest(
        &mut driver,
        json!({"@type":"ok","@extra":remove.as_extra()}),
    );
    driver.load_emoji_status_choices().unwrap();
    let stale = request_id("getRecentEmojiStatuses");
    let clear = driver.clear_recent_emoji_statuses().unwrap().unwrap();
    ingest(&mut driver, json!({"@type":"ok","@extra":clear.as_extra()}));
    ingest(
        &mut driver,
        json!({"@type":"emojiStatuses","@extra":stale.as_extra(),"emoji_statuses":[{"@type":"emojiStatus","type":{"@type":"emojiStatusTypeCustomEmoji","custom_emoji_id":"91"},"expiration_date":0}]}),
    );
    assert!(driver.session.emoji.recent_statuses.is_empty());
    assert_eq!(
        driver.session.emoji.status_note.as_deref(),
        Some("Recent emoji statuses cleared.")
    );
    driver.session.auth = AuthorizationState::WaitPhoneNumber;
    assert!(driver.load_emoji_status_choices().is_err());
    assert!(driver.clear_recent_emoji_statuses().is_err());
    let _ = std::fs::remove_dir_all(dir);
}
