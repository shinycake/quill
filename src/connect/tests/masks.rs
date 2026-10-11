use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::MasksTab;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn sets_answer(extra: crate::ids::RequestId, ids: &[i64]) -> String {
    let sets: Vec<Value> = ids
        .iter()
        .map(|id| {
            json!({"@type":"stickerSetInfo","id":id.to_string(),"title":format!("Set {id}"),
            "name":format!("set{id}"),"is_installed":true,"is_archived":false,"is_official":false,
            "sticker_type":{"@type":"stickerTypeMask"},"size":2,"covers":[]})
        })
        .collect();
    json!({"@type":"stickerSets","@extra":extra.as_extra(),"total_count":ids.len(),"sets":sets})
        .to_string()
}

fn count_sent(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| serde_json::from_str::<Value>(json).unwrap()["@type"] == type_name)
        .count()
}

#[test]
fn masks_dialog_requests_mask_lists_and_serializes_changes() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    let installed = driver.open_mask_sets().unwrap().unwrap();
    let sent = sent_request(&recorder, "getInstalledStickerSets");
    assert_eq!(sent["sticker_type"]["@type"], "stickerTypeMask");
    assert!(driver.session.stickers.masks.open);
    // A second open while the first is in flight sends nothing new.
    assert_eq!(driver.select_mask_tab(MasksTab::Installed).unwrap(), None);

    driver
        .ingest(copy_and_parse(&sets_answer(installed, &[11, 12]), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.stickers.masks.installed.len(), 2);
    assert_eq!(count_sent(&recorder, "getInstalledStickerSets"), 1);

    let archived_request = driver.select_mask_tab(MasksTab::Archived).unwrap().unwrap();
    let archived = sent_request(&recorder, "getArchivedStickerSets");
    assert_eq!(archived["sticker_type"]["@type"], "stickerTypeMask");
    assert_eq!(archived["offset_sticker_set_id"], "0");
    driver
        .ingest(copy_and_parse(&sets_answer(archived_request, &[21]), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.stickers.masks.archived.len(), 1);

    let reorder = driver.reorder_mask_set(12, 11).unwrap().unwrap();
    let sent = sent_request(&recorder, "reorderInstalledStickerSets");
    assert_eq!(sent["sticker_type"]["@type"], "stickerTypeMask");
    assert_eq!(sent["sticker_set_ids"], json!([12, 11]));
    // One change at a time; unknown sets are rejected.
    assert_eq!(driver.change_mask_set(11, false, true).unwrap(), None);
    assert!(driver.reorder_mask_set(12, 99).is_err());
    assert!(driver.change_mask_set(11, true, true).is_err());

    // Confirmed: both lists are dropped and the open (archived) tab is fetched again.
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type":"ok","@extra":reorder.as_extra()}).to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(!driver.session.stickers.masks.installed_loaded);
    assert_eq!(count_sent(&recorder, "getArchivedStickerSets"), 2);

    driver.change_mask_set(11, false, true).unwrap().unwrap();
    let change = sent_request(&recorder, "changeStickerSet");
    assert_eq!(change["set_id"], "11");
    assert_eq!(change["is_installed"], false);
    assert_eq!(change["is_archived"], true);
}

#[test]
fn editor_strip_loads_the_first_mask_sets_once() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    driver.load_editor_masks().unwrap();
    assert!(!driver.session.stickers.masks.open);
    let installed = sent_request(&recorder, "getInstalledStickerSets");
    let extra = installed["@extra"].as_str().unwrap().to_string();
    let answer = sets_answer(crate::ids::RequestId(0), &[11, 12, 13, 14, 15, 16])
        .replace("\"@extra\":\"0\"", &format!("\"@extra\":\"{extra}\""));
    driver
        .ingest(copy_and_parse(&answer, &seq, &sink).unwrap())
        .unwrap();
    // Four sets are loaded for the strip, each asked for once.
    assert_eq!(count_sent(&recorder, "getStickerSet"), 4);
    driver.load_editor_masks().unwrap();
    assert_eq!(count_sent(&recorder, "getStickerSet"), 4);

    driver.release_editor_masks();
    assert!(!driver.session.stickers.masks.strip_wanted);
}
