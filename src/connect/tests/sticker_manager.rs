use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| {
            serde_json::from_str::<Value>(json).is_ok_and(|value| value["@type"] == type_name)
        })
        .count()
}

fn answer(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    body: Value,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    driver
        .ingest(copy_and_parse(&body.to_string(), seq, sink).unwrap())
        .unwrap();
}

#[test]
fn closing_removes_sets_one_by_one_then_sends_the_order() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .commit_sticker_manager(false, vec![22, 44], Some(vec![33, 22, 11, 44]))
        .unwrap();
    let first = sent_request(&recorder, "changeStickerSet");
    assert_eq!(first["set_id"], "22");
    assert_eq!(first["is_installed"], false);
    assert_eq!(first["is_archived"], false);
    assert_eq!(count(&recorder, "changeStickerSet"), 1, "one at a time");
    assert_eq!(count(&recorder, "reorderInstalledStickerSets"), 0);
    answer(
        &mut driver,
        json!({"@type":"ok","@extra":first["@extra"]}),
        &seq,
        &sink,
    );
    let second = sent_request(&recorder, "changeStickerSet");
    assert_eq!(second["set_id"], "44");
    assert_eq!(count(&recorder, "reorderInstalledStickerSets"), 0);
    answer(
        &mut driver,
        json!({"@type":"ok","@extra":second["@extra"]}),
        &seq,
        &sink,
    );
    assert_eq!(
        sent_request(&recorder, "reorderInstalledStickerSets")["sticker_set_ids"],
        json!([33, 11])
    );
}

#[test]
fn a_failed_removal_does_not_stop_the_rest() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .commit_sticker_manager(false, vec![22], Some(vec![33, 22, 11]))
        .unwrap();
    let removal = sent_request(&recorder, "changeStickerSet");
    answer(
        &mut driver,
        json!({"@type":"error","@extra":removal["@extra"],"code":500,"message":"test"}),
        &seq,
        &sink,
    );
    assert_eq!(
        sent_request(&recorder, "reorderInstalledStickerSets")["sticker_set_ids"],
        json!([33, 11])
    );
}

#[test]
fn an_untouched_box_sends_nothing() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let before = recorder.snapshot().len();
    driver.commit_sticker_manager(false, vec![], None).unwrap();
    assert_eq!(recorder.snapshot().len(), before);
}

#[test]
fn a_reorder_alone_goes_out_at_once() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .commit_sticker_manager(false, vec![], Some(vec![3, 1, 2]))
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "reorderInstalledStickerSets")["sticker_set_ids"],
        json!([3, 1, 2])
    );
}

#[test]
fn mask_changes_use_the_mask_requests() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver
        .commit_sticker_manager(true, vec![5], Some(vec![7, 5, 6]))
        .unwrap();
    let removal = sent_request(&recorder, "changeStickerSet");
    assert_eq!(removal["set_id"], "5");
    answer(
        &mut driver,
        json!({"@type":"ok","@extra":removal["@extra"]}),
        &seq,
        &sink,
    );
    let reorder = sent_request(&recorder, "reorderInstalledStickerSets");
    assert_eq!(reorder["sticker_set_ids"], json!([7, 6]));
    assert_eq!(reorder["sticker_type"]["@type"], "stickerTypeMask");
}

#[test]
fn viewing_trending_sends_only_the_unread_sets_and_clears_their_dots() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let set = |id: i64, viewed: bool| crate::telegram::envelope::StickerSetInfo {
        id,
        title: String::new(),
        name: String::new(),
        size: 1,
        is_installed: false,
        is_official: false,
        is_owned: false,
        covers: Vec::new(),
        cover_files: Vec::new(),
        is_viewed: viewed,
    };
    driver.session.stickers.stickers.trending = vec![set(1, true), set(2, false), set(3, false)];
    assert!(driver.view_shown_trending_stickers().unwrap().is_some());
    assert_eq!(
        sent_request(&recorder, "viewTrendingStickerSets")["sticker_set_ids"],
        json!([2, 3])
    );
    assert!(
        driver
            .session
            .stickers
            .stickers
            .trending
            .iter()
            .all(|set| set.is_viewed)
    );
    assert_eq!(driver.view_shown_trending_stickers().unwrap(), None);
}
