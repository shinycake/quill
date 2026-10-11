use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::FileId;
use crate::platform::MemorySecretStore;
use crate::state::StickersPurpose;
use crate::state::{RequestPurpose, StickerTab};
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn sticker_tabs_requests_failures_and_confirmed_mutations() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.stickers.open = true;
    let recent = driver
        .select_sticker_tab(StickerTab::Recent)
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "getRecentStickers")["is_attached"],
        false
    );
    assert_eq!(driver.select_sticker_tab(StickerTab::Recent).unwrap(), None);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"error", "@extra":recent.as_extra(), "code":500,"message":"test"})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.stickers.failed);
    assert!(
        driver
            .select_sticker_tab(StickerTab::Recent)
            .unwrap()
            .is_some()
    );
    assert!(!driver.session.stickers.failed);

    driver.select_sticker_tab(StickerTab::Favorites).unwrap();
    let add = driver
        .set_favorite_sticker(FileId(41), true)
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "addFavoriteSticker")["sticker"]["id"],
        41
    );
    assert_eq!(driver.set_favorite_sticker(FileId(42), true).unwrap(), None);
    // A successful mutation replaces an older pending fetch; its late answer is ignored.
    let fetch = sent_request(&recorder, "getFavoriteStickers");
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":add.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_ne!(
        sent_request(&recorder, "getFavoriteStickers")["@extra"],
        fetch["@extra"]
    );
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"stickers","@extra":fetch["@extra"],"stickers":[]}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetFavoriteStickers)
    );
    assert_eq!(driver.session.stickers.tab, StickerTab::Favorites);
    assert!(driver.set_favorite_sticker(FileId(0), true).is_err());
    let clear = driver.clear_recent_stickers().unwrap().unwrap();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":clear.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetRecentStickers)
    );
    assert!(driver.session.stickers.recent.is_empty());
    assert_eq!(
        sent_request(&recorder, "clearRecentStickers")["is_attached"],
        false
    );

    let trending = driver
        .select_sticker_tab(StickerTab::Trending)
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "getTrendingStickerSets")["offset"],
        0
    );
    let sets = json!({"@type":"trendingStickerSets","@extra":trending.as_extra(),"total_count":1,"is_premium":false,"sets":[{"@type":"stickerSetInfo","id":"77","title":"Discover","name":"Discover","size":2}]});
    driver
        .ingest(copy_and_parse(&sets.to_string(), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.stickers.trending.len(), 1);
    assert_eq!(
        sent_request(&recorder, "viewTrendingStickerSets")["sticker_set_ids"],
        json!([77])
    );
    let page = driver.fetch_trending_stickers(true).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "getTrendingStickerSets")["offset"],
        1
    );
    let sets = json!({"@type":"trendingStickerSets","@extra":page.as_extra(),"total_count":3,"is_premium":false,"sets":[{"@type":"stickerSetInfo","id":"77","title":"Discover","name":"Discover","size":2},{"@type":"stickerSetInfo","id":"88","title":"More","name":"More","size":2}]});
    driver
        .ingest(copy_and_parse(&sets.to_string(), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.stickers.trending.len(), 2);
    assert_eq!(driver.session.stickers.trending_next_offset, 3);
    assert_eq!(
        sent_request(&recorder, "viewTrendingStickerSets")["sticker_set_ids"],
        json!([77, 88])
    );
    driver.select_sticker_set(77).unwrap();
    assert_eq!(sent_request(&recorder, "getStickerSet")["set_id"], "77");
    driver.session.stickers.close();
    driver.session.auth = crate::telegram::envelope::AuthorizationState::WaitPhoneNumber;
    assert!(driver.select_sticker_tab(StickerTab::Recent).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sticker_search_and_set_management_use_latest_confirmed_state() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.stickers.open = true;
    driver.select_sticker_tab(StickerTab::Search).unwrap();
    driver.search_sticker_picker(" old ").unwrap();
    let old = sent_request(&recorder, "searchStickerSets")["@extra"].clone();
    driver.search_sticker_picker("new").unwrap();
    let latest = sent_request(&recorder, "searchStickerSets")["@extra"].clone();
    assert_ne!(latest, old);
    assert_eq!(sent_request(&recorder, "searchStickers")["query"], "new");
    let result = |extra, id| {
        json!({"@type":"stickerSets","@extra":extra,"sets":[{"@type":"stickerSetInfo","id":id,"title":"Found","name":"Found","size":2,"is_installed":false}]}).to_string()
    };
    driver
        .ingest(copy_and_parse(&result(old, "66"), &seq, &sink).unwrap())
        .unwrap();
    assert!(driver.session.stickers.found_sets.is_empty());
    driver
        .ingest(copy_and_parse(&result(latest, "77"), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.stickers.found_sets[0].id, 77);
    let first = sent_request(&recorder, "searchStickers")["@extra"].clone();
    let sticker = |id| json!({"@type":"sticker","id":id,"set_id":"77","emoji":"😀","format":{"@type":"stickerFormatWebp"},"sticker":{"@type":"file","id":id,"local":{"@type":"localFile","can_be_downloaded":false},"remote":{"@type":"remoteFile"}}});
    let stickers: Vec<_> = (1..=100).map(sticker).collect();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"stickers","@extra":first,"stickers":stickers}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.stickers.found_stickers.len(), 100);
    assert!(driver.session.stickers.search_has_more);
    let more = driver.more_sticker_search_results().unwrap().unwrap();
    assert_eq!(sent_request(&recorder, "searchStickers")["offset"], 100);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"stickers","@extra":more.as_extra(),"stickers":[sticker(100)]})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.stickers.found_stickers.len(), 100);
    assert_eq!(driver.session.stickers.search_offset, 101);
    assert!(!driver.session.stickers.search_has_more);
    driver.select_sticker_set(77).unwrap();
    assert_eq!(sent_request(&recorder, "getStickerSet")["set_id"], "77");
    driver.select_sticker_tab(StickerTab::Search).unwrap();
    assert_eq!(driver.session.stickers.selected_set_id, None);
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetStickerSet)
    );
    assert!(driver.manage_sticker_set(77, true, true).is_err());
    let install = driver.manage_sticker_set(77, true, false).unwrap().unwrap();
    assert!(!driver.session.stickers.found_sets[0].is_installed);
    assert_eq!(driver.manage_sticker_set(77, false, false).unwrap(), None);
    assert_eq!(sent_request(&recorder, "changeStickerSet")["set_id"], "77");
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":install.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.stickers.found_sets[0].is_installed);
    assert!(
        driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetInstalledStickerSets)
    );
    let remove = driver
        .manage_sticker_set(77, false, false)
        .unwrap()
        .unwrap();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"error","@extra":remove.as_extra(),"code":500,"message":"test"})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.stickers.failed);
    assert!(driver.session.stickers.found_sets[0].is_installed);
    driver.close_sticker_panel();
    let remove = driver
        .manage_sticker_set(77, false, false)
        .unwrap()
        .unwrap();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":remove.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetInstalledStickerSets)
    );
    assert!(!driver.session.stickers.found_sets[0].is_installed);
    driver.search_sticker_picker("").unwrap();
    assert!(!driver.session.stickers.failed);
    assert!(driver.session.stickers.found_sets.is_empty());
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::SearchStickers)
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sticker_reordering_preserves_order_on_failure_and_refetches_on_success() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.stickers.open = true;
    let fetch = driver
        .session
        .request(RequestPurpose::GetInstalledStickerSets, None);
    let sets = |extra, ids: &[i64]| {
        json!({"@type":"stickerSets","@extra":extra,"sets":ids.iter().map(|id| json!({"@type":"stickerSetInfo","id":id.to_string(),"title":"Test","name":"Test","size":0,"is_installed":true})).collect::<Vec<_>>()}).to_string()
    };
    driver
        .ingest(copy_and_parse(&sets(json!(fetch.as_extra()), &[11, 22, 33]), &seq, &sink).unwrap())
        .unwrap();
    let current = driver.session.stickers.sets.clone();
    assert!(driver.reorder_sticker_set(99, 11).is_err());
    assert_eq!(driver.reorder_sticker_set(11, 11).unwrap(), None);
    let move_last = driver.reorder_sticker_set(11, 33).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "reorderInstalledStickerSets")["sticker_set_ids"],
        json!([22, 33, 11])
    );
    assert_eq!(driver.session.stickers.sets, current);
    assert_eq!(driver.reorder_sticker_set(33, 11).unwrap(), None);
    assert_eq!(driver.manage_sticker_set(11, false, true).unwrap(), None);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"error","@extra":move_last.as_extra(),"code":500,"message":"test"})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.stickers.sets, current);
    assert!(driver.session.stickers.failed);
    let move_first = driver.reorder_sticker_set(33, 11).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "reorderInstalledStickerSets")["sticker_set_ids"],
        json!([33, 11, 22])
    );
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":move_first.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    let refreshed = sent_request(&recorder, "getInstalledStickerSets")["@extra"].clone();
    driver
        .ingest(copy_and_parse(&sets(refreshed, &[33, 11, 22]), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(
        driver
            .session
            .stickers
            .sets
            .iter()
            .map(|set| set.id)
            .collect::<Vec<_>>(),
        vec![33, 11, 22]
    );
    driver
        .session
        .stickers
        .sets
        .push(driver.session.stickers.sets[0].clone());
    assert!(driver.reorder_sticker_set(11, 33).is_err());
    driver.session.auth = crate::telegram::envelope::AuthorizationState::WaitPhoneNumber;
    assert!(driver.reorder_sticker_set(11, 33).is_err());
    drop(driver);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn archived_sticker_paging_and_restore_ignore_pre_mutation_fetches() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.stickers.open = true;
    let first = driver
        .select_sticker_tab(StickerTab::Archived)
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "getArchivedStickerSets")["sticker_type"]["@type"],
        "stickerTypeRegular"
    );
    assert_eq!(driver.fetch_archived_stickers(true).unwrap(), None);
    let sets = |extra, ids: Vec<i64>| {
        json!({"@type":"stickerSets","@extra":extra,"sets":ids.iter().map(|id| json!({"@type":"stickerSetInfo","id":id.to_string(),"title":"Archived","name":"Archived","size":0,"is_installed":false,"is_archived":true})).collect::<Vec<_>>()}).to_string()
    };
    driver
        .ingest(
            copy_and_parse(
                &sets(json!(first.as_extra()), (1..=100).collect()),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.stickers.archived_has_more);
    assert!(driver.session.open_chat.is_none());
    let preview = driver.select_sticker_set(1).unwrap().unwrap();
    driver.ingest(copy_and_parse(&json!({"@type":"stickerSet","@extra":preview.as_extra(),"id":"1","stickers":[{"@type":"sticker","id":"900","set_id":"1","emoji":"😀","format":{"@type":"stickerFormatWebp"},"sticker":{"@type":"file","id":901,"local":{"@type":"localFile","can_be_downloaded":true},"remote":{"@type":"remoteFile"}}}]}).to_string(),&seq,&sink).unwrap()).unwrap();
    assert_eq!(sent_request(&recorder, "downloadFile")["file_id"], 901);

    let more = driver.fetch_archived_stickers(true).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "getArchivedStickerSets")["offset_sticker_set_id"],
        "100"
    );
    driver
        .ingest(copy_and_parse(&sets(json!(more.as_extra()), vec![100, 101]), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.stickers.archived.len(), 101);
    assert!(!driver.session.stickers.archived_has_more);
    assert_eq!(driver.fetch_archived_stickers(true).unwrap(), None);
    let before = driver.session.stickers.archived.clone();
    let restore = driver.manage_sticker_set(1, true, false).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "changeStickerSet")["is_archived"],
        false
    );
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"error","@extra":restore.as_extra(),"code":500,"message":"test"})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.stickers.archived, before);
    let old = driver.fetch_archived_stickers(false).unwrap().unwrap();
    driver.select_sticker_set(2).unwrap();
    assert!(
        driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetStickerSet)
    );
    let restore = driver.manage_sticker_set(1, true, false).unwrap().unwrap();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":restore.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.stickers.archived.is_empty());
    assert_eq!(driver.session.stickers.selected_set_id, None);
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetStickerSet)
    );
    let fresh = sent_request(&recorder, "getArchivedStickerSets")["@extra"].clone();
    assert_ne!(fresh, json!(old.as_extra()));
    driver
        .ingest(copy_and_parse(&sets(json!(old.as_extra()), vec![1]), &seq, &sink).unwrap())
        .unwrap();
    assert!(driver.session.stickers.archived.is_empty());
    driver
        .ingest(copy_and_parse(&sets(fresh, vec![2]), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.stickers.archived[0].id, 2);
    let archive = driver.manage_sticker_set(2, false, true).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "changeStickerSet")["is_installed"],
        false
    );
    assert_eq!(
        sent_request(&recorder, "changeStickerSet")["is_archived"],
        true
    );
    driver.session.stickers.close();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":archive.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.stickers.archived.is_empty());
    drop(driver);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn composer_suggestions_wait_for_installed_sets_and_keep_the_latest_emoji() {
    use crate::sticker_suggest::StickerSuggestMode;
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.settings.media_prefs.sticker_suggest_mode = StickerSuggestMode::InstalledOnly;
    let fetch = driver.update_sticker_suggestions("😀").unwrap().unwrap();
    assert!(driver.session.stickers.suggest_waiting_for_sets);
    assert_eq!(driver.update_sticker_suggestions("🔥").unwrap(), None);
    driver.ingest(copy_and_parse(&json!({"@type":"stickerSets","@extra":fetch.as_extra(),"sets":[{"@type":"stickerSetInfo","id":"77","size":1,"is_installed":true}]}).to_string(),&seq,&sink).unwrap()).unwrap();
    assert!(driver.session.stickers.installed_loaded);
    assert_eq!(sent_request(&recorder, "searchStickers")["emojis"], "🔥");
    assert_eq!(driver.update_sticker_suggestions("🔥").unwrap(), None);
    let old = sent_request(&recorder, "searchStickers")["@extra"].clone();
    let fresh = driver.update_sticker_suggestions("😀").unwrap().unwrap();
    let sticker = |id, set_id| json!({"@type":"sticker","id":id,"set_id":set_id,"emoji":"😀","format":{"@type":"stickerFormatWebp"},"sticker":{"@type":"file","id":id,"local":{"@type":"localFile","can_be_downloaded":true},"remote":{"@type":"remoteFile"}}});
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"stickers","@extra":old,"stickers":[sticker(8,"77")]}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.stickers.suggestions.is_empty());
    driver.ingest(copy_and_parse(&json!({"@type":"stickers","@extra":fresh.as_extra(),"stickers":[sticker(9,"77"),sticker(10,"88")]}).to_string(),&seq,&sink).unwrap()).unwrap();
    assert_eq!(driver.session.stickers.suggestions.len(), 1);
    assert_eq!(driver.session.stickers.suggestions[0].file_id, FileId(9));
    assert!(driver.session.stickers.found_stickers.is_empty());
    assert!(!driver.session.stickers.open);
    assert_eq!(sent_request(&recorder, "downloadFile")["file_id"], 9);
    driver.update_sticker_suggestions("plain text").unwrap();
    assert!(driver.session.stickers.suggestions.is_empty());
    let pending = driver.update_sticker_suggestions("😀").unwrap().unwrap();
    driver.session.settings.media_prefs.sticker_suggest_mode = StickerSuggestMode::None;
    driver.update_sticker_suggestions("😀").unwrap();
    driver.ingest(copy_and_parse(&json!({"@type":"stickers","@extra":pending.as_extra(),"stickers":[sticker(9,"77")]}).to_string(),&seq,&sink).unwrap()).unwrap();
    assert!(driver.session.stickers.suggestions.is_empty());
    assert!(!driver.session.stickers.suggest_waiting_for_sets);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sticker_batches_report_confirmed_partial_results_and_reject_overlapping_work() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    assert!(driver.manage_sticker_sets(&[77, -1], true).is_err());
    assert_eq!(driver.session.stickers.batch_total, 0);
    assert_eq!(driver.manage_sticker_sets(&[77, 88, 77], true).unwrap(), 2);
    assert_eq!(driver.session.stickers.batch_total, 2);
    assert_eq!(driver.session.stickers.batch_completed, 0);
    assert_eq!(driver.session.stickers.batch_pending, vec![77, 88]);
    assert!(driver.manage_sticker_sets(&[99], false).is_err());
    let ok = driver
        .session
        .requests
        .pending
        .values()
        .find(|pending| {
            pending.purpose
                == RequestPurpose::Stickers(StickersPurpose::ManageStickerSet {
                    set_id: 77,
                    installed: true,
                    archived: false,
                })
        })
        .unwrap()
        .id;
    let err = sent_request(&recorder, "changeStickerSet")["@extra"].clone();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":ok.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.stickers.batch_completed, 1);
    assert_eq!(driver.session.stickers.batch_pending, vec![88]);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"error","@extra":err,"code":500,"message":"test"}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.stickers.batch_failed, 1);
    assert!(driver.session.stickers.batch_pending.is_empty());
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":ok.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.stickers.batch_completed, 1);
    assert_eq!(driver.manage_sticker_sets(&[88], false).unwrap(), 1);
    assert_eq!(driver.session.stickers.batch_failed, 0);
    assert_eq!(driver.session.stickers.batch_completed, 0);
    assert_eq!(driver.session.stickers.batch_total, 1);
    assert_eq!(
        sent_request(&recorder, "changeStickerSet")["is_installed"],
        false
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn premium_sticker_send_uses_full_type_and_current_account_entitlement() {
    use crate::ids::ChatId;
    use crate::telegram::requests::StickerSend;
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.ingest(copy_and_parse(&json!({"@type":"updateNewChat","chat":{"id":7,"title":"Test","type":{"@type":"chatTypePrivate","user_id":7}}}).to_string(),&seq,&sink).unwrap()).unwrap();
    let fetch = driver.session.request(RequestPurpose::GetStickerSet, None);
    let sticker = |id, premium| json!({"@type":"sticker","id":id,"set_id":"77","emoji":"😀","format":{"@type":"stickerFormatWebp"},"full_type":{"@type":"stickerFullTypeRegular","premium_animation":premium},"sticker":{"@type":"file","id":id,"local":{"@type":"localFile","can_be_downloaded":false},"remote":{"@type":"remoteFile"}}});
    driver.ingest(copy_and_parse(&json!({"@type":"stickerSet","@extra":fetch.as_extra(),"id":"77","stickers":[sticker(9,json!({"@type":"file","id":90})),sticker(10,json!(null))]}).to_string(),&seq,&sink).unwrap()).unwrap();
    assert!(driver.session.sticker_requires_premium(FileId(9)));
    assert!(!driver.session.sticker_requires_premium(FileId(10)));
    let send = |id| StickerSend {
        file_id: FileId(id),
        emoji: "😀",
        width: 100,
        height: 100,
        thumb: None,
        reply_to: None,
        topic_id: None,
    };
    assert!(driver.send_sticker(ChatId(7), send(9)).is_err());
    assert!(driver.send_sticker(ChatId(7), send(10)).is_ok());
    driver.session.my_user_id = Some(7);
    driver.ingest(copy_and_parse(&json!({"@type":"updateUser","user":{"@type":"user","id":7,"first_name":"Test","is_premium":true,"type":{"@type":"userTypeRegular"}}}).to_string(),&seq,&sink).unwrap()).unwrap();
    assert!(driver.send_sticker(ChatId(7), send(9)).is_ok());
    // History metadata remains sufficient after picker/search caches have changed.
    driver.ingest(copy_and_parse(&json!({"@type":"updateNewMessage","message":{"id":1,"chat_id":7,"content":{"@type":"messageSticker","is_premium":false,"sticker":sticker(9,json!({"@type":"file","id":90}))}}}).to_string(),&seq,&sink).unwrap()).unwrap();
    driver.session.stickers.stickers.clear();
    driver.session.users.get_mut(&7).unwrap().is_premium = false;
    assert!(driver.session.sticker_requires_premium(FileId(9)));
    assert!(driver.send_sticker(ChatId(7), send(9)).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn gif_search_pages_and_saved_mutations_use_current_confirmed_state() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let saved = driver.open_gif_panel().unwrap().unwrap();
    driver.search_gifs("before bot option").unwrap();
    assert!(driver.session.gifs.search_failed);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"animations","@extra":saved.as_extra(),"animations":[]})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.gifs.loaded);
    assert!(driver.session.gifs.search_failed); // Saved loading must not hide a search failure.
    driver.ingest(copy_and_parse(&json!({"@type":"updateOption","name":"animation_search_bot_username","value":{"@type":"optionValueString","value":"gif"}}).to_string(),&seq,&sink).unwrap()).unwrap();
    let count_saved = || {
        recorder
            .snapshot()
            .iter()
            .filter(|request| {
                serde_json::from_str::<serde_json::Value>(request).unwrap()["@type"]
                    == "getSavedAnimations"
            })
            .count()
    };
    assert_eq!(count_saved(), 1); // A confirmed empty collection must not fetch on every update.
    driver.search_gifs("old").unwrap();
    let old_bot = sent_request(&recorder, "searchPublicChat")["@extra"].clone();
    driver.search_gifs(" cats ").unwrap();
    let bot = sent_request(&recorder, "searchPublicChat")["@extra"].clone();
    assert_ne!(old_bot, bot);
    let resolved = |extra| {
        json!({"@type":"chat","@extra":extra,"id":7,"title":"GIF bot","type":{"@type":"chatTypePrivate","user_id":7}}).to_string()
    };
    driver
        .ingest(copy_and_parse(&resolved(old_bot), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.gifs.search_bot_user_id, None);
    driver
        .ingest(copy_and_parse(&resolved(bot), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "getInlineQueryResults")["query"],
        "cats"
    );
    let old = sent_request(&recorder, "getInlineQueryResults")["@extra"].clone();
    let fresh = driver.search_gifs("dogs").unwrap().unwrap();
    let animation = |id| json!({"@type":"animation","duration":1,"width":100,"height":100,"animation":{"@type":"file","id":id,"local":{"@type":"localFile","can_be_downloaded":false},"remote":{"@type":"remoteFile"}},"thumbnail":{"@type":"thumbnail","width":100,"height":100,"file":{"@type":"file","id":id+100,"local":{"@type":"localFile","can_be_downloaded":true},"remote":{"@type":"remoteFile"}}}});
    let page = |extra, ids: Vec<i32>, next: &str| {
        json!({"@type":"inlineQueryResults","@extra":extra,"inline_query_id":"1","results":ids.iter().map(|id|json!({"@type":"inlineQueryResultAnimation","id":id.to_string(),"animation":animation(*id)})).collect::<Vec<_>>(),"next_offset":next}).to_string()
    };
    driver
        .ingest(copy_and_parse(&page(old, vec![8], ""), &seq, &sink).unwrap())
        .unwrap();
    assert!(driver.session.gifs.search_results.is_empty());
    driver
        .ingest(copy_and_parse(&page(json!(fresh.as_extra()), vec![9], "p2"), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.gifs.search_results.len(), 1);
    assert_eq!(sent_request(&recorder, "downloadFile")["file_id"], 109);
    let more = driver.more_gif_search_results().unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "getInlineQueryResults")["offset"],
        "p2"
    );
    driver
        .ingest(
            copy_and_parse(
                &page(json!(more.as_extra()), vec![9, 10], "p2"),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.gifs.search_results.len(), 2);
    assert!(driver.session.gifs.search_next_offset.is_empty());
    assert_eq!(driver.more_gif_search_results().unwrap(), None);
    let old_saved = driver.show_saved_gifs().unwrap().unwrap();
    let add = driver.set_gif_saved(FileId(9), true).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "addSavedAnimation")["animation"]["id"],
        9
    );
    assert_eq!(driver.set_gif_saved(FileId(9), false).unwrap(), None);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":add.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    let fresh_saved = sent_request(&recorder, "getSavedAnimations")["@extra"].clone();
    assert_ne!(fresh_saved, json!(old_saved.as_extra()));
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"animations","@extra":old_saved.as_extra(),"animations":[]})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(!driver.session.gifs.loaded);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"animations","@extra":fresh_saved,"animations":[animation(9)]})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    let remove = driver.set_gif_saved(FileId(9), false).unwrap().unwrap();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"error","@extra":remove.as_extra(),"code":500,"message":"test"})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.gifs.animations.len(), 1);
    assert!(driver.session.gifs.failed);
    let remove = driver.set_gif_saved(FileId(9), false).unwrap().unwrap();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":remove.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    let final_saved = sent_request(&recorder, "getSavedAnimations")["@extra"].clone();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"animations","@extra":final_saved,"animations":[]}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    let count = count_saved();
    driver.ingest(copy_and_parse(&json!({"@type":"updateAnimationSearchParameters","provider":"Tenor","emojis":["🔥"]}).to_string(),&seq,&sink).unwrap()).unwrap();
    assert_eq!(count_saved(), count);
    assert_eq!(driver.session.gifs.provider_emojis, vec!["🔥"]);
    driver.close_gif_panel();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"updateSavedAnimations","animation_ids":[]}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.gifs.stale);
    assert!(driver.open_gif_panel().unwrap().is_some());
    let failed_fetch = sent_request(&recorder, "getSavedAnimations")["@extra"].clone();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"error","@extra":failed_fetch,"code":500,"message":"test"})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.gifs.failed);
    let count = count_saved();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"updateAnimationSearchParameters","provider":"Tenor","emojis":[]})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(count_saved(), count); // Errors also wait for an explicit retry.
    assert!(driver.open_gif_panel().unwrap().is_some());
    let old_saved = sent_request(&recorder, "getSavedAnimations")["@extra"].clone();
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"updateSavedAnimations","animation_ids":[]}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    let new_saved = sent_request(&recorder, "getSavedAnimations")["@extra"].clone();
    assert_ne!(old_saved, new_saved);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"animations","@extra":old_saved,"animations":[animation(99)]})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.gifs.animations.is_empty());
    driver.search_gifs("").unwrap();
    let old_page = sent_request(&recorder, "getInlineQueryResults")["@extra"].clone();
    driver.ingest(copy_and_parse(&json!({"@type":"updateOption","name":"animation_search_bot_username","value":{"@type":"optionValueString","value":"othergif"}}).to_string(),&seq,&sink).unwrap()).unwrap();
    assert_eq!(
        sent_request(&recorder, "searchPublicChat")["username"],
        "othergif"
    );
    assert_eq!(driver.session.gifs.search_bot_user_id, None);
    driver
        .ingest(copy_and_parse(&page(old_page, vec![99], ""), &seq, &sink).unwrap())
        .unwrap();
    assert!(driver.session.gifs.search_results.is_empty());
    driver.session.auth = crate::telegram::envelope::AuthorizationState::WaitPhoneNumber;
    assert!(driver.set_gif_saved(FileId(9), true).is_err());
    let _ = std::fs::remove_dir_all(dir);
}
